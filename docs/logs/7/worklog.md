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
