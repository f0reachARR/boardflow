# Issue #5: Action API: BoardRun作成・Fail・Import実装

## 経緯
- バックエンド実装Issue分割タスクの一環として作成
- Action ライフサイクルの中核3エンドポイント

## ユーザー要望
- docs/以下の仕様に基づくRustバックエンド実装の第5段階

## Issue作成内容
- POST /api/v1/board-runs, POST .../fail, POST .../artifact-bundles/import
- URL: https://github.com/f0reachARR/boardflow/issues/5

## 後続処理タイプの初期仮説
`implementation_required`

## 調査フェーズ (2026-05-01)

### 調査トピック

1. **S3互換 presigned URL 生成 (Rust)**
   - 調査結果: `aws-sdk-s3` (v1.131.0+) + `aws-config` (v1.8.16) を採用
   - MinIO互換: `force_path_style(true)` + `endpoint_url()` で対応可能
   - presigned PUT URL: `client.put_object().presigned(PresigningConfig::expires_in(...))` で生成
   - ドキュメント: `docs/external/aws-sdk-s3-presigned-url.md`

2. **PostgreSQL job queue enqueue パターン**
   - 調査結果: 既存 `github_jobs` テーブル + 部分ユニークインデックス追加で冪等enqueue実装可能
   - `INSERT ... ON CONFLICT (board_run_id, type) WHERE board_run_id IS NOT NULL` パターン
   - トランザクション内で BoardRun ステータス変更 + bundle 作成 + job enqueue を一括実行
   - Worker pull: `FOR UPDATE SKIP LOCKED` パターン
   - ドキュメント: `docs/external/postgresql-job-queue-enqueue.md`

3. **aws-sdk-s3 最新バージョンと MinIO 互換設定**
   - `aws-sdk-s3`: v1.131.0+ (semver `"1"` 指定)
   - `aws-config`: v1.8.16 (semver `"1"` 指定)
   - 両方に `features = ["behavior-version-latest"]` が必要
   - workspace Cargo.toml に追加が必要
   - ドキュメント: `docs/external/aws-sdk-s3-presigned-url.md` に統合

### Cargo.toml への追加予定

```toml
[workspace.dependencies]
aws-config = { version = "1", features = ["behavior-version-latest"] }
aws-sdk-s3 = { version = "1", features = ["behavior-version-latest"] }
```

### マイグレーション追加予定

```sql
CREATE UNIQUE INDEX idx_github_jobs_board_run_id_type
ON github_jobs (board_run_id, type)
WHERE board_run_id IS NOT NULL;
```

### 結論ステータス
`implementation_required`

### 残リスク
- 本番S3互換サービスの選定 (MVP ではMinIOで進行)
- presigned URLの有効期限のデフォルト値 (1時間を提案)
- `github_jobs` の max_attempts / backoff ポリシー (worker 実装時に決定)
- docker-compose への MinIO サービス追加が必要

---

## 計画フェーズ (2026-05-01)

### 目的
GitHub Actions ライフサイクルの中核3エンドポイントを実装し、Action が BoardRun 作成→成果物 upload→import 完了まで一連のフローを実行可能にする。

### 非目的
- Worker による zip 展開・manifest 検証 (Issue #7 以降)
- Web UI 向け Read API (Issue #6 以降)
- 本番 S3 / CloudFront 設定
- DRC/ERC 結果の保存ロジック (Import worker 側)
- docker-compose へのMinIO追加 (別タスク or 同PR内で最小限)

### 受け入れ条件
1. `POST /api/v1/board-runs` が冪等にBoardRunを作成し、presigned PUT URLを返す
2. `POST /api/v1/board-runs/{board_run_id}/fail` がBoardRunをfailedに遷移する (冪等)
3. `POST /api/v1/board-runs/{board_run_id}/artifact-bundles/import` がArtifactBundleを作成し、import jobをenqueueする
4. 全エンドポイントでBearer認証・repository権限チェックが動作する
5. 冪等性・状態遷移ルールが仕様通りに動作する
6. 統合テストが各正常系・異常系をカバーする
7. OpenAPI スキーマに3エンドポイントが記載される

### 詳細要件

#### 2.2 BoardRun 作成 API (`POST /api/v1/board-runs`)
- Request: `board_project_id`, `project_path`, `tree_hash`, `commit_sha`, `branch`, `ref`, `github_run_id`, `github_run_attempt`
- 認証: Bearer token → token の repository_id から board_project が同リポジトリに属するか検証
- 冪等キー: `board_project_id + github_run_id + github_run_attempt` (DBのUNIQUE制約)
- 新規作成時: BoardRun (status=created) + ArtifactBundle (status=pending) + presigned PUT URL
- 既存 created/uploading: 既存run + 新 presigned URL を返す
- 既存 importing: run返却、artifact_bundle は null
- 既存 completed/failed/timed_out: terminal状態を返す (追加actionなし)
- Presigned URL: aws-sdk-s3 で MinIO互換の PUT presigned URL (1時間有効)

#### 2.3 Artifact Bundle Import API (`POST /api/v1/board-runs/{board_run_id}/artifact-bundles/import`)
- Request: `staging_object_key`, `bundle_sha256`, `bundle_size_bytes`
- 認証: Bearer token → board_run の board_project → repository がtoken と一致
- 冪等キー: `board_run_id + staging_object_key + bundle_sha256`
- 正常系: ArtifactBundle更新 (status=queued → DB上は pending) + BoardRun status → importing + github_jobs enqueue
- 同一run + 異なるkey/sha256: 409 conflict
- completed run: 既存bundle状態を返す (新job作成しない)
- failed/timed_out run: 410 gone

#### 2.4 Fail API (`POST /api/v1/board-runs/{board_run_id}/fail`)
- Request: `status` (= "failed"), `error: { message, details }`
- 認証: Bearer token → board_run の board_project → repository がtoken と一致
- 冪等: 既存 failed → 既存の failed_at を返す
- completed: 409 conflict
- timed_out: 410 gone
- 正常遷移: created/uploading/importing → failed, completed_at = now

### 影響範囲

| レイヤー | 変更 |
|---|---|
| workspace Cargo.toml | `aws-config`, `aws-sdk-s3` 追加 |
| crates/api/Cargo.toml | `aws-config`, `aws-sdk-s3` 依存追加 |
| crates/api/src/routes/ | `board_run.rs` 新規作成 |
| crates/api/src/routes/mod.rs | `pub mod board_run;` 追加 |
| crates/api/src/lib.rs | 3ルート登録 |
| crates/api/src/error.rs | `not_found()`, `conflict()`, `gone()` メソッド追加 |
| crates/api/src/config.rs | `presigned_url_expiry_secs` 追加 (optional) |
| crates/db/src/queries/ | `board_run.rs`, `artifact_bundle.rs`, `github_job.rs` 新規 |
| crates/db/src/queries/mod.rs | 3モジュール追加 |
| crates/db/migrations/ | 部分ユニークインデックス追加マイグレーション |
| crates/api/tests/ | `board_run_test.rs` 新規 |
| docker-compose.yml | MinIO サービス追加 |

### 設計方針

#### S3 クライアント初期化
- `crates/api/src/lib.rs` の `create_app()` に S3 client を `Extension` として注入
- `AppConfig` から endpoint/credentials を読み取り、`aws_sdk_s3::Client` を構築
- テスト時は S3 presigned URL 生成をスキップ可能にする (Option<Client> or trait)
  → MVP では `Option<aws_sdk_s3::Client>` を Extension に入れ、None 時は固定URLを返す

#### ルートハンドラ構成
```rust
// crates/api/src/routes/board_run.rs
pub async fn create_board_run(...) -> Result<Json<CreateBoardRunResponse>, AppError>
pub async fn fail_board_run(...) -> Result<Json<FailBoardRunResponse>, AppError>
pub async fn import_artifact_bundle(...) -> Result<Json<ImportArtifactBundleResponse>, AppError>
```

#### DB クエリ関数
```rust
// crates/db/src/queries/board_run.rs
pub async fn find_by_idempotency_key(pool, board_project_id, github_run_id, github_run_attempt) -> Result<Option<BoardRun>>
pub async fn create(pool, id, board_project_id, ...) -> Result<BoardRun>
pub async fn update_status(pool, id, new_status) -> Result<BoardRun>
pub async fn find_by_id(pool, id) -> Result<Option<BoardRun>>

// crates/db/src/queries/artifact_bundle.rs
pub async fn find_by_board_run_id(pool, board_run_id) -> Result<Option<ArtifactBundle>>
pub async fn create(pool, id, board_run_id, ...) -> Result<ArtifactBundle>
pub async fn update_for_import(pool, id, staging_object_key, sha256, size_bytes) -> Result<ArtifactBundle>

// crates/db/src/queries/github_job.rs
pub async fn enqueue_import(pool, id, installation_id, repository_id, board_project_id, board_run_id, payload) -> Result<GithubJob>
```

#### 認可チェック共通化
- board_run_id から board_project_id → repository_id を取得し、token.repository_id と一致確認
- `find_board_run_with_auth()` 的なヘルパーをroute内で実装 (過度な抽象化はしない)

#### Presigned URL 生成
```rust
// crates/api/src/routes/board_run.rs 内のヘルパー
async fn generate_presigned_put_url(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
    expires_in: Duration,
) -> Result<(String, DateTime<Utc>), AppError>
```

### テスト観点

| テストケース | 種別 | エンドポイント |
|---|---|---|
| 正常作成 (新規) | 統合 | create |
| 冪等再送 (同一key) | 統合 | create |
| terminal状態の既存run | 統合 | create |
| board_project が別リポジトリ | 統合 | create |
| 認証なし | 統合 | create |
| 正常fail | 統合 | fail |
| 既にfailed (冪等) | 統合 | fail |
| completed → fail (409) | 統合 | fail |
| timed_out → fail (410) | 統合 | fail |
| 存在しないboard_run_id | 統合 | fail |
| 正常import | 統合 | import |
| 冪等再送 (同一bundle) | 統合 | import |
| 異なるsha256で再送 (409) | 統合 | import |
| failed run へ import (410) | 統合 | import |
| completed run へ import | 統合 | import |

テストでは S3 client を None にし、presigned URL 生成をスキップしてDB・状態遷移ロジックを検証する。

### ドキュメント更新対象
- `docs/logs/5/worklog.md` (本ファイル)
- `docs/backend/summary.md` に実装済みエンドポイント追記

### 実装順序

1. **マイグレーション追加** — `20260501000000_add_github_jobs_idempotent_index.up.sql`
2. **workspace Cargo.toml** — `aws-config`, `aws-sdk-s3` 追加
3. **crates/api/Cargo.toml** — 依存追加
4. **crates/api/src/error.rs** — `not_found()`, `conflict()`, `gone()` メソッド追加
5. **crates/db/src/queries/board_run.rs** — DB クエリ関数
6. **crates/db/src/queries/artifact_bundle.rs** — DB クエリ関数
7. **crates/db/src/queries/github_job.rs** — DB クエリ関数
8. **crates/db/src/queries/mod.rs** — モジュール登録
9. **crates/api/src/routes/board_run.rs** — 3ハンドラ + request/response types
10. **crates/api/src/routes/mod.rs** — モジュール登録
11. **crates/api/src/lib.rs** — ルート登録 + S3 client Extension
12. **docker-compose.yml** — MinIO サービス追加
13. **crates/api/tests/board_run_test.rs** — 統合テスト
14. **ビルド確認 + テスト実行**

### マイグレーション計画

```sql
-- 20260501000000_add_github_jobs_idempotent_index.up.sql
CREATE UNIQUE INDEX idx_github_jobs_board_run_id_type
ON github_jobs (board_run_id, type)
WHERE board_run_id IS NOT NULL;
```

```sql
-- 20260501000000_add_github_jobs_idempotent_index.down.sql
DROP INDEX IF EXISTS idx_github_jobs_board_run_id_type;
```

### 実装要否
`implementation_required`

### 未解決の疑問
- なし (仕様書・research成果物で十分な情報が揃っている)

### 残リスク
- MinIO がテスト環境で起動していない場合、presigned URL 生成のテストがスキップされる
  → S3 client を Option にし、None 時は固定URLでテスト通過させる設計で対応
- `board_project_id` のprefix形式 (`bp_...`) はDB上はUUIDだが、API入出力でprefix付き文字列に変換が必要
  → 既存 plan.rs の実装を確認: plan.rs では `board_project_id` を文字列で返している。UUID → prefix付き変換は別途ユーティリティが必要か、あるいはUUIDそのまま使うか
  → 既存テスト・plan.rs の挙動に合わせる (現状UUIDの文字列表現を使用)

### 更新した作業ログパス
`docs/logs/5/worklog.md`
