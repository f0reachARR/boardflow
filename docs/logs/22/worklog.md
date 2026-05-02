# Issue #22: Worker: Run Resultコメント作成ジョブハンドラ実装 - 作業ログ

## 経緯

- ハンドラ実装 (`create_run_result_comment.rs`, `comment_body.rs`, `dispatcher.rs`, `import.rs`) は完了済み
- `comment_body.rs` のユニットテスト (`should_post_run_result` 系) も完了済み
- **統合テスト `crates/worker/tests/run_result_comment_test.rs` が未作成**

## ユーザー要望

- 10件のテストケースをカバーする統合テストファイルを作成する
- 既存テスト (`dashboard_comment_test.rs`, `create_issue_test.rs`) のパターンに準拠

## 調査結果

### ハンドラのコードパス分析 (`create_run_result_comment.rs`)

1. `board_project_id` 欠損 → `Failed`
2. `board_run_id` 欠損 → `Failed`
3. board_project 見つからない → `Failed`
4. issue_number 未設定 → `Reschedule(5s)`
5. get_issue で Issue Closed:
   - `recreate_issue_on_update=false` → `Completed`
   - `recreate_issue_on_update=true` + tree_hash変更なし → `Completed`
   - `recreate_issue_on_update=true` + tree_hash変更あり → issue_history挿入, clear_issue_info, enqueue create_issue, `Reschedule(5s)`
6. get_issue で 404 → clear_issue_info, enqueue create_issue, `Reschedule(5s)`
7. get_issue でその他エラー → `handle_github_error`
8. current_run 見つからない → `Failed`
9. `should_post_run_result` が false → `Completed` (スキップ)
10. `should_post_run_result` が true → create_comment API呼び出し
    - 成功 → `Completed`
    - RateLimited → `Reschedule(retry_after)`
    - その他エラー → `Reschedule(backoff)`

### should_post_run_result ロジック

- previous run なし (初回) → false (コメントしない)
- ERC: Passed→Failed or Failed→Passed → true
- DRC: Passed→Failed or Failed→Passed → true
- current.erc_errors > prev.erc_errors → true
- current.drc_errors > prev.drc_errors → true
- それ以外 → false

### 既存テストのパターン

- `#[tokio::test]`, `#[ignore]`, `#[serial]`
- `get_pool()` → `Option<PgPool>`
- `setup_test_data(pool)` → `(repo_id, bp_id, run_id, installation_id)`
- `setup_with_issue(pool)` → issue_number設定済み
- `setup_with_two_runs(pool)` → prev_run + current_run

## 実装 (2026-05-02)

### フェーズ: 統合テスト作成

#### 作成ファイル
- `crates/worker/tests/run_result_comment_test.rs` (新規)

#### テストケース一覧 (10件)
| # | テスト名 | 検証内容 |
|---|----------|----------|
| 1 | `test_run_result_comment_success` | ERC passed→failed で create_comment 呼出、Completed |
| 2 | `test_run_result_comment_skip_first_run` | 初回run (prev無し) → Completed (スキップ) |
| 3 | `test_run_result_comment_skip_no_change` | 両run passed → Completed (スキップ) |
| 4 | `test_run_result_comment_missing_board_project_id` | board_project_id=None → Failed |
| 5 | `test_run_result_comment_missing_board_run_id` | board_run_id=None → Failed |
| 6 | `test_run_result_comment_no_issue` | issue_number未設定 → Reschedule(5s) |
| 7 | `test_run_result_comment_issue_closed_recreate` | Closed+recreate+tree_hash変更 → Reschedule + create_issue enqueued |
| 8 | `test_run_result_comment_issue_closed_no_recreate` | Closed+recreate=false → Completed |
| 9 | `test_run_result_comment_issue_404` | 404 → Reschedule + issue_history保存 + issue_number cleared |
| 10 | `test_run_result_comment_rate_limited` | RateLimited(60s) → Reschedule(backoff>=60) |

#### テスト結果
- `cargo check` コンパイル成功 (EXIT:0)

#### 次ステップ
- DATABASE_URL設定済み環境で `cargo test -p boardflow-worker --test run_result_comment_test -- --ignored` 実行
- 全テスト PASS 確認後、PR作成
- `cleanup_test_data(pool, repo_id, bp_id)`
- `handler_result_debug(&result)` → debug出力
- `MockGitHubClient` with configurable results
- `make_job(type, bp_id, run_id)` → GithubJob
- `make_config()` → WorkerConfig

---

## 実装計画

### 目的

`create_run_result_comment` ハンドラの統合テストを作成し、全主要コードパスの正常動作を検証する。

### 非目的

- ハンドラ本体のコード変更
- `should_post_run_result` のユニットテスト追加 (既存)
- 他のハンドラのテスト修正

### 受け入れ条件

- `crates/worker/tests/run_result_comment_test.rs` が存在する
- `cargo test -p boardflow-worker --test run_result_comment_test -- --ignored` で全テストPASS
- 10件のテストケースすべてがカバーされる
- `cargo clippy --workspace` が警告なしで通る

### 詳細要件

#### テストケース一覧

| # | テスト名 | セットアップ | 期待結果 |
|---|----------|-------------|----------|
| 1 | `test_run_result_comment_success` | 2 runs: prev=passed, cur=failed | Completed + create_comment呼出 |
| 2 | `test_run_result_comment_skip_first_run` | 1 run only (no prev) | Completed (スキップ) |
| 3 | `test_run_result_comment_skip_no_change` | 2 runs: both passed | Completed (スキップ) |
| 4 | `test_run_result_comment_missing_board_project_id` | job.board_project_id=None | Failed |
| 5 | `test_run_result_comment_missing_board_run_id` | job.board_run_id=None | Failed |
| 6 | `test_run_result_comment_no_issue` | issue_number=NULL | Reschedule |
| 7 | `test_run_result_comment_issue_closed_recreate` | issue closed + recreate=true + tree_hash変更 | Reschedule + create_issue enqueued |
| 8 | `test_run_result_comment_issue_closed_no_recreate` | issue closed + recreate=false | Completed |
| 9 | `test_run_result_comment_issue_404` | get_issue returns NotFound | Reschedule + issue_number cleared |
| 10 | `test_run_result_comment_rate_limited` | create_comment returns RateLimited | Reschedule(60s) |

#### テストファイル構造

```
run_result_comment_test.rs
├── imports (boardflow_domain, boardflow_github, boardflow_worker, chrono, serial_test, sqlx, uuid)
├── MockGitHubClient struct
│   ├── get_issue_result: Mutex<Option<Result<...>>>
│   ├── create_comment_result: Mutex<Option<Result<...>>>
│   └── captured_comment_body: Mutex<Option<String>>
├── impl MockGitHubClient
│   ├── default_success() → Open issue + create_comment Ok
│   ├── with_get_issue(result) → builder
│   └── with_create_comment(result) → builder
├── impl GitHubAppClient for MockGitHubClient
├── make_config() → WorkerConfig
├── make_job(type, bp_id, run_id) → GithubJob
├── get_pool() → Option<PgPool>
├── setup_with_two_runs_erc_change(pool)
│   → repo + bp(issue set) + prev_run(erc=passed) + cur_run(erc=failed)
│   → returns (repo_id, bp_id, prev_run_id, cur_run_id, installation_id)
├── setup_with_single_run(pool)
│   → repo + bp(issue set) + single run (no prev)
│   → returns (repo_id, bp_id, run_id, installation_id)
├── setup_with_two_runs_no_change(pool)
│   → repo + bp(issue set) + prev_run(passed) + cur_run(passed)
├── cleanup_test_data(pool, repo_id, bp_id)
├── handler_result_debug(result) → String
└── 10 test functions
```

### 影響範囲

- 新規ファイル: `crates/worker/tests/run_result_comment_test.rs`
- 既存コード変更なし

### 設計方針

- `dashboard_comment_test.rs` のパターンを踏襲
- MockGitHubClient は create_issue/update_comment を panic にする (このハンドラでは直接呼ばないため)
- DB セットアップでは `erc_status`/`drc_status`/`erc_errors`/`drc_errors` を明示的に設定
- `find_previous_completed` が正しく prev_run を返すよう `completed_at` でタイムスタンプ順序を保証

### テスト観点

- ハンドラの全分岐パスがカバーされている
- `should_post_run_result` の判定結果が統合テストレベルで正しく反映される
- Issue再作成フローで `issue_number` がクリアされ `create_issue` ジョブが enqueue される
- Rate limiting時のbackoff値が正しい (retry_after_secs の値が使われる)

### ドキュメント更新対象

- `docs/logs/22/worklog.md` (本ファイル) のみ

### 実装要否

`implementation_required`

### 未解決の疑問

なし — ハンドラ実装と既存テストパターンから全情報が揃っている。

### 実装手順

1. `crates/worker/tests/run_result_comment_test.rs` を作成
2. MockGitHubClient, ヘルパー関数 (setup系, cleanup, make_job, make_config, handler_result_debug) を実装
3. テストケース 4, 5 (Missing fields) を実装 — DB不要
4. テストケース 6 (No issue) を実装
5. テストケース 2 (Skip first run) を実装
6. テストケース 3 (Skip no change) を実装
7. テストケース 1 (Success) を実装
8. テストケース 7 (Issue closed + recreate) を実装
9. テストケース 8 (Issue closed + no recreate) を実装
10. テストケース 9 (Issue 404) を実装
11. テストケース 10 (Rate limited) を実装
12. `cargo clippy` + `cargo test` で検証
13. コミット & プッシュ

---

## 作業ステータス

- [x] 計画策定
- [ ] 実装
- [ ] テスト実行確認
- [ ] レビュー
- [ ] マージ

## レビュー (2026-05-02)

### Issueまでの経緯

- Issue #22 の対象は、既存の `create_run_result_comment` ハンドラ実装に対する統合テスト追加と、spec 12.2 / 12.3 / 13.1 / rate limit 対応の観点でのレビュー。
- レビュー対象として `crates/worker/src/handlers/create_run_result_comment.rs`、`crates/worker/src/comment_body.rs`、`crates/worker/tests/run_result_comment_test.rs`、`crates/worker/src/dispatcher.rs`、`crates/worker/src/handlers/import.rs`、`docs/spec.md` を確認。

### 今回のユーザー要望

- 統合テスト 10 件が仕様を十分に網羅しているかを確認する。
- テストコードの race condition / cleanup 漏れの有無を確認する。
- ハンドラ本体が spec 12.2, 12.3, 13.1 と整合しているかを確認する。

### 今回の調査結果

- `create_run_result_comment` ハンドラ本体は、missing IDs、issue 未作成、closed issue、404、rate limit、初回 run スキップ、状態変化時コメント作成の主要分岐を実装している。
- `should_post_run_result()` のユニットテストは、初回 run スキップ、pass→fail、fail→pass、新規 error 増加、変化なしを個別にカバーしている。
- 統合テスト 10 件は正常系、初回 skip、変化なし skip、missing IDs、issue 未作成、closed recreate、closed no recreate、404、rate limit を確認している。
- Web 調査では、GitHub REST API の rate limit 対応は `Retry-After` があればそれに従い、無ければ少なくとも 1 分待機し、継続失敗時は指数バックオフする方針が推奨されていることを確認。

### 実装内容の評価

- ハンドラ本体の仕様整合性は概ね良好。closed issue で `recreate_issue_on_update=false` は停止、`404` は issue 情報クリア + `create_issue` enqueue、rate limit は reschedule となっており、ユーザー提示の観点 2, 3, 4 には沿っている。
- `dispatcher.rs` には `create_run_result_comment` のルーティングがあり、`import.rs` でも import 完了後に同ジョブが enqueue されるため、ジョブフロー自体は 13.1 と整合している。
- テストは `serial_test` を使い、`cleanup_test_data()` で作成データを消しており、明白な race condition や cleanup 漏れは見当たらない。

### 今回のテスト結果

- `export DATABASE_URL="postgresql://boardflow:boardflow@localhost:5432/boardflow" && cargo test -p boardflow-worker --test run_result_comment_test -- --ignored` を再実行し、10 件すべて PASS を確認。
- `cargo clippy -p boardflow-worker --tests -- -D warnings` を再実行し、成功を確認。

### レビュー結果

- PR作成可否: `pr_ready: false`

#### 重大度順の指摘

1. `spec 13.1` の closed issue 分岐にある「`recreate_issue_on_update=true` かつ `tree_hash` 変更ありでのみ再作成する」という条件が、統合テストでは未検証。
    - `crates/worker/src/handlers/create_run_result_comment.rs` では `tree_hash_changed()` 分岐が実装されているが、`crates/worker/tests/run_result_comment_test.rs` の closed issue 系は changed case と `recreate=false` しかなく、unchanged case がない。
    - このままだと、closed issue を常に再作成してしまう回帰が入っても今回の 10 テストでは検知できない。

2. `spec 12.2` の本文フォーマットに対する回帰テストが不足している。
    - 実装側の `run_result_comment()` は marker、Commit、Run URL、Diff URL、ERC/DRC table を生成している。
    - ただし統合テスト成功ケースは `create_comment` が呼ばれたことしか見ておらず、本文内容を検証していない。
    - 既存のユニットテスト `test_run_result_comment_contains_markers()` も marker と一部結果表示のみで、Run URL、Diff URL、table header までは確認していない。
    - ユーザーが明示した 12.2 の確認項目に対して、実装は満たしていてもテスト網羅としては不足している。

### 必須修正

1. `crates/worker/tests/run_result_comment_test.rs` に、Issue closed + `recreate_issue_on_update=true` + `tree_hash` unchanged のケースを追加し、`Completed` かつ `create_issue` 非 enqueue を確認すること。
2. `crates/worker/tests/run_result_comment_test.rs` の success ケース、または `crates/worker/src/comment_body.rs` のユニットテストで、Run Result 本文に marker、Commit、Run URL、Diff URL、`| Check | Result |` が含まれることを直接検証すること。

### 任意改善

1. `404` と closed recreate のテストで、`board_project_issue_history.reason` が期待値 (`deleted` / `recreated`) になっていることまで確認すると、履歴保存の回帰に強くなる。
2. success ケースで ERC だけでなく DRC 変化、fail→pass、新規 error 増加を統合テスト側にも 1 ケースずつ足すと、ユニットテストと統合テストの責務分担がより明確になる。

### テスト不足

- closed issue + unchanged tree hash の未検証。
- Run Result コメント本文の必須要素に対する直接検証の不足。
- integration レベルでは fail→pass と新規 error 増加の投稿条件は未検証。

### ドキュメント確認

- `docs/spec.md` の 12.2, 12.3, 13.1 を確認。
- `docs/logs/22/worklog.md` の既存内容には「統合テスト未作成」「次ステップでテスト実行確認」といった過去状態が残っており、今回の実装完了・テスト PASS 状態とは一致していない。

### PR/完了結果

- 現時点では `pr_ready: false`。
- 理由は、実装の致命的不整合ではなく、ユーザーがレビュー観点として明示した spec 12.2 / 13.1 に対する回帰テストがまだ 2 点不足しているため。

### 残リスク

- closed issue の tree_hash unchanged 分岐が将来壊れても現状テストでは検知できない。
- コメント本文の URL / table 生成が壊れても現状テストでは検知できない。

---

## ドキュメント確認 (2026-05-02, docs review)

### docs review の調査結果

- `docs/logs/22/worklog.md` 内の過去レビュー節は現行実装に対して stale であり、最新状態をそのまま表していない。
- 現行の統合テスト `crates/worker/tests/run_result_comment_test.rs` は 10 件ではなく 11 件ある。
- 追加済みの 11 件目は `test_run_result_comment_issue_closed_tree_hash_unchanged` で、spec 13.1 の「closed issue かつ tree_hash unchanged では再作成しない」を確認している。
- success ケースではコメント本文に marker、Commit、Run URL、Diff URL、`| Check | Result |` が含まれることを検証しており、spec 12.2 の主要要素に対応している。
- `docs/spec.md` の 12.2、12.3、13.1 と `docs/backend/summary.md` の Run Result コメント方針・closed issue 方針には、今回の実装と矛盾する記述は見当たらない。

### docs review の対応内容

- ドキュメントレビューとして、Issue #22 の worklog に現行状態の確認結果を追記した。
- 過去節に残る「10件テスト」「必須修正 2 点」「`pr_ready: false`」は当時のレビュー記録として残るが、現時点の最終判定は本節を優先する。

### docs review 時点のテスト結果

- ユーザー提示の最新結果として、`crates/worker/tests/run_result_comment_test.rs` は 11 tests all passed。
- `cargo clippy --tests -- -D warnings` は PASS。

### docs review の判定

- `docs_ready: true`

### docs review のドキュメント確認

- 修正が必要だったのは `docs/logs/22/worklog.md` の最新状態反映のみ。
- `docs/spec.md` の更新は不要。
- `docs/backend/summary.md` の更新は不要。

### docs review の PR/完了結果

- Issue #22 について、ドキュメント観点では PR 作成可。

### docs review の残リスク

- worklog 内には履歴として stale な過去レビュー節が残るため、読む側は本節を最新判定として参照する必要がある。

---

## 2回目レビュー後の最終判定 (2026-05-02)

### 最終確認

- `pr_ready: true`（レビュー指摘 2 点を fix コミットで対応済み）
  - tree_hash unchanged テスト (`test_run_result_comment_issue_closed_tree_hash_unchanged`) 追加済み
  - success テストでコメント本文の必須要素 (marker, Commit, Run URL, Diff URL, `| Check | Result |`) 検証済み
- `docs_ready: true`（docs review 確認済み）
- テスト: 11 tests all passed、`cargo clippy --tests -- -D warnings` PASS

### PR/完了結果

- PR 作成: `feat/22-run-result-comment-handler` → `main`
- PR タイトル: `test(#22): create_run_result_comment ハンドラの統合テスト追加`
- Closes #22

### 残リスク

- fail→pass や DRC 変化、新規 error 増加の統合テストケースは任意改善として未追加（ユニットテストで個別検証済み）
- 404 / closed recreate の `board_project_issue_history.reason` 値の検証は任意改善として未追加
