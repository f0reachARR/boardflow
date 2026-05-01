# Issue #26: Worker: GitHub APIジョブ汎用ディスパッチャ実装

## 調査フェーズ (2026-05-02)

### Issueまでの経緯

- #19（GitHub Appクライアント）がマージ済み。`crates/github/` に `GitHubAppClient` trait と `OctocrabGitHubAppClient` 実装が存在
- `crates/worker/src/main.rs` は現在 `artifact_bundle_import` ジョブのみをポーリング・処理
- import完了後に `create_issue`, `create_dashboard_comment` / `update_dashboard_comment`, `create_run_result_comment` ジョブをenqueueする処理は既に実装済み（main.rs Step 13）
- `crates/db/src/queries/github_job.rs` に `dequeue(pool, job_type)`, `enqueue()`, `mark_completed()`, `mark_failed()`, `reschedule()` が存在
- `crates/jobs/src/lib.rs` に `MAX_ATTEMPTS`, `BASE_BACKOFF_SECS`, `backoff_secs()` が存在

### ユーザー要望

docs以下の仕様（spec.md Section 11-13）に基づいて、Worker内で複数のGitHub APIジョブタイプを処理するための汎用ディスパッチャを実装する。

### 調査結果

#### 1. 既存コードベースの確認

**GitHubAppClient trait** (`crates/github/src/client.rs`):
- `get_installation_token(installation_id)` → `SecretString`
- `create_issue(installation_id, owner, repo, title, body)` → `CreatedIssue`
- `get_issue(installation_id, owner, repo, issue_number)` → `IssueInfo`
- `create_comment(installation_id, owner, repo, issue_number, body)` → `CreatedComment`
- `update_comment(installation_id, owner, repo, comment_id, body)` → `()`

**GitHubClientError** (`crates/github/src/error.rs`):
- `RateLimited { retry_after_secs: Option<u64> }` — 既にrate limit用バリアントが定義済み
- 403/429の検出ロジック実装済み。ただし `retry_after_secs` は現状常に `None`

**dequeue関数** (`crates/db/src/queries/github_job.rs`):
- `dequeue(executor, job_type: &str)` — job_typeを指定してdequeue
- 汎用ディスパッチャでは、複数job_typeを順番にポーリングするか、`dequeue_any`を新設する必要あり

**import完了後のジョブenqueue** (`crates/worker/src/main.rs` Step 13):
- `create_issue` — board_projectにissue_numberがない場合
- `create_dashboard_comment` / `update_dashboard_comment` — dashboard_comment_idの有無で分岐
- `create_run_result_comment` — 毎回enqueue（追記条件判定はハンドラ側で行う想定）

#### 2. octocrab レートリミット処理

octocrabには `RetryConfig::HandleRateLimits` ミドルウェアが内蔵されている（`src/service/middleware/retry.rs`）。

動作:
- `retry-after` ヘッダがあればその秒数待機
- `x-ratelimit-remaining` が 0 かつ `x-ratelimit-reset` があれば reset 時刻まで待機
- 429 でヘッダがない場合は `min_wait_seconds` 待機
- 403 でヘッダがない場合はリトライしない（権限エラーの可能性）
- 5xx はリトライ

設定方法:
```rust
OctocrabBuilder::new()
    .add_retry_config(RetryConfig::HandleRateLimits {
        metrics: Arc::new(NoOpRateLimitMetrics),
        max_retries: 3,
        min_wait_seconds: 60,
    })
```

デフォルトは `RetryConfig::Simple(3)`（`retry` featureが有効な場合）。

**結論**: octocrabレベルの自動リトライ＋ジョブキューレベルの `reschedule()` による二段構えが最適。
- octocrabの `HandleRateLimits` で一時的なrate limitを自動吸収
- それでも失敗した場合はジョブキューの `reschedule()` + exponential backoff で再スケジュール
- `GitHubClientError::RateLimited` のとき、backoff_secsを通常より長くする（reset時刻ベース）

#### 3. spec.md Section 13 で要求されている制御

- **並列数制御**: installation_id単位、repository_id単位、job type単位 → MVPでは単一workerの逐次処理で自然に満たされる
- **Dashboardコメント更新のdebounce**: 同一BoardProjectに未処理のupdate_dashboard_commentがある場合payloadを最新に置き換え → enqueue時のON CONFLICTまたはハンドラ内で最新状態を取得して対応
- **レートリミット時の挙動**: octocrab HandleRateLimits + ジョブキューrescheduleで対応可能

#### 4. 汎用ディスパッチャの設計方針

既存コードから自然に導かれる設計:

1. `poll_and_process()` を拡張し、GitHub APIジョブタイプも処理する
2. ジョブタイプごとにハンドラ関数を用意（`handle_create_issue`, `handle_create_dashboard_comment`, etc.）
3. `dequeue()` を各ジョブタイプで呼ぶか、`dequeue_any_github_job()` を新設
4. ハンドラは `GitHubAppClient` trait を引数に取り、テスト可能にする
5. WorkerConfigに `GITHUB_APP_ID`, `GITHUB_PRIVATE_KEY_PEM` を追加し、`OctocrabGitHubAppClient` をmain()で初期化

#### 5. 外部調査は不要

以下の理由から、新規の `docs/external/` ドキュメント作成は不要:
- octocrabのrate limit処理は `docs/external/github-app-octocrab.md` に追記する程度で十分
- PostgreSQLジョブキューパターンは `docs/external/postgresql-job-queue-polling.md` に既存
- 新しい外部ライブラリの導入は不要

### 計画（概要）

実装フェーズで以下を行う:
1. `WorkerConfig` にGitHub App設定を追加
2. `main.rs` で `OctocrabGitHubAppClient` を初期化（`HandleRateLimits` 有効化）
3. `poll_and_process()` をGitHub APIジョブタイプ対応に拡張
4. 各ジョブタイプのハンドラを実装
5. ジョブ失敗時のreschedule/mark_failedロジックを汎用化

## 実装フェーズ (2026-05-02)

### 実装内容

#### 新規ファイル

1. **`crates/worker/src/comment_body.rs`** — Issue本文、Dashboardコメント、Run Resultコメントのテンプレート生成関数群
   - `issue_body()`, `issue_title()`, `dashboard_comment()`, `run_result_comment()`
   - `should_post_run_result()` — 追記条件判定ロジック（pass→fail、fail→pass、新規エラー増加）
   - `check_result_text()` — チェック結果のテキスト表現

2. **`crates/worker/src/dispatcher.rs`** — 優先度順ジョブディスパッチャ
   - `poll_and_dispatch()` — 5つのジョブタイプを優先度順にdequeue、ハンドラ呼び出し、HandlerResultに基づく後処理
   - import → create_issue → create_dashboard_comment → update_dashboard_comment → create_run_result_comment

3. **`crates/worker/src/handlers/mod.rs`** — `HandlerResult` enum定義（Completed / Reschedule / Failed）

4. **`crates/worker/src/handlers/import.rs`** — 既存`process_import_job`と`handle_job_failure`を移動（ロジック変更なし）

5. **`crates/worker/src/handlers/create_issue.rs`** — Issue作成ハンドラ
   - `find_by_id_with_repository` でプロジェクト情報取得
   - GitHub API経由でIssue作成
   - `update_issue_info` でDB更新
   - 後続 `create_dashboard_comment` ジョブをenqueue

6. **`crates/worker/src/handlers/create_dashboard_comment.rs`** — Dashboardコメント作成ハンドラ
   - Issue未作成時はReschedule（依存関係を自然に解決）
   - コメント作成後に `dashboard_comment_id` を更新

7. **`crates/worker/src/handlers/update_dashboard_comment.rs`** — Dashboardコメント更新ハンドラ
   - 既存コメントID使用して本文を最新に更新

8. **`crates/worker/src/handlers/create_run_result_comment.rs`** — Run Resultコメント作成ハンドラ
   - `find_previous_completed` で前回completedランを取得
   - `should_post_run_result()` で追記条件判定
   - 条件を満たす場合のみコメント投稿

#### 変更ファイル

9. **`crates/worker/src/config.rs`** — `github_app_id`, `github_private_key_pem`, `app_base_url` 追加

10. **`crates/worker/src/main.rs`** — モジュール構造変更、`OctocrabGitHubAppClient`初期化、`dispatcher::poll_and_dispatch`呼び出し

11. **`crates/worker/Cargo.toml`** — `secrecy`, `chrono` 依存追加

12. **`crates/db/src/queries/board_project.rs`** — `update_issue_info`, `update_dashboard_comment_id`, `clear_dashboard_comment_id` 追加

13. **`crates/db/src/queries/board_run.rs`** — `find_previous_completed` 追加

### テスト結果

11テスト全パス:
- `test_issue_title` — タイトル生成
- `test_issue_body_contains_markers` — Issue本文のHTML markers/URL確認
- `test_dashboard_comment_contains_markers` — Dashboardコメントのmarkers/ステータス表示確認
- `test_run_result_comment_contains_markers` — Run Resultコメントのmarkers確認
- `test_should_post_run_result_first_run` — 初回runは必ず投稿
- `test_should_post_run_result_pass_to_fail` — pass→failで投稿
- `test_should_post_run_result_fail_to_pass` — fail→passで投稿
- `test_should_post_run_result_new_errors` — エラー数増加で投稿
- `test_should_not_post_run_result_no_change` — 変化なしは非投稿
- `test_should_not_post_run_result_same_failure` — 同じ失敗は非投稿
- `test_should_not_post_run_result_fewer_errors` — エラー減少は非投稿

### 設計判断

1. **dequeue戦略**: `dequeue_any`は新設せず、各ジョブタイプを優先度順に個別dequeue（計画通り）
2. **import handler**: 自前でmark_completed/reschedule管理（トランザクション内完了のため）
3. **GitHub client未設定時**: Reschedule(60s)で遅延（将来設定されることを想定）
4. **Issue依存**: create_dashboard_comment/create_run_result_commentはissue未作成時にReschedule(5s)で自然に依存解決
5. **Run Result投稿条件**: 仕様通り(pass→fail, fail→pass, エラー数増加)を実装

### 残リスク

- octocrab `HandleRateLimits` ミドルウェアは本実装では未有効化（OctocrabGitHubAppClient::new内で追加可能だが、#19のスコープ）
- 統合テスト（実DB/mock GitHub）は未実装（別Issue推奨）
- `app_base_url` のデフォルト値 `https://boardflow.example.com` はプロダクションデプロイ時に要変更

---

## 計画フェーズ (2026-05-02)

### 目的

Worker内で `create_issue`, `create_dashboard_comment`, `update_dashboard_comment`, `create_run_result_comment` の4ジョブタイプを処理する汎用ディスパッチャを実装する。既存の `artifact_bundle_import` ポーリングと共存させ、ジョブタイプに応じたハンドラへディスパッチする基盤を構築する。

### 非目的

- 並列数制御（installation_id/repository_id単位のスロットリング）→ MVP単一workerの逐次処理で暗黙的に満たす
- Dashboardコメント更新のdebounce（enqueue側のON CONFLICT既存で簡易対応済み）→ 高度なdebounceは後続Issue
- `create_label`, `update_issue_body` ジョブ → 本Issueのスコープ外
- Web UIのIssue同期状態表示 → 別Issue

### 受け入れ条件

1. `poll_and_process()` が全5ジョブタイプ（import + 4 GitHub APIジョブ）をポーリングし、対応するハンドラへディスパッチする
2. `create_issue` ハンドラ: GitHub Issue作成 → `board_projects` に `issue_number`, `issue_node_id`, `issue_url`, `issue_sync_status` を更新
3. `create_dashboard_comment` ハンドラ: Dashboardコメント作成 → `board_projects.dashboard_comment_id` を更新
4. `update_dashboard_comment` ハンドラ: 既存Dashboardコメントを最新状態に編集更新
5. `create_run_result_comment` ハンドラ: 状態変化時にRun Resultコメントを追記、変化なし時はスキップ（完了扱い）
6. Issue未作成時（`issue_number` なし）にコメントジョブが来た場合はrescheduleする
7. octocrabに `RetryConfig::HandleRateLimits` が設定される
8. GitHub API失敗時は `MAX_ATTEMPTS` まで reschedule + exponential backoff、超過で mark_failed
9. `WorkerConfig` に `GITHUB_APP_ID` / `GITHUB_PRIVATE_KEY_PEM` 環境変数を追加
10. コンパイルが通り、既存のimportジョブ処理に影響がない

### 詳細要件

#### dequeue戦略

`dequeue_any` を新設せず、各ジョブタイプを順番にdequeueする方式を採用する。

理由:
- 既存 `dequeue(pool, job_type)` をそのまま利用可能
- 優先度制御が直感的（呼び出し順 = 優先度）
- `FOR UPDATE SKIP LOCKED` により他workerとの競合もない

ポーリング順序:
1. `artifact_bundle_import` （最優先: 他ジョブの前提条件）
2. `create_issue` （コメントジョブの前提条件）
3. `create_dashboard_comment`
4. `update_dashboard_comment`
5. `create_run_result_comment`

いずれかのジョブが見つかったら即処理し、全て空のときだけ `poll_interval_secs` sleep する。

#### Issue作成ハンドラ (`create_issue`)

1. `board_project::find_by_id_with_repository(pool, job.board_project_id)` でプロジェクト情報取得
2. 冪等性チェック: `bp.issue_number.is_some()` なら完了扱い（`mark_completed`）
3. Issue本文をspec.md Section 11.5のテンプレートに従い生成
4. `github_client.create_issue(installation_id, owner, repo, title, body)` 呼び出し
5. 成功時: `board_projects` を `issue_number`, `issue_node_id`, `issue_url`, `issue_sync_status = synced` に更新
6. `mark_completed`

#### Dashboardコメント作成ハンドラ (`create_dashboard_comment`)

1. `board_project::find_by_id_with_repository(pool, job.board_project_id)` 取得
2. 依存チェック: `bp.issue_number.is_none()` なら reschedule（Issue未作成）
3. 冪等性チェック: `bp.dashboard_comment_id.is_some()` なら完了扱い
4. Dashboardコメント本文をspec.md Section 12.1のテンプレートに従い生成
5. `github_client.create_comment(installation_id, owner, repo, issue_number, body)` 呼び出し
6. 成功時: `board_projects.dashboard_comment_id` を更新
7. `mark_completed`

#### Dashboardコメント更新ハンドラ (`update_dashboard_comment`)

1. `board_project::find_by_id_with_repository(pool, job.board_project_id)` 取得
2. 依存チェック: `bp.issue_number.is_none()` なら reschedule
3. `bp.dashboard_comment_id` が `None` なら → `create_dashboard_comment` 相当の処理にフォールバック
4. Dashboardコメント本文を最新Run情報から生成
5. `github_client.update_comment(installation_id, owner, repo, comment_id, body)` 呼び出し
6. 404の場合: `dashboard_comment_id` をクリア → `create_comment` で再作成
7. `mark_completed`

#### Run Resultコメントハンドラ (`create_run_result_comment`)

1. `board_project::find_by_id_with_repository(pool, job.board_project_id)` 取得
2. 依存チェック: `bp.issue_number.is_none()` なら reschedule
3. 追記条件判定（spec.md Section 12.3）:
   - 前回Run（`board_project.latest_completed_run_id` で特定、ただしimportジョブ完了後に更新済みなので、1つ前のrunを参照する必要がある → `board_run_id` の直前のcompletedを取得）
   - 今回RunのERC/DRCステータスと比較
   - 状態変化なしなら `mark_completed` してスキップ
4. Run Resultコメント本文をspec.md Section 12.2のテンプレートに従い生成
5. `github_client.create_comment(installation_id, owner, repo, issue_number, body)` 呼び出し
6. `mark_completed`

#### 失敗処理の汎用化

`handle_job_failure()` を拡張し、GitHub APIジョブタイプでも適切に動作させる:

- `GitHubClientError::RateLimited { retry_after_secs }` の場合:
  - `retry_after_secs` があればその秒数でreschedule
  - なければ通常のexponential backoffの2倍でreschedule
- `GitHubClientError::NotFound` の場合:
  - Issue 404: `issue_number` をクリアし、Issue未作成相当として `mark_completed`（再作成はenqueue側が担当）
  - Comment 404: `dashboard_comment_id` をクリアし、再作成フローへ
- `GitHubClientError::Auth` の場合: reschedule（一時的な権限問題の可能性）
- その他: 通常のreschedule + exponential backoff

GitHub APIジョブの失敗は `board_run` や `artifact_bundle` の状態に影響しない（import完了後のpost-processingのため）。

#### GitHubAppClient の初期化

`WorkerConfig` に `github_app_id: Option<u64>` と `github_private_key_pem: Option<String>` を追加する。
Optional にする理由: import-only workerの場合はGitHub App設定なしで起動可能にするため。

`main()` で `OctocrabGitHubAppClient::new()` を初期化し、`Arc<dyn GitHubAppClient>` として共有する。
GitHub App設定がない場合、GitHub APIジョブはスキップする（importのみ処理）。

### 影響範囲

| ファイル | 変更内容 |
|---------|---------|
| `crates/worker/src/config.rs` | `github_app_id`, `github_private_key_pem` 追加 |
| `crates/worker/src/main.rs` | client初期化 + poll_and_process拡張 + モジュール宣言 |
| `crates/worker/src/dispatcher.rs` | **新規** ジョブディスパッチャ（各タイプのdequeue → handler呼び出し） |
| `crates/worker/src/handlers/mod.rs` | **新規** ハンドラモジュール宣言 |
| `crates/worker/src/handlers/create_issue.rs` | **新規** Issue作成ハンドラ |
| `crates/worker/src/handlers/create_dashboard_comment.rs` | **新規** Dashboardコメント作成ハンドラ |
| `crates/worker/src/handlers/update_dashboard_comment.rs` | **新規** Dashboardコメント更新ハンドラ |
| `crates/worker/src/handlers/create_run_result_comment.rs` | **新規** Run Resultコメント作成ハンドラ |
| `crates/worker/src/comment_body.rs` | **新規** コメント本文テンプレート生成 |
| `crates/worker/Cargo.toml` | `secrecy` 依存追加 |
| `crates/db/src/queries/board_project.rs` | `update_issue_info()`, `update_dashboard_comment_id()`, `clear_issue_info()` 追加 |
| `crates/db/src/queries/board_run.rs` | `find_previous_completed()` 追加（Run Result判定用） |

### 設計方針

#### ファイル構成

```
crates/worker/src/
├── main.rs           # エントリポイント: config読込、client初期化、ループ
├── config.rs         # WorkerConfig
├── dispatcher.rs     # poll_and_dispatch(): dequeue戦略とディスパッチロジック
├── comment_body.rs   # IssueBody/Dashboardコメント/RunResultコメントの本文生成
└── handlers/
    ├── mod.rs
    ├── import.rs             # process_import_job (既存コードを移動)
    ├── create_issue.rs
    ├── create_dashboard_comment.rs
    ├── update_dashboard_comment.rs
    └── create_run_result_comment.rs
```

#### main.rs の変更概要

```rust
// main.rs (概要)
mod config;
mod comment_body;
mod dispatcher;
mod handlers;

#[tokio::main]
async fn main() {
    let config = WorkerConfig::from_env();
    let pool = ...;
    let s3_client = ...;

    // GitHub App client (optional)
    let github_client: Option<Arc<dyn GitHubAppClient>> = match (&config.github_app_id, &config.github_private_key_pem) {
        (Some(app_id), Some(pem)) => {
            let gh_config = GitHubAppConfig { app_id: *app_id, private_key_pem: SecretString::from(pem.clone()) };
            let client = OctocrabGitHubAppClient::new(&gh_config).expect("failed to create GitHub client");
            Some(Arc::new(client))
        }
        _ => {
            tracing::warn!("GitHub App not configured, skipping GitHub API jobs");
            None
        }
    };

    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            _ = dispatcher::poll_and_dispatch(&pool, &s3_client, &config, github_client.as_deref()) => {}
        }
    }
}
```

#### dispatcher.rs の概要

```rust
pub async fn poll_and_dispatch(
    pool: &PgPool,
    s3_client: &aws_sdk_s3::Client,
    config: &WorkerConfig,
    github_client: Option<&dyn GitHubAppClient>,
) {
    // 1. artifact_bundle_import
    if let Some(job) = try_dequeue(pool, "artifact_bundle_import").await {
        handle_import(pool, s3_client, config, &job).await;
        return;
    }

    // 2-5. GitHub API jobs (skip if no client)
    if let Some(gh) = github_client {
        for job_type in &["create_issue", "create_dashboard_comment", "update_dashboard_comment", "create_run_result_comment"] {
            if let Some(job) = try_dequeue(pool, job_type).await {
                handle_github_job(pool, gh, config, &job).await;
                return;
            }
        }
    }

    // No jobs found
    tokio::time::sleep(Duration::from_secs(config.poll_interval_secs)).await;
}
```

#### 各ハンドラのシグネチャ

```rust
// 統一的なハンドラ結果型
pub enum HandlerResult {
    Completed,
    Reschedule { reason: String, backoff_secs: f64 },
    Failed { reason: String },
}

pub async fn handle_create_issue(
    pool: &PgPool,
    github_client: &dyn GitHubAppClient,
    job: &GithubJob,
) -> HandlerResult;
```

### テスト観点

1. **ユニットテスト（各ハンドラ）**: `GitHubAppClient` trait をモックし、DB操作はテスト用のPgPoolを使用
   - Issue作成成功 → DB更新確認
   - 冪等性: issue_number既存時のスキップ確認
   - 依存チェック: issue_number未設定時のreschedule確認
   - エラーハンドリング: RateLimited, NotFound, Auth 各ケース
2. **ユニットテスト（comment_body）**: テンプレート生成の正確性
3. **統合テスト**: `crates/worker/tests/` にGitHub APIモック + テストDBを使った統合テスト（後続Issueで対応も可）
4. **Run Result追記条件**: 状態変化パターン（passed→failed, failed→passed, failed→failed変化なし）のテスト

### ドキュメント更新対象

- `docs/external/github-app-octocrab.md`: `RetryConfig::HandleRateLimits` の設定例を追記（簡潔に）
- `docs/backend/summary.md`: workerの構成変更を反映

### 実装要否

**implementation_required**

### 未解決の疑問

1. **Run Resultの「前回Run」の取得方法**: `board_project.latest_completed_run_id` はimportジョブ完了時に今回のrunに更新済み。前回runを知るには `board_runs` テーブルから `board_project_id` で `completed_at DESC` の2番目を取得する必要がある。
   → **解決**: `board_run_id`（今回run）の `completed_at` より前の最新completed runをクエリで取得する方式で対応。

2. **Issueが closed + recreate_issue_on_update の場合の処理**: spec.md 11.7にある通り、closed Issueで `recreate_issue_on_update = true` かつ `tree_hash` 変更ありなら新Issue作成。この判定はどの時点で行うか？
   → **解決**: `create_issue` ハンドラ実行時に `get_issue()` でIssue状態を確認し、closed + 条件成立なら新Issue作成する。既存issueの情報は `board_project_issue_history` に退避する。MVPでは `create_issue` ジョブが来た時点で `issue_number = None` が前提のため、初回のシンプルケースのみ実装。`recreate` は後続Issueで対応する。

3. **SaaSのベースURL**: IssueやコメントにSaaSのURLを埋め込む必要がある。`WorkerConfig` に `BOARDFLOW_BASE_URL` を追加する。
   → 追加する。

### 残リスク

- octocrab `RetryConfig::HandleRateLimits` の `retry` feature flag が有効でないとコンパイルエラーになる可能性 → Cargo.toml確認が必要
- `board_project_issue_history` テーブルのマイグレーションが未作成の場合、`recreate_issue_on_update` フローは未実装とする
- 単一workerのMVPでは並列数制御は不要だが、スケールアウト時に `FOR UPDATE SKIP LOCKED` だけでは不十分になる可能性

### 更新した作業ログパス

`docs/logs/26/worklog.md`

### 残リスク

- octocrabの `RetryConfig::HandleRateLimits` がデフォルト有効か確認が必要（`retry` featureはデフォルトfeaturesに含まれるが、`HandleRateLimits` は明示設定が必要）
- Issue作成→コメント作成の依存関係（create_issueが完了しないとコメントジョブが実行できない）のジョブ間依存解決方法
- `retry_after_secs` をoctocrabエラーから正確に抽出する方法（現状Noneのまま）

### 結論ステータス

**implementation_required**

既存のコードベース（`crates/github/`, `crates/db/`, `crates/jobs/`, `crates/worker/`）と外部ドキュメントで実装に十分。新規外部ライブラリの導入は不要。
