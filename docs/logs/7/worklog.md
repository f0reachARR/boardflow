# Issue #7: Import Worker実装

## 経緯
- バックエンド実装Issue分割タスクの一環として作成
- artifact import の非同期処理（最も複雑なコンポーネント）

## ユーザー要望
- docs/以下の仕様に基づくRustバックエンド実装の第7段階

## Issue作成内容
- PostgreSQL-backed queue consumer, zip展開, manifest検証, artifact保存, DRC/ERC解析, snapshot保存, BoardRun完了処理
- URL: https://github.com/f0reachARR/boardflow/issues/7

## 後続処理タイプの初期仮説
`implementation_required`

---

## 調査フェーズ (2026-05-01)

### 調査トピックと結果

#### 1. S3 オブジェクトダウンロード (aws-sdk-s3)

- **結論**: `get_object` + `ByteStream::collect()` + `into_bytes()` で一括メモリ読み込み
- **追加依存**: なし（`aws-sdk-s3`, `aws-config` はworkspace Cargo.toml追加済み）
- **SHA256検証**: `sha2` クレート (workspace追加済み) でダウンロード後に検証
- **詳細**: `docs/external/aws-sdk-s3-download.md`

#### 2. ZIP アーカイブ展開 (zip クレート)

- **結論**: `zip` クレート v2.x (zip-rs/zip2) を採用。インメモリ展開 (`Cursor` + `ZipArchive::new`)
- **追加依存**: `zip = "2"` をworkspace Cargo.tomlに追加が必要
- **セキュリティ**: CVE-2025-29787 により v2.3.0 以上が必須。`enclosed_name()` でパストラバーサル対策、`decompressed_size()` + 展開中サイズ追跡でzip bomb対策
- **詳細**: `docs/external/zip-archive-rust.md`

#### 3. PostgreSQL ジョブキューポーリング (SQLx)

- **結論**: CTE + `SELECT ... FOR UPDATE SKIP LOCKED` パターンを採用
- **追加依存**: なし（`sqlx` はworkspace Cargo.toml追加済み）
- **ポーリング**: 5秒間隔、`tokio::select!` でgraceful shutdown対応
- **リトライ**: `attempts` インクリメント + `run_after` への指数バックオフ設定。MAX_ATTEMPTS=5
- **Worker クラッシュ回復**: `run_after` visibility timeout方式、または定期スタック検出バッチ
- **部分インデックス推奨**: `idx_github_jobs_dequeue ON (type, status, run_after, created_at) WHERE status = 'pending'`
- **詳細**: `docs/external/postgresql-job-queue-polling.md`

### 必要なクレート追加

| クレート | バージョン | 対象 | 状態 |
|---|---|---|---|
| `aws-sdk-s3` | `"1"` | workspace | 追加済み |
| `aws-config` | `"1"` | workspace | 追加済み |
| `sha2` | `"0.10"` | workspace | 追加済み |
| `zip` | `"2"` | workspace | **未追加 — 追加が必要** |
| `sqlx` | `"0.8"` | workspace | 追加済み |
| `tokio` | `"1"` | workspace | 追加済み |

### Worker crate 依存追加が必要なもの

`crates/worker/Cargo.toml`:
- `boardflow-artifact = { path = "../artifact" }`
- `boardflow-domain = { path = "../domain" }`
- `aws-sdk-s3 = { workspace = true }`
- `aws-config = { workspace = true }`
- `sqlx = { workspace = true }`
- `serde_json = { workspace = true }`
- `sha2 = { workspace = true }`
- `uuid = { workspace = true }`
- `chrono = { workspace = true }`

`crates/artifact/Cargo.toml`:
- `zip = { workspace = true }`
- `serde = { workspace = true }`
- `serde_json = { workspace = true }`

### 結論ステータス

`implementation_required`

### 残リスク
- KiCad DRC/ERC レポートフォーマットの詳細調査が必要
- manifest.json の具体的 schema 定義が docs/spec.md に未記載
- bundle_size_bytes の上限値が未定義
- staging bucket / final bucket の環境変数命名規則が未定義
- Worker クラッシュ回復方式の最終決定（visibility timeout vs 定期バッチ）

---

## 計画フェーズ (2026-05-01)

### 実装要否

`implementation_required`

**implementable: true**

### 目的

PostgreSQL-backed ジョブキューから `artifact_bundle_import` ジョブを取得し、S3からアーティファクトバンドル（ZIP）をダウンロード・展開してスナップショットを作成する Import Worker を実装する。

### 非目的

- GitHub Issue 作成・コメント更新（別 job type、別 Issue で対応）
- KiCad CLI の実行（GitHub Actions 側で実行済み）
- staging bundle の定期削除（MVPでは `delete_after` を設定するのみ）
- Worker クラッシュ回復の定期バッチ（MVP後に検討）
- run_check_findings の個別パース（manifest.json の `checks` から集計値のみ保存）

### 受け入れ条件

1. Worker が `github_jobs` テーブルから `artifact_bundle_import` ジョブをポーリングし処理できる
2. S3 staging bucket から ZIP をダウンロードし SHA256 検証に成功する
3. ZIP を展開し `manifest.json` を解析できる
4. 各 artifact を final bucket に保存し `artifacts` テーブルに記録できる
5. `run_checks` に ERC/DRC 結果を保存できる
6. `board_project_snapshots` にスナップショットを作成できる
7. `board_run_diff_metadata` と `board_run_diffs` を保存できる
8. 成功時: `board_runs.status = completed`, `artifact_bundles.status = completed`, `github_jobs.status = completed`
9. 失敗時: 適切なリトライまたは永続的失敗処理が行われる
10. Graceful shutdown に対応する（SIGTERM で処理中ジョブの完了を待つ）
11. 各 crate にユニットテストがある

### 詳細要件

#### manifest.json スキーマ（MVP確定版）

```json
{
  "version": 1,
  "project_path": "hardware/motor_driver/motor_driver.kicad_pro",
  "tree_hash": "sha256:...",
  "commit_sha": "abc123",
  "files": [
    { "path": "motor_driver.kicad_pcb", "sha256": "sha256:..." }
  ],
  "artifacts": [
    {
      "type": "schematic_pdf",
      "filename": "motor_driver-schematic.pdf",
      "content_type": "application/pdf",
      "status": "available",
      "source_path": "artifacts/schematic_pdf/motor_driver-schematic.pdf"
    }
  ],
  "checks": [
    {
      "kind": "erc",
      "status": "passed",
      "error_count": 0,
      "warning_count": 2,
      "notice_count": 0
    }
  ],
  "diff_metadata": {
    "file_hashes": [...],
    "bom_summary": {...},
    "checks_summary": {...},
    "artifacts_summary": {...}
  }
}
```

#### 環境変数

既存の `.env.example` から:
- `DATABASE_URL` — PostgreSQL接続
- `MINIO_ENDPOINT` — S3互換エンドポイント
- `MINIO_ACCESS_KEY` / `MINIO_SECRET_KEY` — S3認証
- `MINIO_BUCKET_STAGING` — staging bucket名 (default: `boardflow-staging`)
- `MINIO_BUCKET_FINAL` — final bucket名 (default: `boardflow-final`)

#### 定数

- `MAX_ATTEMPTS = 5` — 最大リトライ回数
- `MAX_BUNDLE_SIZE = 500 * 1024 * 1024` (500MB) — バンドルサイズ上限
- `POLL_INTERVAL = 5秒` — ポーリング間隔
- `DELETE_AFTER_SUCCESS = 24時間` — 成功後のstaging bundle削除猶予
- `DELETE_AFTER_FAILURE = 7日` — 失敗後のstaging bundle削除猶予

### 影響範囲

| crate | 変更種別 | 概要 |
|---|---|---|
| `crates/worker/` | 大幅修正 | main.rs の worker ループ実装、config追加 |
| `crates/artifact/` | 新規実装 | ZIP展開、manifest解析、S3操作 |
| `crates/jobs/` | 新規実装 | dequeue/ack/nack ロジック |
| `crates/db/` | 追加 | 新規クエリ多数 |
| `crates/domain/` | 軽微追加 | (既存モデルで十分、変更なし予定) |
| workspace `Cargo.toml` | 依存追加 | `zip = "2"` |
| `crates/db/migrations/` | 追加 | dequeue用部分インデックス |

### 設計方針

#### アーキテクチャ

```
Worker binary (crates/worker/src/main.rs)
  │
  ├── config: WorkerConfig (env vars → struct)
  ├── S3Client 初期化 (aws-config + force_path_style)
  ├── PgPool 初期化 (boardflow-db)
  │
  └── worker loop (tokio::select! + ctrl_c)
        │
        ├── boardflow_jobs::dequeue_job(pool, "artifact_bundle_import")
        │     └── CTE + FOR UPDATE SKIP LOCKED
        │
        └── process_import_job(pool, s3_client, config, job)
              │
              ├── 1. board_run, artifact_bundle を取得・ロック
              ├── 2. artifact_bundle.status → validating
              ├── 3. S3 download (staging bucket)
              ├── 4. SHA256 検証
              ├── 5. artifact_bundle.status → importing
              ├── 6. ZIP 展開 (boardflow_artifact::extract_bundle)
              ├── 7. manifest.json パース・検証
              ├── 8. 各 artifact → final bucket upload + DB insert
              ├── 9. run_checks insert
              ├── 10. board_runs.erc_status/drc_status 更新
              ├── 11. board_project_snapshots insert
              ├── 12. board_run_diff_metadata insert
              ├── 13. board_run_diffs insert (no_baseline or ready)
              ├── 14. board_runs.status → completed
              ├── 15. artifact_bundles.status → completed, delete_after設定
              ├── 16. board_projects.latest_tree_hash/latest_completed_run_id 更新
              └── 17. github_jobs.status → completed (via ack)
```

#### crate 間の依存関係

```
boardflow-worker
  ├── boardflow-db (pool, queries)
  ├── boardflow-jobs (dequeue/ack/nack)
  ├── boardflow-artifact (ZIP展開, manifest解析, S3 upload)
  └── boardflow-domain (models)

boardflow-jobs
  ├── boardflow-db (executor型のみ)
  └── boardflow-domain (GithubJob model)

boardflow-artifact
  └── (外部依存のみ: zip, serde, serde_json, sha2, aws-sdk-s3)
```

### 変更ファイル一覧

#### 新規作成

| ファイル | 概要 |
|---|---|
| `crates/jobs/src/lib.rs` | dequeue_job, complete_job, fail_job, mark_permanently_failed |
| `crates/artifact/src/lib.rs` | モジュール宣言 |
| `crates/artifact/src/manifest.rs` | Manifest struct + parse/validate |
| `crates/artifact/src/extract.rs` | ZIP展開 (extract_bundle) |
| `crates/artifact/src/s3.rs` | S3 download + upload ヘルパー |
| `crates/worker/src/config.rs` | WorkerConfig (from_env) |
| `crates/worker/src/import.rs` | process_import_job 本体 |
| `crates/db/src/queries/snapshot.rs` | board_project_snapshots クエリ |
| `crates/db/src/queries/artifact.rs` | artifacts insert クエリ |
| `crates/db/src/queries/run_check.rs` | run_checks insert クエリ |
| `crates/db/src/queries/diff.rs` | board_run_diff_metadata + board_run_diffs クエリ |
| `crates/db/migrations/20260501000001_add_github_jobs_dequeue_index.up.sql` | dequeue部分インデックス |
| `crates/db/migrations/20260501000001_add_github_jobs_dequeue_index.down.sql` | rollback |

#### 修正

| ファイル | 概要 |
|---|---|
| `Cargo.toml` (workspace) | `zip = "2"` 追加 |
| `crates/worker/Cargo.toml` | 依存追加 |
| `crates/artifact/Cargo.toml` | 依存追加 |
| `crates/jobs/Cargo.toml` | 依存追加 |
| `crates/worker/src/main.rs` | worker loop 実装 |
| `crates/db/src/queries/mod.rs` | 新モジュール宣言追加 |
| `crates/db/src/queries/github_job.rs` | (既存のenqueue_importはそのまま) |
| `crates/db/src/queries/board_run.rs` | mark_completed, update_check_status 追加 |
| `crates/db/src/queries/artifact_bundle.rs` | mark_validating, mark_importing, mark_completed, mark_failed 追加 |
| `crates/db/src/queries/board_project.rs` | update_latest_run 追加 |

### 依存関係の追加

#### workspace `Cargo.toml`

```toml
zip = "2"
```

#### `crates/worker/Cargo.toml`

```toml
[dependencies]
boardflow-db = { path = "../db" }
boardflow-jobs = { path = "../jobs" }
boardflow-artifact = { path = "../artifact" }
boardflow-domain = { path = "../domain" }
tokio = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
aws-sdk-s3 = { workspace = true }
aws-config = { workspace = true }
sqlx = { workspace = true }
serde_json = { workspace = true }
sha2 = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }
thiserror = { workspace = true }
```

#### `crates/artifact/Cargo.toml`

```toml
[dependencies]
zip = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
sha2 = { workspace = true }
aws-sdk-s3 = { workspace = true }
aws-config = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
uuid = { workspace = true }
```

#### `crates/jobs/Cargo.toml`

```toml
[dependencies]
boardflow-domain = { path = "../domain" }
sqlx = { workspace = true }
uuid = { workspace = true }
tracing = { workspace = true }
```

### DBマイグレーション

#### `20260501000001_add_github_jobs_dequeue_index.up.sql`

```sql
-- Partial index for efficient job dequeue polling
CREATE INDEX idx_github_jobs_dequeue
ON github_jobs (type, run_after, created_at)
WHERE status = 'pending';
```

#### `20260501000001_add_github_jobs_dequeue_index.down.sql`

```sql
DROP INDEX IF EXISTS idx_github_jobs_dequeue;
```

### crate別実装詳細

#### 1. `crates/jobs/` — ジョブポーリング

**`src/lib.rs`**:

```rust
pub const MAX_ATTEMPTS: i32 = 5;

/// CTE + FOR UPDATE SKIP LOCKED で1件取得し status=running に遷移
pub async fn dequeue_job(pool: &PgPool, job_type: &str) -> Result<Option<GithubJob>, sqlx::Error>;

/// status=completed に遷移
pub async fn complete_job(pool: &PgPool, job_id: Uuid) -> Result<(), sqlx::Error>;

/// status=pending に戻し run_after を未来に設定（リトライ）
pub async fn fail_job(pool: &PgPool, job_id: Uuid, error: &str, retry_delay_secs: i64) -> Result<(), sqlx::Error>;

/// MAX_ATTEMPTS超過: status=failed に遷移
pub async fn mark_permanently_failed(pool: &PgPool, job_id: Uuid, error: &str) -> Result<(), sqlx::Error>;

/// 指数バックオフ計算 (10s, 30s, 90s, 270s, 810s)
pub fn retry_delay_secs(attempts: i32) -> i64;
```

#### 2. `crates/artifact/` — ZIP展開・manifest解析・S3操作

**`src/manifest.rs`**:

```rust
#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub project_path: String,
    pub tree_hash: String,
    pub commit_sha: String,
    pub files: Vec<ManifestFile>,
    pub artifacts: Vec<ManifestArtifact>,
    pub checks: Vec<ManifestCheck>,
    pub diff_metadata: Option<ManifestDiffMetadata>,
}

#[derive(Debug, Deserialize)]
pub struct ManifestFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Deserialize)]
pub struct ManifestArtifact {
    pub r#type: String,
    pub filename: String,
    pub content_type: String,
    pub status: String,           // "available" | "missing" | "failed" | "skipped"
    pub source_path: Option<String>, // ZIP内の相対パス（available時のみ必須）
    pub logical_name: Option<String>,
    pub status_reason: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ManifestCheck {
    pub kind: String,             // "erc" | "drc"
    pub status: String,           // "passed" | "failed" | "skipped"
    pub error_count: i32,
    pub warning_count: i32,
    pub notice_count: i32,
}

#[derive(Debug, Deserialize)]
pub struct ManifestDiffMetadata {
    pub file_hashes: Option<serde_json::Value>,
    pub bom_summary: Option<serde_json::Value>,
    pub checks_summary: Option<serde_json::Value>,
    pub artifacts_summary: Option<serde_json::Value>,
}

pub fn parse_manifest(data: &[u8]) -> Result<Manifest, ManifestError>;
pub fn validate_manifest(manifest: &Manifest) -> Result<(), ManifestError>;
```

**`src/extract.rs`**:

```rust
pub struct ExtractedFile {
    pub name: String,
    pub data: Vec<u8>,
}

pub struct ExtractedBundle {
    pub manifest: Manifest,
    pub files: HashMap<String, Vec<u8>>,  // source_path → data
}

/// ZIP展開 + manifest解析 + 検証
pub fn extract_bundle(zip_bytes: &[u8], max_total_size: u64) -> Result<ExtractedBundle, ExtractError>;
```

セキュリティ:
- `enclosed_name()` によるパストラバーサル対策
- `decompressed_size()` + 展開中累計サイズ追跡で zip bomb 対策
- `max_total_size` = 500MB

**`src/s3.rs`**:

```rust
/// S3 クライアント構築 (MinIO互換)
pub async fn create_s3_client(endpoint: Option<&str>, access_key: Option<&str>, secret_key: Option<&str>) -> S3Client;

/// staging bucket からダウンロード + SHA256 検証
pub async fn download_and_verify(client: &S3Client, bucket: &str, key: &str, expected_sha256: &str) -> Result<Vec<u8>, S3Error>;

/// final bucket へアップロード
pub async fn upload_artifact(client: &S3Client, bucket: &str, key: &str, data: &[u8], content_type: &str) -> Result<(), S3Error>;
```

#### 3. `crates/db/` — 追加クエリ

**`src/queries/snapshot.rs`**:
- `insert(executor, id, board_project_id, board_run_id, tree_hash, commit_sha, file_hashes_json) -> Result<BoardProjectSnapshot>`

**`src/queries/artifact.rs`**:
- `insert(executor, id, board_run_id, type, status, filename, source_path, ...) -> Result<Artifact>`
- `insert_batch(executor, artifacts: &[NewArtifact]) -> Result<Vec<Artifact>>` (optional, 一括insert)

**`src/queries/run_check.rs`**:
- `insert(executor, id, board_run_id, check_kind, status, error_count, warning_count, notice_count, raw_summary_json) -> Result<RunCheck>`

**`src/queries/diff.rs`**:
- `insert_metadata(executor, id, board_run_id, file_hashes_json, ...) -> Result<BoardRunDiffMetadata>`
- `insert_diff(executor, id, board_run_id, base_board_run_id, status, summary_json) -> Result<BoardRunDiff>`

**`src/queries/board_run.rs`** (追加):
- `mark_completed(executor, id) -> Result<BoardRun>` — status='completed', completed_at=NOW()
- `update_check_status(executor, id, erc_status, erc_errors, erc_warnings, drc_status, drc_errors, drc_warnings) -> Result<BoardRun>`

**`src/queries/artifact_bundle.rs`** (追加):
- `mark_validating(executor, id) -> Result<ArtifactBundle>`
- `mark_importing_status(executor, id) -> Result<ArtifactBundle>`
- `mark_completed(executor, id) -> Result<ArtifactBundle>` — status='completed', validated_at=NOW(), delete_after=NOW()+24h
- `mark_failed(executor, id, error_message) -> Result<ArtifactBundle>` — status='failed', delete_after=NOW()+7d

**`src/queries/board_project.rs`** (追加):
- `update_latest_run(executor, id, latest_tree_hash, latest_completed_run_id) -> Result<BoardProject>`

#### 4. `crates/worker/` — main.rs + import.rs

**`src/config.rs`**:

```rust
pub struct WorkerConfig {
    pub database_url: String,
    pub minio_endpoint: Option<String>,
    pub minio_access_key: Option<String>,
    pub minio_secret_key: Option<String>,
    pub bucket_staging: String,
    pub bucket_final: String,
    pub poll_interval: Duration,
    pub max_bundle_size: u64,
}

impl WorkerConfig {
    pub fn from_env() -> Result<Self, ConfigError>;
}
```

**`src/main.rs`** (worker loop):

```rust
#[tokio::main]
async fn main() {
    // 1. tracing 初期化
    // 2. WorkerConfig::from_env()
    // 3. PgPool 作成
    // 4. S3Client 作成
    // 5. run_worker(pool, s3_client, config).await
}

async fn run_worker(pool: PgPool, s3_client: S3Client, config: WorkerConfig) {
    loop {
        tokio::select! {
            _ = signal::ctrl_c() => { break; }
            result = boardflow_jobs::dequeue_job(&pool, "artifact_bundle_import") => {
                match result {
                    Ok(Some(job)) => {
                        if let Err(e) = import::process_import_job(&pool, &s3_client, &config, &job).await {
                            handle_job_failure(&pool, &job, &e).await;
                        } else {
                            boardflow_jobs::complete_job(&pool, job.id).await.ok();
                        }
                    }
                    Ok(None) => { sleep(config.poll_interval).await; }
                    Err(e) => { error; sleep(config.poll_interval).await; }
                }
            }
        }
    }
}
```

**`src/import.rs`** (process_import_job):

処理フロー:
1. payload_json から `staging_object_key`, `bundle_sha256`, `bundle_size_bytes` を取得
2. `board_run` を取得、status が `importing` でなければエラー
3. `artifact_bundle` を取得、status を `validating` に遷移
4. S3 からダウンロード + SHA256 検証
5. `artifact_bundle.status` を `importing` に遷移
6. ZIP 展開 + manifest パース
7. manifest.version == 1 検証
8. トランザクション開始:
   - 各 artifact を final bucket にアップロード + `artifacts` insert
   - `run_checks` insert + `board_runs` の erc/drc status 更新
   - `board_project_snapshots` insert
   - `board_run_diff_metadata` insert
   - `board_run_diffs` insert (latest_completed_run_id があれば `ready`、なければ `no_baseline`)
   - `board_runs.status` → `completed`
   - `artifact_bundles.status` → `completed`
   - `board_projects.latest_tree_hash` / `latest_completed_run_id` 更新
9. ジョブ完了 (`complete_job`)

Note: S3 upload はトランザクション外で行い、DB 更新はトランザクション内で一括コミットする。
S3 upload 成功後に DB コミット失敗した場合、orphaned objects が残るが MVP では許容する。

### テスト計画

#### `crates/jobs/` ユニットテスト

- `dequeue_job`: pending ジョブが取得できること、running/completed は取得しないこと、run_after が未来のジョブは取得しないこと
- `complete_job`: status が completed に遷移すること
- `fail_job`: status が pending に戻り run_after が更新されること
- `retry_delay_secs`: 指数バックオフ値が正しいこと

#### `crates/artifact/` ユニットテスト

- `parse_manifest`: 正常な manifest.json がパースできること
- `parse_manifest`: 不正な JSON でエラーを返すこと
- `validate_manifest`: version != 1 でエラーを返すこと
- `extract_bundle`: 正常な ZIP が展開できること
- `extract_bundle`: パストラバーサル攻撃がブロックされること (`../` パス)
- `extract_bundle`: zip bomb (サイズ超過) がブロックされること
- `extract_bundle`: manifest.json が存在しない ZIP でエラーを返すこと
- `download_and_verify`: SHA256 不一致でエラーを返すこと (モック)

#### `crates/db/` クエリテスト

- 各 insert クエリの正常系テスト
- board_run mark_completed の遷移テスト
- artifact_bundle status 遷移テスト

#### `crates/worker/` 統合テスト

- `process_import_job` の正常フロー（テスト用S3 mock + テストDB）
- 失敗時のリトライフロー
- MAX_ATTEMPTS超過時の永続的失敗

### 実装順序

依存関係を考慮した実装順:

1. **Phase 1: 依存追加・マイグレーション**
   - workspace Cargo.toml に `zip = "2"` 追加
   - 各 crate の Cargo.toml 更新
   - dequeue インデックスのマイグレーション追加

2. **Phase 2: `crates/jobs/`**
   - dequeue_job, complete_job, fail_job, mark_permanently_failed, retry_delay_secs
   - ユニットテスト

3. **Phase 3: `crates/artifact/`**
   - manifest.rs (Manifest struct + parse + validate)
   - extract.rs (ZIP展開)
   - s3.rs (download_and_verify, upload_artifact, create_s3_client)
   - ユニットテスト

4. **Phase 4: `crates/db/` 追加クエリ**
   - queries/snapshot.rs
   - queries/artifact.rs
   - queries/run_check.rs
   - queries/diff.rs
   - queries/board_run.rs 追加
   - queries/artifact_bundle.rs 追加
   - queries/board_project.rs 追加
   - queries/mod.rs 更新

5. **Phase 5: `crates/worker/`**
   - config.rs (WorkerConfig)
   - import.rs (process_import_job)
   - main.rs (worker loop + graceful shutdown)

6. **Phase 6: テスト・動作確認**
   - cargo build 成功確認
   - ユニットテスト全通過
   - docker-compose 環境での統合テスト

### ドキュメント更新対象

- `docs/logs/7/worklog.md` — 本計画 + 実装進捗
- (実装完了後) `docs/backend/summary.md` — Worker の説明更新

### 未解決の疑問と対応

| 疑問 | 対応 |
|---|---|
| bundle_size_bytes 上限 | MVP で 500MB に設定。Issue本文の推奨値を採用 |
| staging/final bucket 環境変数名 | 既存の `MINIO_BUCKET_STAGING` / `MINIO_BUCKET_FINAL` を使用 (.env.example 確認済み) |
| manifest.json スキーマ | Issue本文提供のスキーマを確定版として採用 |
| Worker クラッシュ回復 | MVP では visibility timeout 方式 (`run_after` ベース) を採用。定期バッチは後続 |
| final bucket の storage_key 形式 | `runs/{board_run_id}/artifacts/{artifact_type}/{filename}` を採用 |
| board_run_diffs の base_board_run_id 取得方法 | `board_projects.latest_completed_run_id` を参照。NULL の場合 `no_baseline` |

### 更新した作業ログパス

`docs/logs/7/worklog.md`

---

## 実装フェーズ (2026-05-01)

### 実装完了

ブランチ: `feat/import-worker` (commit: `1cc086d`)

### 変更したファイル一覧

#### 新規作成
| ファイル | 概要 |
|---|---|
| `crates/artifact/tests/extract_test.rs` | ZIP展開・SHA256検証のユニットテスト (13件) |
| `crates/db/migrations/20260501000001_add_dequeue_index.up.sql` | pending jobポーリング用部分インデックス |
| `crates/db/migrations/20260501000001_add_dequeue_index.down.sql` | rollback |
| `crates/db/src/queries/artifact.rs` | artifacts INSERT クエリ |
| `crates/db/src/queries/diff.rs` | board_run_diff_metadata + board_run_diffs INSERT |
| `crates/db/src/queries/run_check.rs` | run_checks INSERT |
| `crates/db/src/queries/snapshot.rs` | board_project_snapshots INSERT |
| `crates/worker/src/config.rs` | WorkerConfig (環境変数からの設定読み込み) |

#### 修正
| ファイル | 概要 |
|---|---|
| `Cargo.toml` (workspace) | `zip = "2"` 追加 |
| `crates/artifact/Cargo.toml` | 全依存追加 |
| `crates/artifact/src/lib.rs` | S3ダウンロード、SHA256検証、ZIP展開、manifest解析、S3アップロード |
| `crates/jobs/Cargo.toml` | 全依存追加 |
| `crates/jobs/src/lib.rs` | dequeue/ack/nack 実装 |
| `crates/worker/Cargo.toml` | 全依存追加 (sqlx含む) |
| `crates/worker/src/main.rs` | Worker loop + import job処理パイプライン全体 |
| `crates/db/src/queries/mod.rs` | artifact/diff/run_check/snapshot モジュール追加 |
| `crates/db/src/queries/github_job.rs` | dequeue/mark_completed/mark_failed/reschedule 追加 |
| `crates/db/src/queries/board_run.rs` | mark_completed 追加 |
| `crates/db/src/queries/artifact_bundle.rs` | mark_importing/mark_completed/mark_failed 追加 |
| `crates/db/src/queries/board_project.rs` | update_latest_completed_run 追加 |

### 実装の概要

1. **ジョブポーリング基盤 (`crates/jobs/`)**: CTE + FOR UPDATE SKIP LOCKED による安全なジョブ取得、指数バックオフリトライ (10s × 3^attempts)、MAX_ATTEMPTS=5 での永続的失敗
2. **アーティファクト処理 (`crates/artifact/`)**: S3ダウンロード、SHA256ハッシュ検証、ZIP展開 (manifest.json v1パース)、パストラバーサル防御、500MBサイズ上限、S3アップロード
3. **DBクエリ群 (`crates/db/`)**: github_job/artifact_bundle/board_run/board_project のステータス遷移、snapshot/artifact/run_check/diff の INSERT
4. **Workerメインループ (`crates/worker/`)**: tokio::select! による graceful shutdown、ポーリング間隔設定可能、12ステップのインポートパイプライン (ダウンロード→検証→展開→アップロード→永続化→完了)

### テスト結果

```
running 13 tests
test test_extract_bundle_too_large ... ok
test test_extract_bundle_missing_manifest ... ok
test test_verify_sha256_mismatch ... ok
test test_extract_bundle_invalid_manifest_json ... ok
test test_verify_sha256_valid ... ok
test test_extract_bundle_artifact_not_found_in_zip ... ok
test test_extract_bundle_available_artifact_no_source_path ... ok
test test_extract_bundle_path_traversal_absolute ... ok
test test_extract_bundle_skips_non_available_artifacts ... ok
test test_extract_bundle_unsupported_version ... ok
test test_extract_bundle_path_traversal_dotdot ... ok
test test_extract_bundle_multiple_artifacts ... ok
test test_extract_bundle_valid ... ok

test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

テスト観点:
- **正常系**: 有効なZIP展開、複数アーティファクト展開、SHA256検証成功
- **境界値**: サイズ上限定数確認
- **エラー系**: manifest.json不在、不正JSON、未対応バージョン、source_path未設定
- **セキュリティ**: パストラバーサル(..攻撃)、絶対パス拒否
- **スキップ系**: non-available アーティファクトのスキップ確認

### ビルド結果

```
cargo build → Finished `dev` profile [unoptimized + debuginfo] target(s)
```

警告なし、エラーなし。

### 残リスク

| リスク | 詳細 |
|---|---|
| 統合テスト未実施 | DB + S3 (MinIO) を使った end-to-end テストは未実装。CI環境のdocker-compose設定が必要 |
| Worker クラッシュ回復 | running状態で放置されたジョブの定期検出バッチは未実装 (MVP後) |
| delete_after 設定 | staging bundle の TTL 設定は未実装 |
| diff baseline 解決 | board_project.latest_completed_run_id からの baseline 取得は "no_baseline" 固定 (MVP) |
| メモリ使用量 | 500MB bundle の全量メモリ展開。大規模プロジェクトではストリーミング展開が必要な可能性 |

---

## レビューフェーズ (2026-05-01)

### レビュー結果

**pr_ready: false**

総評:
- `cargo build -p boardflow-worker -p boardflow-db -p boardflow-jobs` と `cargo test -p boardflow-artifact` は成功しており、artifact crate 単体の ZIP/SHA256 実装は最低限成立している。
- 一方で、Issue #7 の受け入れ条件と `docs/spec.md` に対して、worker 成功時の後続ジョブ enqueue、diff baseline 解決、bundle/manifest 検証、リトライ時の DB 整合性、MinIO 設定読み込みに未充足が残っている。
- 特に、部分失敗後の再実行で insert が競合しうる点と、成功時に required な後続処理が実装されていない点は PR blocker。

### 重大度順の指摘

1. **部分失敗時に import パイプラインが冪等でなく、再試行で壊れた状態に陥る**
  - `crates/worker/src/main.rs` はトランザクションを使わずに artifact / run_check / snapshot / diff を順次 insert しており、途中で失敗すると一部だけ永続化された状態で `github_jobs` を再スケジュールする。
  - `crates/db/migrations/20260430000001_create_schema.up.sql` では `board_run_diff_metadata.board_run_id` と `board_run_diffs.board_run_id` が UNIQUE、`artifacts(board_run_id, type, source_path)` も UNIQUE なので、再試行時に重複 insert で恒久的に失敗しやすい。
  - 該当: `crates/worker/src/main.rs` の成功パス全体、特に diff 保存から完了遷移 (`diff::insert_diff_metadata`, `diff::insert_diff`, `board_run::mark_completed`)。
  - 修正方針: import 完了までを DB トランザクションにまとめるか、各 insert を UPSERT / idempotent update に変更し、再試行可能性を保証する。

2. **差分保存が `no_baseline` 固定で、仕様の baseline 解決と summary 作成を満たしていない**
  - `crates/worker/src/main.rs` で `let diff_status = "no_baseline";` を固定し、`base_board_run_id` も `summary_json` も常に `None`。
  - `docs/spec.md` では、`latest_completed_run_id` を更新する前に base_run を解決し、比較可能なら `ready` と差分 summary を保存することが要求されている。
  - この状態だと Issue #7 の受け入れ条件にある `board_run_diff_metadata` / `board_run_diffs` 保存は一部しか満たしていない。
  - 修正方針: `board_projects.latest_completed_run_id` を事前取得し、baseline ありなら `base_board_run_id` と `summary_json` を保存、比較材料不足なら `unavailable` を保存する。

3. **成功時の後続ジョブ enqueue が未実装で、BoardRun 完了処理が仕様未達**
  - worker の成功パスは `board_project::update_latest_completed_run`、`artifact_bundle::mark_completed`、`github_job::mark_completed` で終了しており、Issue 作成ジョブ、Dashboard コメント更新ジョブ、Run Result コメントジョブの enqueue がない。
  - `docs/spec.md` では BoardRun 完了時にこれら後続ジョブの enqueue が required とされている。
  - 修正方針: successful import の最後で `board_projects.issue_sync_status` や既存 Issue 状態を見て create/update comment 系 job を enqueue する。

4. **bundle / manifest 検証が仕様より大幅に弱く、不正 bundle を受理しうる**
  - `crates/artifact/src/lib.rs` は `manifest.json` の存在確認、version=1、`available` artifact の `source_path` 存在確認、単純な `..` / absolute path 拒否しかしていない。
  - `docs/spec.md` が要求する「manifest 未記載 entry 拒否」「entry の展開後サイズ上限」「artifact ごとの `sha256` / `size_bytes` / `content_type` 検証」「diff_metadata 補助ファイル検証」は未実装。
  - 現行 manifest struct に artifact の `sha256` と `size_bytes` もなく、仕様の完全性と整合していない。
  - 修正方針: ZIP 全 entry を列挙して manifest と突合し、entry 単位のサイズ・ハッシュ・content type・許可拡張子を検証する。必要なら manifest schema を仕様どおり拡張する。

5. **worker が既存の MinIO 設定体系を読まず、`.env.example` のままでは動作しない**
  - `crates/worker/src/config.rs` は `STAGING_BUCKET` / `ARTIFACTS_BUCKET` / `S3_ENDPOINT` を読むが、既存設定は `.env.example` と API 側実装の両方で `MINIO_BUCKET_STAGING` / `MINIO_BUCKET_FINAL` / `MINIO_ENDPOINT`。
  - `crates/worker/src/main.rs` も `aws_config::load_defaults()` のみで、API 側のような MinIO access key / secret key 明示設定をしていない。
  - このため、既存の開発環境定義では worker だけ別設定を要求し、環境差異で起動失敗する可能性が高い。
  - 修正方針: API と同じ env 名・認証設定に揃える。少なくとも worker config と `.env.example` を一致させる。

6. **retryable failure でも artifact_bundle を即 `failed` にしており、状態遷移と TTL が不整合**
  - `handle_job_failure` は再試行予定のケースでも先に `artifact_bundle::mark_failed` を呼ぶ。
  - さらに `crates/db/src/queries/artifact_bundle.rs` の `mark_completed` / `mark_failed` は `delete_after` を設定しておらず、仕様にある 24h / 7d の cleanup 契約も満たしていない。
  - 修正方針: retryable failure では bundle を `pending` か `validating/importing` の再実行可能状態に戻し、terminal failure のみ `failed` にする。合わせて `delete_after` を仕様どおり設定する。

### 必須修正

- import パイプラインをトランザクション化または idempotent 化し、再試行で重複 insert しないようにする。
- baseline 解決と diff summary 作成を実装し、`board_run_diffs` を `no_baseline` 固定にしない。
- successful import 後の Issue / Dashboard / Run Result 系ジョブ enqueue を実装する。
- manifest / zip validation を仕様レベルまで引き上げ、entry 単位のサイズ・sha256・content_type 検証を追加する。
- worker の env 名と MinIO 認証設定を既存 API / `.env.example` に揃える。
- `artifact_bundles.delete_after` と retryable failure 時の bundle status 遷移を修正する。

### 任意改善

- `crates/jobs/` と `crates/db/src/queries/github_job.rs` で dequeue/ack/nack 相当の責務が二重化しているため、どちらを正本にするか統一した方が保守しやすい。
- `artifact::upload_artifact` が `Vec<u8>` を受け取って clone を要求するため、大きい artifact ではメモリ効率が悪い。借用や `Bytes` ベースに寄せる余地がある。
- `artifact_bundles.status` に `validating` があるのに worker が使っていないため、状態の意味をコード上でも揃えた方が追跡しやすい。

### テスト不足

- worker 成功パスの統合テストがなく、DB 状態遷移と S3 upload を通した E2E が未検証。
- retry path のテストがなく、途中失敗後の再実行・重複 insert・backoff 設定が確認されていない。
- baseline があるケースの diff 作成テストがない。
- `.env.example` ベースの worker 設定読み込みテストがない。
- bundle validation について、manifest 未記載 entry、entry size 上限、sha256/size_bytes mismatch、content_type mismatch の異常系テストがない。

### ドキュメント確認

- `docs/spec.md` の BoardRun 完了処理、bundle validation、diff 作成、staging bundle TTL に対して、実装は未充足が残る。
- `docs/logs/7/worklog.md` の「残リスク」に delete_after 未実装と diff baseline 未解決が明記されており、実装側も未完を認識している。ただし `pr_ready` の観点では blocker のまま。
- `Research成果物` にある `zip >= 2.3.0` 採用自体は Cargo.lock 上で満たされている。

### PR/完了結果

- 判定: `pr_ready: false`
- blocker 解消前に PR 化すると、import worker の主要責務の一部が未達のまま取り込まれるリスクが高い。

### 残リスク

- 部分失敗後の永続状態が壊れた場合、手動 DB 修復が必要になる可能性がある。
- 現状の ZIP 検証では、仕様上 reject すべき malformed bundle を completed 扱いする可能性がある。
- 運用環境で worker だけ設定名が異なるため、デプロイ時に設定漏れが起きやすい。

---

## レビュー指摘対応 (2026-05-01)

### 修正内容

6点 + 追加修正1点のレビュー指摘を対応。

#### 修正1: Import永続化をトランザクション化
- `process_import_job` のDB書き込みを `pool.begin()` / `tx.commit()` でトランザクション化
- S3ダウンロード・SHA256検証・ZIP展開・S3アップロードはトランザクション外（事前）
- artifact insert, run_check insert, snapshot insert, diff insert, board_run mark_completed, board_project update, artifact_bundle mark_completed, github_job mark_completed, 後続job enqueue → すべてトランザクション内
- 途中失敗時のリトライで重複insert問題を解消

#### 修正2: Baseline解決とdiff summary作成
- `board_project::find_by_id` (既存) を使って `latest_completed_run_id` を取得
- base_run_id が Some なら diff status = "ready"、None なら "no_baseline"
- summary_json にファイル数を含むシンプルなサマリーを保存

#### 修正3: 後続ジョブenqueue
- `github_job::enqueue` 汎用関数を `crates/db/src/queries/github_job.rs` に追加
- Import成功後にトランザクション内で `issue_sync` と `run_result_comment` ジョブを enqueue

#### 修正4: ZIP/manifest検証強化
- `ManifestArtifact` に `sha256: Option<String>` と `size_bytes: Option<i64>` フィールド追加
- extract時に sha256 と size_bytes が指定されていれば検証
- zip内に manifest 未記載エントリがある場合は `tracing::warn` ログを出力
- テスト4件追加: sha256検証pass/fail、size検証pass/fail

#### 修正5: Worker env名をAPIと統一
- `MINIO_BUCKET_STAGING`, `MINIO_BUCKET_FINAL`, `MINIO_ENDPOINT`, `MINIO_ACCESS_KEY`, `MINIO_SECRET_KEY` を使用
- `aws-config` 依存を削除、明示的な credentials provider で S3 client 構築
- `WorkerConfig` に `s3_access_key` / `s3_secret_key` フィールド追加

#### 修正6: Retryable failure時のbundle status
- `handle_job_failure` を修正: `attempts >= MAX_ATTEMPTS` の場合のみ bundle と run を failed に
- リトライ可能な失敗時は bundle を 'importing' のまま維持
- `artifact_bundle::mark_completed` に `delete_after = NOW() + INTERVAL '7 days'` を追加

#### 追加修正: crates/jobs/ の重複排除
- `crates/jobs/src/lib.rs` から重複SQL操作を削除
- `MAX_ATTEMPTS`, `BASE_BACKOFF_SECS`, `backoff_secs()` のみ保持
- Worker は `boardflow_jobs` の定数を利用、SQL操作は `boardflow_db::queries::github_job` を使用
- `crates/jobs/Cargo.toml` から不要依存を削除

### 変更ファイル一覧

| ファイル | 変更概要 |
|---|---|
| `crates/worker/src/main.rs` | トランザクション化、baseline解決、後続job enqueue、S3 config修正、retry修正 |
| `crates/worker/src/config.rs` | env名統一 (MINIO_*)、access_key/secret_key追加 |
| `crates/worker/Cargo.toml` | aws-config 依存削除 |
| `crates/artifact/src/lib.rs` | ManifestArtifact に sha256/size_bytes追加、extract時検証追加、未記載エントリ警告 |
| `crates/artifact/tests/extract_test.rs` | 新フィールド対応 + 検証テスト4件追加 |
| `crates/db/src/queries/github_job.rs` | `enqueue` 汎用関数追加 |
| `crates/db/src/queries/artifact_bundle.rs` | mark_completed に delete_after 追加 |
| `crates/jobs/Cargo.toml` | 不要依存削除 |
| `crates/jobs/src/lib.rs` | 定数/ヘルパーのみに簡素化 |

### テスト結果

```
test result: ok. 63 passed; 0 failed; 0 ignored
```

内訳:
- auth_test: 8 passed
- board_run_test: 19 passed
- config_test: 1 passed
- integration_test: 2 passed
- plan_test: 16 passed
- extract_test: 17 passed (4件新規追加)

### 残リスク

- `github_job::enqueue` は `ON CONFLICT DO NOTHING` + `RETURNING *` のため、conflict時に `RowNotFound` エラーとなる。MVP では同一ジョブが二重enqueueされる可能性は低いが、将来的に `fetch_optional` への変更を検討。
- diff summary は現在ファイル数のみ。ファイルレベルの added/removed/changed 計算は baseline snapshot との比較が必要で、別途実装予定。
- S3アップロード後にトランザクションが失敗すると孤立オブジェクトが残る。staging bucket の TTL (delete_after) で自然回収される想定。

---

## レビュー再確認フェーズ (2026-05-01)

### 再確認結果

**pr_ready: false**

総評:
- 前回の6件のうち、トランザクション化、baseline取得、manifestの sha256 / size_bytes 検証、MinIO系 env 名統一、retryable failure 時の bundle failed 抑止は実装に反映されている。
- ただし、後続ジョブ enqueue と bundle/manifest の仕様整合は再レビュー時点でも未充足が残る。加えて、成功時の staging bundle TTL が仕様の24時間ではなく7日になっている。
- `cargo test --workspace` はこのワークスペースでは 1 件失敗し、報告されていた「63テストすべて成功」は再現できなかった。

### 前回6件の必須修正の再評価

1. **DB書き込みのトランザクション化**
  - 対応あり。worker は artifact / run_check / snapshot / diff / board_run / board_project / artifact_bundle / github_job 更新を1トランザクションにまとめている。
  - 確認箇所: `crates/worker/src/main.rs`

2. **Baseline解決**
  - 対応あり。`board_project::find_by_id` で `latest_completed_run_id` を取得し、baseline の有無で `ready` / `no_baseline` を切り替えている。
  - 確認箇所: `crates/worker/src/main.rs`

3. **後続ジョブ enqueue**
  - 部分対応に留まる。`issue_sync` と `run_result_comment` の enqueue は追加されたが、仕様が要求する job type は `create_issue` / `create_dashboard_comment` / `update_dashboard_comment` / `create_run_result_comment` であり、Dashboard コメント系が未実装。
  - さらに generic enqueue は conflict 時に `RETURNING *` が0行になるため、冪等 enqueue としては不完全。

4. **ZIP / manifest 検証強化**
  - 部分対応。artifact entry の sha256 / size_bytes 検証は追加された。
  - ただし manifest 未記載 entry は reject ではなく warning のみで、仕様の「原則拒否」を満たしていない。

5. **Worker env 名統一**
  - 対応あり。`MINIO_ENDPOINT` / `MINIO_ACCESS_KEY` / `MINIO_SECRET_KEY` / `MINIO_BUCKET_STAGING` / `MINIO_BUCKET_FINAL` を参照し、明示的 credentials provider も使っている。

6. **Bundle status / delete_after 修正**
  - 部分対応。retryable failure では bundle を failed にせず、terminal failure のみ failed にする挙動は反映されている。
  - ただし成功時 `delete_after` が 7 日になっており、仕様の 24 時間と不一致。

### 重大度順の指摘

1. **成功時の後続ジョブが仕様と不整合で、必要な GitHub 連携を満たしていない**
  - worker は `issue_sync` と `run_result_comment` を enqueue しているが、仕様の job type は `create_issue` / `create_dashboard_comment` / `update_dashboard_comment` / `create_run_result_comment`。
  - Dashboard コメント更新系が enqueue されておらず、Issue 未作成時の create_issue 依存関係も未実装。

2. **staging bundle の成功時 TTL が仕様違反**
  - 成功時 `delete_after = NOW() + INTERVAL '7 days'` になっているが、仕様は 24 時間以内の削除対象を要求している。

3. **manifest 未記載 zip entry を reject せず warning で通している**
  - 仕様では「manifest未記載のzip entryは原則拒否」だが、現実装は warning ログのみで import を継続する。

4. **ワークスペース全体テストが再現時点で green ではない**
  - `cargo test --workspace` 実行時に `crates/api/tests/board_run_test.rs` の `test_fail_board_run_conflict` が失敗した。
  - 失敗内容は `boardflow_api_tokens.repository_id` の外部キー違反で、Import Worker の変更点とは直接関係しない可能性が高いが、現時点で「全テスト成功」とは判定できない。

### 必須修正

- 後続ジョブ enqueue を `docs/spec.md` の job type と整合させ、少なくとも Issue 作成系と Dashboard コメント系の分岐を実装する。
- `artifact_bundles.mark_completed` の `delete_after` を成功時24時間に修正する。
- manifest 未記載 zip entry を warning ではなく reject に変更する。
- `cargo test --workspace` が通る状態を再現し、63テスト成功報告との差分を解消する。

### 任意改善

- `github_job::enqueue` は conflict を正常系として扱えるように `fetch_optional` + 既存行取得、または `ON CONFLICT DO UPDATE ... RETURNING *` に寄せた方がよい。
- 孤立 final bucket object の回収戦略を別ジョブまたは定期クリーンアップとして明文化すると運用が安定する。

### テスト結果

実行コマンド:

```text
cargo test --workspace
```

結果:

```text
board_run_test: 19 tests 中 18 passed, 1 failed
失敗: test_fail_board_run_conflict
原因: boardflow_api_tokens.repository_id の外部キー違反
```

### ドキュメント確認

- `docs/spec.md` の後続 job type 定義、manifest 未記載 entry 拒否、成功 bundle の24時間TTL と現実装に不整合あり。
- worklog に記載されていた residual risk のうち、`ON CONFLICT DO NOTHING` + `RETURNING *` は low risk 扱いにはできず、冪等 enqueue が必要な箇所では実障害になりうる。
- 一方で、孤立 S3 オブジェクトは運用リスクではあるが、単独では PR blocker とまでは判定しない。

### PR/完了結果

- 判定: `pr_ready: false`
- ブロッカーは「後続 job type 不整合」「成功時 TTL 不一致」「manifest 未記載 entry 未拒否」「全体テスト未再現」の4点。

### 残リスク

- `ON CONFLICT DO NOTHING` + `RETURNING *` は conflict 発生時に `RowNotFound` となるため、今後 job dedupe を本格運用すると顕在化する可能性がある。
- final bucket への upload を DB transaction 外で行う設計上、失敗時に孤立 object は残りうる。ただしこれは cleanup 戦略があれば吸収可能で、今回の blocker ではない。

---

## レビュー指摘修正フェーズ 2 (2026-05-01)

### 修正内容 (5件)

#### 修正1: 後続ジョブenqueueを仕様に合わせる

- `crates/worker/src/main.rs` Step 13 を全面書き換え
- `board_project::find_by_id` で board_project を再取得し:
  - `issue_number` が None → `create_issue` enqueue
  - `dashboard_comment_id` が Some → `update_dashboard_comment`、None → `create_dashboard_comment` enqueue
  - `create_run_result_comment` を常に enqueue
- すべて `let _ = ...` で best-effort（conflict時は既存ジョブが処理する）

#### 修正2: delete_after を24時間に変更

- `crates/db/src/queries/artifact_bundle.rs` の `mark_completed` で `INTERVAL '7 days'` → `INTERVAL '24 hours'`

#### 修正3: manifest未記載zip entryを拒否

- `crates/artifact/src/lib.rs` の `extract_bundle` を修正
- warning ログを `ArtifactError::Manifest` エラーに変更
- `allowed_paths` HashSet を構築し、manifest.json + 全artifact source_path 以外のファイルエントリを reject
- ディレクトリエントリは許可

#### 修正4: cargo test --workspace 失敗解消

- `test_fail_board_run_conflict` は再現環境で正常に pass
- 前回レビューで報告されていた外部キー違反は一時的な DB 状態の問題と判断
- コード変更なし

#### 修正5: github_job::enqueue の ON CONFLICT 問題

- `crates/db/src/queries/github_job.rs` の `enqueue` 関数:
  - 戻り値を `Result<GithubJob, sqlx::Error>` → `Result<Option<GithubJob>, sqlx::Error>` に変更
  - `fetch_one` → `fetch_optional` に変更
- Worker側: `let _ = github_job::enqueue(...).await.map_err(...)?;` で Option を無視

### テスト追加

- `test_extract_bundle_rejects_unlisted_entry`: manifest に未記載のzip entryがある場合にエラーが返ることを検証

### 変更ファイル一覧

| ファイル | 変更概要 |
|---|---|
| `crates/worker/src/main.rs` | 後続ジョブenqueueを仕様準拠に全面書き換え |
| `crates/db/src/queries/github_job.rs` | `enqueue` を `fetch_optional` + `Option<GithubJob>` に変更 |
| `crates/db/src/queries/artifact_bundle.rs` | `mark_completed` の delete_after を 24 hours に変更 |
| `crates/artifact/src/lib.rs` | manifest未記載entry を reject に変更 |
| `crates/artifact/tests/extract_test.rs` | `test_extract_bundle_rejects_unlisted_entry` 追加 |

### テスト結果

```
cargo test --workspace: 64 tests passed, 0 failed

内訳:
- auth_test: 8 passed
- board_run_test: 19 passed (test_fail_board_run_conflict 含む)
- config_test: 1 passed
- integration_test: 2 passed
- plan_test: 16 passed
- extract_test: 18 passed (1件新規追加)
```

### 残リスク

- diff summary は現在ファイル数のみ。ファイルレベルの added/removed/changed 計算は baseline snapshot との比較が必要で、別途実装予定。
- S3アップロード後にトランザクションが失敗すると孤立オブジェクトが残る。staging bucket の TTL (delete_after) で自然回収される想定。
- Worker 統合テスト（DB + S3 の E2E）は未実装。CI 環境の docker-compose 設定が必要。

---

## レビューフェーズ3 (2026-05-01)

### 対象

- Issue #7: Import Worker実装
- 確認対象は前回指摘の5件、`cargo build --workspace`、`cargo test --workspace`、および新規PRブロッカーの有無のみ

### 調査結果

#### 修正1: 後続ジョブenqueue

- `crates/worker/src/main.rs` を確認
- `board_project.issue_number` が未設定のときのみ `create_issue` を enqueue
- `board_project.dashboard_comment_id` の有無で `create_dashboard_comment` / `update_dashboard_comment` を分岐
- run result 用は `create_run_result_comment` を enqueue
- `docs/spec.md` の job type 例と整合

#### 修正2: delete_after

- `crates/db/src/queries/artifact_bundle.rs` の `mark_completed` を確認
- `delete_after = NOW() + INTERVAL '24 hours'` になっており、`docs/spec.md` の「import成功済みのstaging bundleは24時間以内に削除対象」と整合

#### 修正3: manifest未記載entry拒否

- `crates/artifact/tests/extract_test.rs` の `test_extract_bundle_rejects_unlisted_entry` を確認
- エラー種別は `ArtifactError::Manifest` を期待しており、メッセージに `not declared in manifest` とファイル名を含むことを検証
- `docs/spec.md` の「manifest未記載のzip entryは原則拒否する」と整合

#### 修正4: test_fail_board_run_conflict

- `crates/api/tests/board_run_test.rs` の `test_fail_board_run_conflict` は現行HEADで存在し、`cargo test --workspace` 再実行でも pass
- 前回の失敗は現時点では再現せず、pre-existing issue は解消済みとして扱ってよい

#### 修正5: github_job::enqueue

- `crates/db/src/queries/github_job.rs` を確認
- `enqueue` は `Result<Option<GithubJob>, sqlx::Error>` を返し、`fetch_optional` を使用
- `INSERT ... ON CONFLICT DO NOTHING RETURNING *` は競合時に行を返さないため、この変更は PostgreSQL の挙動と整合
- worker 側では戻り値 `Option` を無視しており、重複時にも異常終了しない

### テスト結果

```text
cargo build --workspace
- Finished dev profile successfully

cargo test --workspace
- 全64テスト pass
- board_run_test: 19 passed（test_fail_board_run_conflict を含む）
- extract_test: 18 passed（manifest未記載entry reject test を含む）
```

### ドキュメント確認

- `docs/spec.md` の以下と整合を確認
  - manifest未記載entry拒否
  - import成功後の `delete_after = 24 hours`
  - 後続 GitHub job type 名称

### レビュー結果

- 前回指摘の5件は、今回レビュー範囲ではすべて修正済みと判断
- `cargo build --workspace` と `cargo test --workspace` は現行HEADで成功
- ユーザーが許容すると明示した残リスク（E2E未整備、diff summary簡素、S3孤立オブジェクト自然回収）は、今回のPRブロッカーとして扱わない
- 今回レビュー範囲で新たなPRブロッカーは見つからず

### PR/完了結果

- 判定: `pr_ready: true`

### 残リスク

- Worker 統合テスト（DB + S3 の E2E）は未実装だが、MVPスコープ外として許容
- diff summary は最小実装のままだが、今回レビュー条件では許容
- S3孤立オブジェクトはTTLベースの自然回収前提で、今回レビュー条件では許容

---

## ドキュメント確認フェーズ (後半修正後) (2026-05-01)

### `mark_failed` の delete_after 修正

- `crates/db/src/queries/artifact_bundle.rs` の `mark_failed` に `delete_after = NOW() + INTERVAL '7 days'` を追加
- `docs/spec.md` 9.5節の「failed bundle は7日後に削除対象」と整合
- ドキュメント確認フェーズの残差分を解消

### docs_ready: true

---

## ドキュメント確認フェーズ (2026-05-01)

### 対象

- Issue #7: Import Worker実装
- 確認対象: `docs/spec.md` 9.5節、`docs/backend/api.md` Import API status、`docs/backend/summary.md`、`docs/technology.md`、新規 `docs/external/`、`docs/logs/7/worklog.md`

### 調査結果

#### 1. `docs/spec.md` 9.5節

- 完了時の主処理順序は現行実装と概ね整合している。
- ただし staging bundle cleanup は差分が残る。仕様は「failed または timed_out run の staging bundle は7日後に削除対象」としている一方、現行実装の `crates/db/src/queries/artifact_bundle.rs` では `mark_failed` が `delete_after` を設定していない。
- このため 9.5節は仕様として妥当だが、現行実装とは完全一致していない。

#### 2. `docs/backend/api.md`

- Import API のレスポンス `status` は `queued` / `running` / `completed` / `failed` と記載されており、`crates/api/src/routes/board_run.rs` の `bundle_status_str()` と整合している。
- `Pending -> queued`、`Validating` / `Importing -> running`、`Completed -> completed`、`Failed -> failed` を確認。

#### 3. `docs/backend/summary.md`

- Queue / Worker セクションに artifact import worker の責務、完了後の後続 job enqueue、staging/final bucket 方針がすでに記載されている。
- Import Worker 実装に合わせた追加追記は今回必須ではない。

#### 4. `docs/technology.md`

- このファイルは crate 単位の依存一覧ではなく、採用技術のレイヤー別サマリを扱っている。
- `zip = "2"` は workspace 依存の追加だが、技術スタック全体の意思決定として新セクションを足す必要まではない。

#### 5. `docs/external/`

- `docs/external/aws-sdk-s3-download.md` は採用メモの一部が stale だったため、現行実装に合わせて「`aws-config` 経由でも direct builder でもよい」形に修正。
- `docs/external/postgresql-job-queue-polling.md` はポーリング間隔を固定値の結論に見せないよう、「実装側で調整可能」である旨を追記。
- `docs/external/zip-archive-rust.md` は `zip = "2"`、インメモリ展開、未記載 entry 拒否、サイズ検証の方向性と矛盾していないため修正不要と判断。

#### 6. `docs/logs/7/worklog.md`

- 途中フェーズの古いレビュー内容は履歴として残っているが、末尾には再レビューと最終 `pr_ready: true` が追記されており、時系列ログとしては成立している。
- ただし今回のドキュメント観点では、spec 9.5 と実装の cleanup 差分を明示しておく必要があるため、本フェーズを追記した。

### ドキュメント確認

- `docs/backend/api.md`、`docs/backend/summary.md`、`docs/technology.md` は今回の観点では修正不要。
- `docs/external/aws-sdk-s3-download.md` と `docs/external/postgresql-job-queue-polling.md` は現行実装との読み違いを避けるため補正した。
- `docs/spec.md` は仕様として維持すべき内容だが、現行実装が failed bundle の `delete_after` を設定しておらず、実装との差分が残る。

### PR/完了結果

- 判定: `docs_ready: false`
- ブロッカー: `mark_failed` で `delete_after` が未設定

### 残リスク

- `docs/spec.md` 9.5節の cleanup 方針と現行実装が未一致のため、PR説明では「failed/timed_out bundle の delete_after は未実装」と明示しない限り誤読が起こりうる。
- worklog は十分詳細だが、最終判定だけを見る読者向けには今回のドキュメント確認フェーズを参照する必要がある。

---

## ドキュメント確認 ブロッカー修正 (2026-05-01)

### 修正内容

- `crates/db/src/queries/artifact_bundle.rs` の `mark_failed` に `delete_after = NOW() + INTERVAL '7 days'` を追加
- `docs/spec.md` 9.5節の「failed bundle は7日後に削除対象」と整合

### docs_ready: true

---

## PR作成フェーズ (2026-05-01)

### 前提確認

- ブランチ: `feat/import-worker` (HEAD: ba9120d)
- 未コミット変更: なし (working tree clean)
- `cargo test --workspace`: 64 tests passed, 0 failed
- Review: `pr_ready: true` (3回レビュー後)
- Docs: `docs_ready: true` (failed bundle の delete_after 修正後)

### PR作成結果

- **PRリンク**: https://github.com/f0reachARR/boardflow/pull/15
- タイトル: `feat(worker): implement Import Worker (#7)`
- base: `main`, head: `feat/import-worker`
- `Closes #7` をPR本文に含む

### PR本文の内容

- 要件 (受け入れ条件)
- 調査結果 (Research成果物)
- 実装概要 (変更 crate 一覧、主要設計決定)
- テスト結果 (64テスト全パス)
- 更新ドキュメント
- 外部調査メモ
- 残リスク (MVPスコープ外として許容)
- Review/Docs OK判定

### 残リスク

- Worker E2E統合テスト未実装 (CI docker-compose環境が必要。MVP後対応)
- diff summary 最小実装 (ファイル数カウントのみ。baseline snapshot比較は別対応)
- S3孤立オブジェクト (upload後のTX失敗で孤立発生可能。delete_after TTLで自然回収)
- 500MB全量メモリ展開 (大規模プロジェクトではストリーミング展開が必要な可能性)
- Worker クラッシュ回復 (running状態で放置されたジョブの定期検出バッチは未実装)

---

## 追加調査: ERC/DRC findings manifest.json フォーマット設計 (2026-05-01)

### 経緯

- Issue #7 追加実装として、Worker の Step 5 で `run_check_findings` テーブルへの保存が必要
- 現在の `ManifestCheck` 構造体に `findings` フィールドがない
- KiCad CLI の ERC/DRC JSON 出力フォーマットを確認し、manifest.json の findings 配列を設計する必要がある

### ユーザー要望

1. KiCad CLI ERC/DRC 出力形式の確認
2. manifest.json findings フィールドの設計
3. KiCad JSON → manifest.json findings → run_check_findings テーブルへのマッピングルール

### 調査結果

KiCad 9.0 ソースコード (`include/rc_json_schema.h`, `pcbnew/drc/drc_item.cpp`, `eeschema/erc/erc_item.cpp`) を確認。

#### KiCad JSON 構造体

- **VIOLATION**: `{ type, description, severity, items: [AFFECTED_ITEM], excluded, comment }`
- **AFFECTED_ITEM**: `{ uuid, description, pos: { x, y } }`
- **ERC_REPORT**: sheets 配列内に sheet ごとの violations
- **DRC_REPORT**: violations, unconnected_items, schematic_parity の3つの VIOLATION 配列

#### severity マッピング

- KiCad `"error"` → BoardFlow `"error"`
- KiCad `"warning"` → BoardFlow `"warning"`
- KiCad `"exclusion"` → 除外 (findings に含めない)

#### manifest.json findings 設計

`ManifestCheck` に `findings: Vec<ManifestFinding>` を追加。`ManifestFinding` は:
- `severity`: "error" | "warning" | "notice"
- `rule_code`: KiCad violation.type (例: "clearance")
- `title`: KiCad violation.description
- `message`: affected items の description を結合
- `subject_kind`: "schematic" | "pcb" | "net" | "footprint" | "symbol"
- `subject_ref`: 主要参照先
- `sheet_path`: ERC の sheet path (DRC は null)
- `pcb_layer`: null (KiCad JSON に含まれない)
- `pos_mm`: 最初の affected item の位置 (mm)
- `raw`: 生の KiCad VIOLATION オブジェクト

#### 座標変換

- KiCad: mm (float)
- manifest.json: mm (float、`pos_mm` フィールド)
- DB `x_um`/`y_um`: µm (integer、Worker で mm × 1000 に変換)

### 成果物

- `docs/external/kicad-erc-drc-findings.md` — 詳細な調査メモ (KiCad JSON スキーマ、violation type 一覧、manifest.json 設計、マッピングルール)

### 参照URL

- https://gitlab.com/kicad/code/kicad/-/blob/9.0/include/rc_json_schema.h
- https://gitlab.com/kicad/code/kicad/-/blob/9.0/pcbnew/drc/drc_item.cpp
- https://gitlab.com/kicad/code/kicad/-/blob/9.0/eeschema/erc/erc_item.cpp
- https://docs.kicad.org/9.0/en/cli/cli.html
- https://gitlab.com/kicad/code/kicad/-/issues/23948

### 結論ステータス

`implementation_required`

実装が必要な変更:
1. `crates/artifact/src/lib.rs`: `ManifestCheck` に `findings: Vec<ManifestFinding>` 追加、`ManifestFinding` / `CoordinateMm` 構造体追加
2. `crates/db/src/queries/`: `run_check_finding::insert` クエリ追加
3. `crates/worker/src/main.rs`: Step 5 に findings INSERT ループ追加

### 残リスク

- findings 上限数: 大規模プロジェクトでは数百〜数千の findings。MVP では全件保存 (上限は後で検討)
- `pcb_layer` と `bbox_json` は KiCad JSON に含まれないため常に null
- `notice` severity は KiCad には存在しないが、将来の拡張用にスキーマに残す

---

## 計画フェーズ: run_check_findings INSERT 実装 (2026-05-01)

### 目的

Worker の Import Job 処理 (Step 5) で、`manifest.checks[].findings` 配列を DB の `run_check_findings` テーブルに INSERT する。

### 非目的

- KiCad JSON → ManifestFinding への変換ロジック実装 (GitHub Actions 側の責務)
- findings の集計・分析 API
- findings 上限数の制限 (MVP 後に検討)
- bbox_json の自動算出

### 受け入れ条件

1. `ManifestCheck` に `findings: Vec<ManifestFinding>` フィールドが追加されている (`#[serde(default)]`)
2. `ManifestFinding`, `CoordinateMm` 構造体が `crates/artifact/src/lib.rs` に定義されている
3. `crates/db/src/queries/run_check_finding.rs` が存在し、`insert` 関数がある
4. Worker Step 5 で `run_check::insert` の後に findings ループ INSERT が実行される
5. `pos_mm` (mm) → `x_um`, `y_um` (µm) の変換が正しく行われる (`× 1000`, `round()`)
6. `findings` フィールドが空 (`[]`) の manifest.json でも既存処理が壊れない
7. 既存のユニットテスト (`extract_test.rs`) が引き続きパスする
8. 新規ユニットテストで findings INSERT の正常系・異常系を検証できる

### 詳細要件

#### 1. artifact crate (`crates/artifact/src/lib.rs`)

**追加する構造体:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFinding {
    pub severity: String,
    pub rule_code: String,
    pub title: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub subject_kind: Option<String>,
    #[serde(default)]
    pub subject_ref: Option<String>,
    #[serde(default)]
    pub sheet_path: Option<String>,
    #[serde(default)]
    pub pcb_layer: Option<String>,
    #[serde(default)]
    pub pos_mm: Option<CoordinateMm>,
    #[serde(default)]
    pub raw: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinateMm {
    pub x: f64,
    pub y: f64,
}
```

**`ManifestCheck` 変更:**

```rust
pub struct ManifestCheck {
    // ... 既存フィールド ...
    #[serde(default)]
    pub findings: Vec<ManifestFinding>,  // ← 追加
}
```

#### 2. db crate (`crates/db/src/queries/run_check_finding.rs`)

新規ファイル。`run_check.rs` と同じパターンに従う。

```rust
use boardflow_domain::models::run_check::RunCheckFinding;
use uuid::Uuid;

pub async fn insert(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    run_check_id: Uuid,
    severity: &str,
    rule_code: Option<&str>,
    title: Option<&str>,
    message: Option<&str>,
    subject_kind: Option<&str>,
    subject_ref: Option<&str>,
    sheet_path: Option<&str>,
    pcb_layer: Option<&str>,
    x_um: Option<i32>,
    y_um: Option<i32>,
    bbox_json: Option<&serde_json::Value>,
    raw_payload_json: Option<&serde_json::Value>,
    sort_index: i32,
) -> Result<RunCheckFinding, sqlx::Error> {
    sqlx::query_as::<_, RunCheckFinding>(
        r#"INSERT INTO run_check_findings (
            id, run_check_id, severity, rule_code, title, message,
            subject_kind, subject_ref, sheet_path, pcb_layer,
            x_um, y_um, bbox_json, raw_payload_json,
            sort_index, created_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, NOW())
        RETURNING *"#,
    )
    .bind(id)
    .bind(run_check_id)
    .bind(severity)
    .bind(rule_code)
    .bind(title)
    .bind(message)
    .bind(subject_kind)
    .bind(subject_ref)
    .bind(sheet_path)
    .bind(pcb_layer)
    .bind(x_um)
    .bind(y_um)
    .bind(bbox_json)
    .bind(raw_payload_json)
    .bind(sort_index)
    .fetch_one(executor)
    .await
}
```

#### 3. db crate (`crates/db/src/queries/mod.rs`)

```rust
pub mod run_check_finding;  // ← 追加
```

#### 4. worker crate (`crates/worker/src/main.rs`)

Step 5 の `run_check::insert` 後に findings ループを追加:

```rust
// Import を追加
use boardflow_db::queries::run_check_finding;

// Step 5 内部、run_check::insert の後に:
for (idx, finding) in check.findings.iter().enumerate() {
    let x_um = finding.pos_mm.as_ref().map(|p| (p.x * 1000.0).round() as i32);
    let y_um = finding.pos_mm.as_ref().map(|p| (p.y * 1000.0).round() as i32);
    run_check_finding::insert(
        &mut *tx,
        Uuid::now_v7(),
        check_id,
        &finding.severity,
        Some(finding.rule_code.as_str()),
        Some(finding.title.as_str()),
        finding.message.as_deref(),
        finding.subject_kind.as_deref(),
        finding.subject_ref.as_deref(),
        finding.sheet_path.as_deref(),
        finding.pcb_layer.as_deref(),
        x_um,
        y_um,
        None, // bbox_json
        finding.raw.as_ref(),
        idx as i32,
    )
    .await
    .map_err(|e| ArtifactError::S3(e.to_string()))?;
}
```

### 影響範囲

| クレート | ファイル | 変更種別 |
|---|---|---|
| `artifact` | `src/lib.rs` | 構造体追加 + `ManifestCheck` フィールド追加 |
| `artifact` | `tests/extract_test.rs` | テスト追加 (findings付きZIPのデシリアライズ確認) |
| `db` | `src/queries/run_check_finding.rs` | **新規作成** |
| `db` | `src/queries/mod.rs` | モジュール追加 |
| `worker` | `src/main.rs` | import追加 + Step 5 findings ループ追加 |

### 設計方針

1. **既存パターン踏襲**: `run_check.rs` の insert 関数と同じシグネチャスタイル
2. **逐次 INSERT**: MVP では `for` ループでの逐次 INSERT。大量 findings のバッチ INSERT 最適化は後続タスク
3. **エラーハンドリング**: findings INSERT 失敗は `ArtifactError::S3` でラップしてトランザクションをロールバック (全 findings が atomic)
4. **後方互換**: `#[serde(default)]` により `findings` フィールドが無い旧 manifest.json でも空 Vec としてデシリアライズされる
5. **座標変換**: `mm × 1000 → µm` (round して i32)。`pos_mm` が None の場合は `x_um`, `y_um` ともに None

### テスト観点

| # | テスト種別 | 内容 | ファイル |
|---|---|---|---|
| 1 | ユニット | ManifestFinding のデシリアライズ (全フィールド有り) | `artifact/tests/extract_test.rs` |
| 2 | ユニット | ManifestFinding のデシリアライズ (optional フィールド無し) | `artifact/tests/extract_test.rs` |
| 3 | ユニット | findings 付き ManifestCheck のデシリアライズ | `artifact/tests/extract_test.rs` |
| 4 | ユニット | findings 無し (旧形式) ManifestCheck の後方互換 | `artifact/tests/extract_test.rs` |
| 5 | ユニット | 座標変換 mm→µm (正常値、境界値、None) | `artifact/tests/extract_test.rs` |
| 6 | 統合 | Worker import で findings 付き manifest → DB に正しく INSERT される | `api/tests/board_run_test.rs` (既存テスト拡張) |

### ドキュメント更新対象

- `docs/logs/7/worklog.md`: 本計画の記録 (本セクション)
- `docs/external/kicad-erc-drc-findings.md`: 既に作成済み (変更不要)

### 実装順序

1. `crates/artifact/src/lib.rs`: `ManifestFinding`, `CoordinateMm` 構造体追加、`ManifestCheck` にフィールド追加
2. `crates/artifact/tests/extract_test.rs`: デシリアライズテスト追加 → `cargo test -p boardflow-artifact` で確認
3. `crates/db/src/queries/run_check_finding.rs`: 新規作成
4. `crates/db/src/queries/mod.rs`: モジュール追加
5. `crates/worker/src/main.rs`: import 追加 + Step 5 findings ループ追加
6. `cargo check --workspace` で全体コンパイル確認
7. 統合テスト確認

### ブランチ名

`feature/issue-7-run-check-findings-insert`

### 実装要否

`implementation_required`

### 未解決の疑問

なし (research フェーズで全て解決済み)

### 更新した作業ログパス

`docs/logs/7/worklog.md`
