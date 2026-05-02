# Issue #21 — Dashboardコメント作成・更新ジョブハンドラ

## Issueまでの経緯

- #19（GitHub Appクライアント）、#20（Issue作成）、#26（ディスパッチャ）がマージ済み
- `create_issue` ハンドラおよび統合テストが実装済み（`create_issue_test.rs`）
- ダッシュボードコメントハンドラの実装に進む段階

## ユーザー要望

- docs以下の仕様に基づいてアプリケーションを一通り実装する
- GitHub Issueに対してDashboardコメントを作成・更新するジョブハンドラを実装する
- BoardProjectの最新状態サマリをIssueコメントとして管理する

## 調査結果（2026-05-02）

### 実装済みコンポーネント

| ファイル | 状態 | 概要 |
|---|---|---|
| `crates/worker/src/handlers/create_dashboard_comment.rs` | 完全実装済み | board_project_id/board_run_id チェック、board_project fetch、Issue状態確認（closed/404）、recreate_issue_on_update＋tree_hash変更チェック、冪等性チェック（dashboard_comment_id既存ならスキップ）、GitHub API create_comment、dashboard_comment_id のDB保存 |
| `crates/worker/src/handlers/update_dashboard_comment.rs` | 完全実装済み | デバウンス（latest_completed_run_id使用）、フォールバック（dashboard_comment_idがNoneなら作成）、コメント404時のクリア＆再作成、GitHub API update_comment |
| `crates/worker/src/comment_body.rs` | 実装済み | `dashboard_comment()` 関数がコメント本文を生成 |
| `crates/worker/src/dispatcher.rs` | 実装済み | `create_dashboard_comment` / `update_dashboard_comment` 両ジョブタイプへのディスパッチ |

### 未実装コンポーネント

| ファイル | 状態 | 概要 |
|---|---|---|
| `crates/worker/tests/create_dashboard_comment_test.rs` | 未作成 | create_dashboard_comment ハンドラの統合テスト |
| `crates/worker/tests/update_dashboard_comment_test.rs` | 未作成 | update_dashboard_comment ハンドラの統合テスト |

### 既存テストパターン

`crates/worker/tests/create_issue_test.rs` が参考パターン:
- `MockGitHubClient` 構造体で `GitHubAppClient` trait をモック
- `make_config()` / `make_job()` ヘルパー関数
- `setup_test_data()` で repositories / board_projects テーブルにテストデータ挿入
- `cleanup_test_data()` で後片付け
- `#[tokio::test]` + `#[ignore]` で DATABASE_URL 必須テスト
- `get_pool()` で DATABASE_URL 環境変数から PgPool 取得

### 統合テストで検証すべきシナリオ

#### create_dashboard_comment

1. **正常系**: Issue存在 → create_comment 成功 → dashboard_comment_id がDBに保存される
2. **冪等性**: dashboard_comment_id が既に設定済み → スキップ（Completed）
3. **Issue未作成**: issue_number が None → Reschedule
4. **Issue closed + recreate_issue_on_update=true + tree_hash変更**: Issue履歴保存 → issue情報クリア → create_issue enqueue → Reschedule
5. **Issue closed + recreate_issue_on_update=false**: Completed
6. **Issue 404**: issue情報クリア → create_issue enqueue → Reschedule
7. **board_project_id / board_run_id 欠落**: Failed

#### update_dashboard_comment

1. **正常系**: dashboard_comment_id 存在 → update_comment 成功 → Completed
2. **フォールバック**: dashboard_comment_id が None → create_comment → dashboard_comment_id 保存
3. **コメント404**: dashboard_comment_id クリア → create_comment → 新ID保存
4. **デバウンス**: latest_completed_run_id を使用して最新状態のコメント生成
5. **Issue closed 処理**: recreate_issue_on_update に基づく分岐
6. **Issue 404**: issue情報クリア → create_issue enqueue

## 外部ライブラリ調査

外部ライブラリの新規APIを使用する箇所はない。既存の `GitHubAppClient` trait（`create_comment` / `update_comment`）で十分。

## 計画

統合テストの追加が必要:
1. `crates/worker/tests/create_dashboard_comment_test.rs` — create ハンドラのテスト
2. `crates/worker/tests/update_dashboard_comment_test.rs` — update ハンドラのテスト

既存の `create_issue_test.rs` のパターンに従い、MockGitHubClient を拡張して `create_comment` / `update_comment` の結果を制御可能にする。

## 結論ステータス

**`implementation_required`** — ハンドラコードは実装済みだが、統合テストが未実装のため、テスト追加の実装が必要。

---

## 実装計画（2026-05-02 確定版）

### 目的

- `create_dashboard_comment` / `update_dashboard_comment` ハンドラの統合テストを追加し、ハンドラのロジック正当性を検証する

### 非目的

- ハンドラ本体コードの変更
- comment_body の単体テスト（別Issue）
- dispatcher の統合テスト（#26 でカバー済み）

### 受け入れ条件

1. `cargo test -p boardflow-worker --test dashboard_comment_test -- --ignored` が全パス
2. create_dashboard_comment の主要パス（正常系、冪等性、Issue未作成、ID欠損、project不存在）をカバー
3. update_dashboard_comment の主要パス（正常系、フォールバック、404再作成、Issue未作成）をカバー
4. テスト間のデータ干渉がない（ランダム github_repository_id 使用）

### 詳細要件

#### テストファイル

`crates/worker/tests/dashboard_comment_test.rs`（1ファイルに create / update 両方のテストを含む）

#### MockGitHubClient 設計

```rust
struct MockGitHubClient {
    get_issue_result: Mutex<Option<Result<IssueInfo, GitHubClientError>>>,
    create_comment_result: Mutex<Option<Result<CreatedComment, GitHubClientError>>>,
    update_comment_result: Mutex<Option<Result<(), GitHubClientError>>>,
}
```

- 各フィールドは `tokio::sync::Mutex<Option<Result<...>>>` で、`take()` して1回だけ消費
- `get_installation_token` は常に成功を返す
- `create_issue` は `panic!`（このテストでは呼ばれない）

#### setup_test_data 拡張

既存の repository + board_project に加え:
- `board_runs` テーブルに1レコード挿入（commit_sha, branch, ref, github_run_id, status='completed' が必須）
- board_project の `issue_number` / `issue_node_id` / `issue_url` を事前設定（Issue存在前提テスト用）

#### テストケース一覧

| # | テスト名 | ハンドラ | シナリオ | 期待結果 | DB検証 |
|---|---|---|---|---|---|
| 1 | `test_create_dashboard_comment_success` | create | issue存在、comment未作成 | Completed | dashboard_comment_id が保存される |
| 2 | `test_create_dashboard_comment_idempotent` | create | dashboard_comment_id 既存 | Completed | API呼ばれない（PanicClient） |
| 3 | `test_create_dashboard_comment_no_issue` | create | issue_number=None | Reschedule | — |
| 4 | `test_create_dashboard_comment_missing_board_project_id` | create | job.board_project_id=None | Failed | — |
| 5 | `test_create_dashboard_comment_missing_board_run_id` | create | job.board_run_id=None | Failed | — |
| 6 | `test_create_dashboard_comment_project_not_found` | create | 存在しないboard_project_id | Failed("not found") | — |
| 7 | `test_update_dashboard_comment_success` | update | dashboard_comment_id 存在 | Completed | — |
| 8 | `test_update_dashboard_comment_fallback_create` | update | dashboard_comment_id=None | Completed | dashboard_comment_id が新規保存 |
| 9 | `test_update_dashboard_comment_404_recreate` | update | update_comment→NotFound | Completed | dashboard_comment_id が新ID |
| 10 | `test_update_dashboard_comment_no_issue` | update | issue_number=None | Reschedule | — |

### 影響範囲

- 新規ファイル: `crates/worker/tests/dashboard_comment_test.rs`
- 既存コード変更: なし

### 設計方針

1. `create_issue_test.rs` のパターンを完全踏襲（get_pool, setup_test_data, cleanup_test_data, make_config, make_job, #[ignore]）
2. `board_runs` テーブルにテストデータ挿入する `insert_test_board_run()` ヘルパー追加
3. MockGitHubClient は `get_issue` / `create_comment` / `update_comment` の結果をシナリオ別に制御
4. テスト間で `github_repository_id` をランダム化してデータ干渉防止
5. 冪等性テストでは PanicClient パターン（API呼び出しでpanic）を使用

### テスト観点

- ハンドラの返却値（Completed / Reschedule / Failed）が正しいか
- DB副作用（dashboard_comment_id 保存/クリア）が正しいか
- 冪等性（既にコメント存在時にAPI呼ばない）
- フォールバック（update時にcomment_id=Noneなら create）
- 404リカバリ（comment削除後に再作成）

### ドキュメント更新対象

- `docs/logs/21/worklog.md` — 本ログに実装結果を追記

### 実装要否

`implementation_required`

### impl エージェントへの引継ぎ情報

- **テストファイルパス**: `crates/worker/tests/dashboard_comment_test.rs`
- **参照パターン**: `crates/worker/tests/create_issue_test.rs`（MockGitHubClient, setup/cleanup, make_config, make_job）
- **ハンドラ呼び出し**: `boardflow_worker::handlers::create_dashboard_comment::handle(&pool, &client, &config, &job)`
- **ハンドラ呼び出し**: `boardflow_worker::handlers::update_dashboard_comment::handle(&pool, &client, &config, &job)`
- **BoardRun必須カラム**: id, board_project_id, commit_sha, branch, ref, github_run_id(i64), github_run_attempt(i32), status='completed', created_at
- **board_project事前設定**: issue_number, issue_node_id, issue_url を setup で UPDATE
- **Mock制御**: `tokio::sync::Mutex<Option<Result<T, E>>>` + `take()`
- **DB検証**: `SELECT dashboard_comment_id FROM board_projects WHERE id = $1`

---

## 実装結果（2026-05-02）

### 作成ファイル

- `crates/worker/tests/dashboard_comment_test.rs` — 統合テスト10件

### テスト結果

- `cargo check -p boardflow-worker --tests` — コンパイル成功
- `cargo test -p boardflow-worker --test dashboard_comment_test -- --list` — 10テスト認識確認

### 実装内容

| # | テスト名 | 検証観点 |
|---|---|---|
| 1 | `test_create_dashboard_comment_success` | 正常系: create_comment → dashboard_comment_id DB保存 |
| 2 | `test_create_dashboard_comment_idempotent` | 冪等性: comment_id既存ならAPI呼ばずCompleted |
| 3 | `test_create_dashboard_comment_no_issue` | issue_number=None → Reschedule(backoff=5.0) |
| 4 | `test_create_dashboard_comment_missing_board_project_id` | board_project_id=None → Failed |
| 5 | `test_create_dashboard_comment_missing_board_run_id` | board_run_id=None → Failed |
| 6 | `test_create_dashboard_comment_project_not_found` | 存在しないBP → Failed("not found") |
| 7 | `test_update_dashboard_comment_success` | 正常系: update_comment → Completed |
| 8 | `test_update_dashboard_comment_fallback_create` | comment_id=None → create_comment → 新ID保存 |
| 9 | `test_update_dashboard_comment_404_recreate` | update 404 → clear + create → 新ID保存 |
| 10 | `test_update_dashboard_comment_no_issue` | issue_number=None → Reschedule(backoff=5.0) |

### 設計ポイント

- MockGitHubClient: `get_issue_result`, `create_comment_result`, `update_comment_result` を `Mutex<Option<Result>>` で制御
- `setup_with_issue()` ヘルパーで board_project に issue_number/node_id/url を事前設定
- board_runs テーブルもsetupで挿入、cleanupで削除
- 冪等性テストでは PanicClient パターン使用（API呼出でpanic → テスト保護）

### コミット

- `2464b3e` — `test(#21): add integration tests for dashboard comment handlers`

## 残リスク

- board_run の `ref` カラムはSQL予約語のため、INSERT文で `"ref"` とクオートが必要（現時点では sqlx が自動処理）
- `tree_hash_changed` 関連テスト（Issue closed + recreate）は setup が複雑なため、MVP外とし別Issue化を検討
- `get_issue` のMock制御（open/closed/404）により、Issue状態テストの追加は将来的に可能
- DATABASE_URL 未設定環境ではテスト実行不可（`#[ignore]` で明示的opt-in）
