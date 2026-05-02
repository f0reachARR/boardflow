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
