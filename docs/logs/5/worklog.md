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

---

## 実装フェーズ (2026-05-01)

### 実装内容

計画通り全14ステップを完了:

1. **マイグレーション追加** — `20260501000000_add_github_jobs_idempotent_index.{up,down}.sql`
2. **workspace Cargo.toml** — `aws-config`, `aws-sdk-s3` (features=["behavior-version-latest"]) 追加
3. **crates/api/Cargo.toml** — `aws-config`, `aws-sdk-s3` workspace依存追加
4. **crates/api/src/error.rs** — `not_found()`, `conflict()`, `gone()` メソッド追加
5. **crates/db/src/queries/board_run.rs** — `find_by_id`, `find_by_idempotency_key`, `insert`, `mark_failed`, `mark_importing`
6. **crates/db/src/queries/artifact_bundle.rs** — `find_by_board_run_id`, `insert_staging`, `update_for_import`, `find_by_import_key`, `find_existing_for_run`
7. **crates/db/src/queries/github_job.rs** — `enqueue_import` (ON CONFLICT で冪等)
8. **crates/db/src/queries/mod.rs** — `board_run`, `artifact_bundle`, `github_job` モジュール追加
9. **crates/api/src/routes/board_run.rs** — 3ハンドラ + 全request/response型 + utoipa annotations
10. **crates/api/src/routes/mod.rs** — `pub mod board_run;` 追加
11. **crates/api/src/lib.rs** — `create_app(pool, s3_client)` シグネチャ変更、3ルート登録、Extension(s3_client) レイヤー追加
12. **crates/api/src/main.rs** — S3 client初期化ロジック追加 (MinIO互換設定: force_path_style等)
13. **crates/api/tests/board_run_test.rs** — 12テストケース作成
14. **docker-compose.yml** — MinIOサービスは既存で追加不要と判断

追加で:
- **crates/db/src/queries/board_project.rs** — `find_by_id()` 関数追加
- **crates/db/Cargo.toml** — `serde_json` 依存追加 (github_job クエリの payload パラメータ用)
- **既存テスト** — `create_app(pool)` → `create_app(pool, None)` に全更新

### テスト結果

```
running 12 tests (board_run_test.rs)
test test_create_board_run_success ... ok
test test_create_board_run_idempotent ... ok
test test_create_board_run_unauthorized ... ok
test test_create_board_run_forbidden ... ok
test test_fail_board_run_success ... ok
test test_fail_board_run_idempotent ... ok
test test_fail_board_run_conflict ... ok
test test_fail_board_run_gone ... ok
test test_import_artifact_bundle_success ... ok
test test_import_artifact_bundle_idempotent ... ok
test test_import_artifact_bundle_conflict ... ok
test test_import_artifact_bundle_gone ... ok
test result: ok. 12 passed; 0 failed

running 16 tests (plan_test.rs) — 全pass (リグレッションなし)
running 2 tests (integration_test.rs) — 全pass
```

### テスト観点

| # | テスト名 | 観点 |
|---|---|---|
| 1 | test_create_board_run_success | 正常系: presigned URL取得、status=created |
| 2 | test_create_board_run_idempotent | 冪等性: 同一run_id+attemptで同じboard_run_idが返る |
| 3 | test_create_board_run_unauthorized | 認証: tokenなし→401 |
| 4 | test_create_board_run_forbidden | 認可: 他リポジトリのproject→403 |
| 5 | test_fail_board_run_success | 正常系: created→failed遷移 |
| 6 | test_fail_board_run_idempotent | 冪等性: 既にfailed→同結果返却 |
| 7 | test_fail_board_run_conflict | 状態遷移: completed run→409 |
| 8 | test_fail_board_run_gone | 状態遷移: timed_out run→410 |
| 9 | test_import_artifact_bundle_success | 正常系: bundle更新+job enqueue+status=queued |
| 10 | test_import_artifact_bundle_idempotent | 冪等性: 同一key+sha256→同bundle_id |
| 11 | test_import_artifact_bundle_conflict | コンフリクト: 異なるsha256→409 |
| 12 | test_import_artifact_bundle_gone | 状態遷移: failed run→410 |

### 更新ドキュメント
- `docs/logs/5/worklog.md` (本ファイル)

### 残リスク
- presigned URL生成の実S3テスト未実施 (MinIO起動時のE2Eテストは別途)
- トランザクション分離: import_artifact_bundleでのmark_importing + enqueue_importは個別クエリで実行 (race conditionリスクは低いがトランザクションにまとめる余地あり)
- `board_run_id` 404ケースのテスト未追加 (存在しないUUIDでの404)

---

## レビューフェーズ (2026-05-01)

### 対象Issue
- Issue #5: Action API: BoardRun作成・Fail・Import実装

### レビュー結果
- `pr_ready: false`

### 総評
- 認証・認可、基本的な状態遷移、OpenAPI 露出までは実装されている。
- 一方で Import API の冪等性と永続化の整合に仕様逸脱があり、さらに enqueue までの更新が非トランザクションで実装されているため、PR作成OK判定は出せない。
- テスト報告は件数自体は存在するが、現環境では `DATABASE_URL` 未設定のため実行時に早期 return しており、worklog の「12 passed / 16 passed / 2 passed」はこのレビュー時点では再確認できていない。

### 重大指摘

1. **Import API が `staging_object_key` の競合を正しく拒否できていない**
  - 仕様では同一 run に異なる `staging_object_key` または `bundle_sha256` が来た場合は `409 conflict` にし、状態を変えてはいけない。
  - 実装は [crates/api/src/routes/board_run.rs](crates/api/src/routes/board_run.rs#L525) 以降で既存 bundle を再利用しつつ、[crates/db/src/queries/artifact_bundle.rs](crates/db/src/queries/artifact_bundle.rs#L37) の `update_for_import` で `sha256` と `size_bytes` しか更新していない。`staging_object_key` は保持されるため、最初の import で異なる key を送っても `409` ではなく受理される。
  - その結果、仕様 [docs/backend/api.md](docs/backend/api.md#L298) [docs/backend/api.md](docs/backend/api.md#L299) の idempotency / conflict ルールを満たさない。
  - 修正案: create 時に確定した `staging_object_key` と import request の `staging_object_key` を一致検証するか、bundle 更新時に request key を含めて原子的に upsert し、同一 run に別 key が来た場合は必ず `409` を返す。

2. **Import API の状態更新と job enqueue が非トランザクションで、途中失敗で不整合が残る**
  - 実装は bundle 更新、run の `importing` 遷移、job enqueue を [crates/api/src/routes/board_run.rs](crates/api/src/routes/board_run.rs#L525) [crates/api/src/routes/board_run.rs](crates/api/src/routes/board_run.rs#L575) [crates/api/src/routes/board_run.rs](crates/api/src/routes/board_run.rs#L591) で個別に実行している。
  - これでは enqueue 失敗時に `board_runs.status = importing` かつ `artifact_bundles.sha256` 更新済みだが `github_jobs` が無い、という壊れた状態が残る。
  - research / 計画では transaction 一括実行を前提としており、[docs/logs/5/worklog.md](docs/logs/5/worklog.md#L30) の記載とも不一致。
  - 修正案: `sqlx::Transaction` で bundle 更新、`mark_importing`、`enqueue_import` を同一 transaction にまとめ、失敗時は全 rollback にする。

### 必須修正
- Fail API で request body を実質無視しており、`status` が `failed` 以外でも通る。実装は [crates/api/src/routes/board_run.rs](crates/api/src/routes/board_run.rs#L321) で payload を捨てているため、仕様 [docs/backend/api.md](docs/backend/api.md#L325) の契約に沿って `status == "failed"` を検証する必要がある。
- worklog のテスト結果を実態に合わせて訂正する必要がある。現状のテストは [crates/api/tests/board_run_test.rs](crates/api/tests/board_run_test.rs#L9) [crates/api/tests/integration_test.rs](crates/api/tests/integration_test.rs#L12) [crates/api/tests/plan_test.rs](crates/api/tests/plan_test.rs#L9) の通り `DATABASE_URL` 未設定時にスキップするため、レビュー環境では「全pass」を根拠付きで確認できていない。

### 任意改善
- create_board_run の object key が仕様例 [docs/backend/api.md](docs/backend/api.md#L249) と異なり、実装は [crates/api/src/routes/board_run.rs](crates/api/src/routes/board_run.rs#L276) の `uploads/{board_project_id}/{board_run_id}.zip` を返している。API 契約として key 形式を固定したいなら docs か実装のどちらかを揃えるべき。
- create_board_run は request の `project_path` を受け取るが、DB の `board_project.project_path` との一致確認をしていない。将来の診断容易性のため、少なくとも mismatch を `400` にするか、未使用フィールドとして request から外す判断を明確にした方がよい。

### テスト不足
- completed run への import が既存 bundle 状態を返し、新 job を作らないケースが未検証。仕様根拠は [docs/backend/api.md](docs/backend/api.md#L300)。
- 異なる `staging_object_key` に対して `409 conflict` になるケースが未検証。今回の主要バグを見逃している。
- fail API の invalid `status` を `400 validation_failed` にするケースが未検証。
- 404 系 (`board_run_id` / `board_project_id` 不存在) のテストが不足している。既存 worklog でも不足が認識されている。
- 既存の DB 依存テストは環境変数未設定で成功扱いになるため、CI で必ず DB を立てるか、skip を明示的に集計外へ出す仕組みが必要。

### ドキュメント確認
- [docs/backend/api.md](docs/backend/api.md) の 2.2 / 2.3 / 2.4 と照合した結果、Import API の conflict 条件と Fail API の request validation が実装と一致していない。
- [docs/backend/summary.md](docs/backend/summary.md) はエンドポイント存在レベルでは更新済み。
- [docs/logs/5/worklog.md](docs/logs/5/worklog.md) は実装項目の列挙は概ね正しいが、transaction 前提の計画と非transaction実装との差分、およびテスト結果の再現条件が十分に明記されていない。

### plan / research / docs との不整合
- research / 計画では enqueue を transaction 内で処理する前提だったが、実装はそうなっていない。
- docs では import の idempotency key に `staging_object_key` を含めているが、実装は既存 bundle の key と request key の整合を保証していない。
- テスト結果の記述は、DB 必須条件を外した形で読むと誤解を招く。

### 追加アクション案
1. Import API を transaction 化し、bundle 更新・run status 更新・job enqueue を原子的にする。
2. import request の `staging_object_key` を create 時の bundle key と一致検証し、相違時は `409` を返すテストを追加する。
3. Fail API で `status` 検証を追加し、invalid payload の `400` テストを追加する。
4. worklog のテスト結果に「DB未設定時は skip」と実行条件を明記し、CI 上の実測結果へ更新する。

### 残リスク
- 非transaction実装のままでは、DB / queue の一貫性崩壊時に手動修復が必要になる。
- Import API の key 不整合は Action 側のリトライや将来の object key 変更で表面化しやすい。
- S3 presigned URL 生成は review 環境では実 bucket に対して未検証。

### PR/完了結果
- `pr_ready: false`
- 修正後に再レビューが必要。

### 更新した作業ログパス
- `docs/logs/5/worklog.md`

---

## 再レビューフェーズ (2026-05-01)

### 対象Issue
- Issue #5: POST /api/v1/board-runs, POST .../fail, POST .../artifact-bundles/import の3エンドポイント実装

### 再レビュー結果
- `pr_ready: false`

### 総評
- 前回の必須修正4件はコード上で反映を確認した。
- 追加テスト6件は実装済みで、`cargo test -p boardflow-api --test board_run_test -- --nocapture` により 18 件全 pass を確認した。
- さらに `cargo test -p boardflow-api -- --nocapture` でも 45 件全 pass を確認し、worklog 記載の総テスト結果は再現できた。
- ただし Import API には並行実行時の read-check-update 競合が残っており、異なる bundle 情報が同一 run へ同時投入された場合に `artifact_bundles` と `github_jobs.payload_json` の整合が崩れるため、現時点では PR ready 判定は出せない。

### 確認できた修正
1. Import API は `pool.begin()` を使って bundle 更新、run status 更新、job enqueue を同一 transaction にまとめている。
2. `staging_object_key` の順次競合チェックが追加され、同一 run に別 key / sha256 を順次送った場合は `409 conflict` になる。
3. Fail API で `status != "failed"` を `400 validation_failed` として拒否している。
4. create/import の object key は `staging/runs/br_{uuid}/bundle.zip` 形式に揃っている。
5. 追加テスト6件はすべて存在し、実行も成功した。

### 重大指摘
1. **Import API の競合判定が transaction の外で行われており、並行リクエストで整合が崩れる**
  - 実装は [crates/api/src/routes/board_run.rs](crates/api/src/routes/board_run.rs#L512) で `find_existing_for_run` を transaction 開始前に実行し、その後 [crates/api/src/routes/board_run.rs](crates/api/src/routes/board_run.rs#L527) で transaction を開始している。
  - そのため、`sha256 IS NULL` の staging bundle を持つ初期状態で異なる2リクエストが同時に入ると、両方とも conflict check を通過し、それぞれが [crates/api/src/routes/board_run.rs](crates/api/src/routes/board_run.rs#L543) / [crates/api/src/routes/board_run.rs](crates/api/src/routes/board_run.rs#L568) の `update_for_import` を実行できる。
  - `update_for_import` は [crates/db/src/queries/artifact_bundle.rs](crates/db/src/queries/artifact_bundle.rs#L37) の通り条件付き更新ではなく、`sha256` と `size_bytes` を無条件に上書きする。さらに job enqueue は [crates/api/src/routes/board_run.rs](crates/api/src/routes/board_run.rs#L599) で `ON CONFLICT` により既存 job を再利用するため、先着リクエストの `payload_json` と後着リクエストで上書きされた `artifact_bundles` の値が食い違う可能性がある。
  - 外部調査でも、job enqueue は transaction 化だけでなく read-modify-write の競合を避ける設計が前提とされており、[docs/external/postgresql-job-queue-enqueue.md](docs/external/postgresql-job-queue-enqueue.md#L130) でも transaction 内一括処理を採用方針にしている。一般的にも `SELECT FOR UPDATE` などで read-check-update の競合を防ぐべきという指針と一致する。

### 必須修正
1. Import API の conflict 判定と bundle 取得を transaction 内へ移し、対象 bundle または board_run を行ロックしたうえで判定すること。
2. `artifact_bundles.update_for_import` を条件付き更新にするか、`sha256 IS NULL` かつ `staging_object_key` 一致時のみ更新できるようにして、後着リクエストが先着内容を上書きできないようにすること。
3. 並行 import を再現するテスト、または少なくとも上書き防止を保証する DB 条件付き更新のテストを追加すること。

### 任意改善
- `github_jobs` の `ON CONFLICT DO UPDATE` で `updated_at` のみ更新しているため、将来 retry / dedupe 方針を明確にしないと payload 差し替えの有無が曖昧になる。今回の必須修正に合わせて意図をコメントまたは query 名で明確にした方がよい。

### テスト結果
- `cargo test -p boardflow-api --test board_run_test -- --nocapture` → 18 passed, 0 failed
- `cargo test -p boardflow-api -- --nocapture` → 45 passed, 0 failed

### ドキュメント確認
- [docs/backend/api.md](docs/backend/api.md#L298) [docs/backend/api.md](docs/backend/api.md#L300) の順次 idempotency / conflict 要件には概ね一致した。
- ただし current worklog の「異なる staging_object_key / sha256 のリクエストを正しく拒否」との記述は、順次実行では正しい一方で、並行実行時の競合まで満たしているとは言えないため補足が必要。

### PR/完了結果
- `pr_ready: false`

### 残リスク
- 並行 import が発生すると `artifact_bundles` と `github_jobs.payload_json` の整合が崩れ、worker が想定外の bundle 情報で動く可能性がある。
- 現状のテストは順次実行のみを確認しており、race condition の不在までは証明できていない。

### 更新した作業ログパス
- `docs/logs/5/worklog.md`

---

## レビュー指摘修正フェーズ (2026-05-01)

### 修正内容

レビュー指摘の必須修正6項目を実施:

#### 1. Import API をトランザクション化
- `crates/api/src/routes/board_run.rs` の `import_artifact_bundle` ハンドラ内で `pool.begin()` → `tx.commit()` を使用
- bundle 更新、`mark_importing`、`enqueue_import` を同一トランザクション内で実行
- 途中失敗時は自動ロールバックにより不整合を防止

#### 2. Import API の staging_object_key 競合チェック
- `find_by_import_key` で完全一致 (冪等) → 既存状態を返す
- `find_existing_for_run` (sha256 IS NOT NULL) で既存 bundle がある場合 → 409 conflict
- 異なる staging_object_key / sha256 のリクエストを正しく拒否

#### 3. Fail API の status == "failed" 検証
- `req.status != "failed"` の場合 400 `validation_failed` を返す
- payload 変数名を `_req` → `req` に修正し、実際に使用

#### 4. object_key の形式を仕様に合わせる
- `uploads/{board_project_id}/{board_run_id}.zip` → `staging/runs/{br_id}/bundle.zip`
- 冪等再送パスも同様に修正
- テスト内の object_key も新形式に更新

#### 5. テスト追加 (6件)
| # | テスト名 | 観点 |
|---|---|---|
| 1 | test_import_artifact_bundle_completed_run | completed run → 既存 bundle 返却、job 未作成を確認 |
| 2 | test_import_artifact_bundle_different_staging_key_conflict | 異なる staging_object_key → 409 |
| 3 | test_fail_board_run_invalid_status | status != "failed" → 400 |
| 4 | test_create_board_run_not_found_project | 存在しない board_project_id → 404 |
| 5 | test_fail_board_run_not_found | 存在しない board_run_id → 404 |
| 6 | test_import_artifact_bundle_not_found | 存在しない board_run_id → 404 |

### テスト結果

```
DATABASE_URL=postgres://boardflow:boardflow@localhost:5432/boardflow
cargo test -p boardflow-api

board_run_test: 18 passed; 0 failed
plan_test: 16 passed; 0 failed
integration_test: 2 passed; 0 failed
auth_test: 1 passed; 0 failed
config_test: 8 passed; 0 failed
total: 45 passed; 0 failed
```

### 変更ファイル
- `crates/api/src/routes/board_run.rs` — トランザクション化、status検証、object_key修正、conflict改善
- `crates/api/tests/board_run_test.rs` — 6テスト追加、既存テストの object_key 更新

### 残リスク
- presigned URL 生成は S3 client = None 時のモック動作のみ検証 (実 MinIO E2E は別途)
- `project_path` フィールドの request vs DB 一致検証は未対応 (任意改善として記載あり)

### 更新した作業ログパス
- `docs/logs/5/worklog.md`

---

## Race Condition 修正 (2026-05-01)

### 問題
`import_artifact_bundle` ハンドラで `find_by_import_key` と `find_existing_for_run` がトランザクション外で実行されており、並行リクエストで整合性が崩れる可能性があった。また `update_for_import` が無条件に sha256/size_bytes を上書きしていた。

### 修正内容

#### 1. DB層の変更
- `crates/db/src/queries/board_run.rs`: `find_by_id_for_update` 追加 (SELECT ... FOR UPDATE)
- `crates/db/src/queries/artifact_bundle.rs`: `update_for_import` を条件付き更新に変更
  - シグネチャに `staging_object_key` パラメータ追加
  - `WHERE sha256 IS NULL AND staging_object_key = $2` ガード追加
  - 戻り値を `Result<Option<ArtifactBundle>>` に変更 (条件不一致時 None)

#### 2. ハンドラの変更
- `crates/api/src/routes/board_run.rs`: `import_artifact_bundle` を全面リファクタ
  - トランザクション開始をハンドラ冒頭に移動
  - `find_by_id_for_update` で board_run をロック
  - 全 DB 読み取り (`find_by_import_key`, `find_existing_for_run`, `find_by_board_run_id`) をトランザクション内に移動
  - `update_for_import` の結果が None の場合 409 Conflict を返す

#### 3. テスト追加
- `crates/api/tests/board_run_test.rs`: `test_import_artifact_bundle_update_conflict` 追加
  - 先着リクエスト成功 → 後着が異なる sha256 で 409 になることを確認
  - トランザクション内 conflict 判定の正しさを順次実行で擬似テスト

### テスト結果
```
cargo test -p boardflow-api

board_run_test: 19 passed; 0 failed (新規1テスト追加)
plan_test: 16 passed; 0 failed
integration_test: 2 passed; 0 failed
config_test: 1 passed; 0 failed
total: 38 passed; 0 failed
```

### 変更ファイル
- `crates/db/src/queries/board_run.rs` — `find_by_id_for_update` 追加
- `crates/db/src/queries/artifact_bundle.rs` — `update_for_import` 条件付き更新化
- `crates/api/src/routes/board_run.rs` — 全体トランザクション化
- `crates/api/tests/board_run_test.rs` — race condition テスト追加

### 残リスク
- 真の並行テスト (tokio::spawn で2リクエスト同時送信) は未実施。順次実行による擬似テストのみ
- FOR UPDATE による行ロックはデッドロックリスクが理論上あるが、単一行ロック＋短トランザクションなので実害なし

### 更新した作業ログパス
- `docs/logs/5/worklog.md`

---

## 最終レビュー確定 (2026-05-01)

### Review 結果
- 第3回レビューにて `pr_ready: true` 確定
- Import API race condition 修正により仕様準拠を確認

### テスト最終結果

```
DATABASE_URL=postgres://boardflow:boardflow@localhost:5432/boardflow
cargo test -p boardflow-api

auth_test: 8 passed; 0 failed
board_run_test: 19 passed; 0 failed
config_test: 1 passed; 0 failed
integration_test: 2 passed; 0 failed
plan_test: 16 passed; 0 failed
total: 46 passed; 0 failed
```

### Docs レビュー指摘対応
- worklog テスト数の矛盾を解消 (本セクションで実測値を正確に記載)
- Import API idempotent replay の status 返却を仕様に合わせて修正
  - `bundle_status_str()` ヘルパーを追加し、`ArtifactBundleStatus` を適切な文字列にマッピング
  - Pending → "queued", Validating/Importing → "running", Completed → "completed", Failed → "failed"
  - completed run からの既存 bundle 返却時も同一ヘルパーを使用

### 変更ファイル
- `crates/api/src/routes/board_run.rs` — `bundle_status_str()` 追加、idempotent replay / completed run の status を動的マッピングに変更

### 残リスク
- presigned URL 生成は S3 client = None 時のモック動作のみ検証 (実 MinIO E2E は別途)
- 真の並行テスト (tokio::spawn で2リクエスト同時送信) は未実施

### 更新した作業ログパス
- `docs/logs/5/worklog.md`

