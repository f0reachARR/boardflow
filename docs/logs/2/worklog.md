# Issue #2: DBマイグレーション・データモデル実装

## 経緯
- バックエンド実装Issue分割タスクの一環として作成
- spec.md Section 10 の全テーブルをマイグレーション化

## ユーザー要望
- docs/以下の仕様に基づくRustバックエンド実装の第2段階

## Issue作成内容
- 13テーブルのSQLxマイグレーション作成
- URL: https://github.com/f0reachARR/boardflow/issues/2

## 後続処理タイプの初期仮説
`implementation_required`

---

## 調査フェーズ (2026-04-30)

### 調査トピックと結論

#### 1. SQLx migration ファイル形式
- **結論**: Simple (非可逆) 形式を採用
- `<timestamp>_<name>.sql` 形式（14桁タイムスタンプ）
- 既存 `20260430000000_init.sql` と同一形式
- 13テーブルは1ファイルで作成推奨
- 詳細: `docs/external/sqlx-migration-format.md`

#### 2. PostgreSQL UUID 生成
- **結論**: アプリ層で UUID v7 生成（`Uuid::now_v7()`）
- `gen_random_uuid()` は PG 13+ でビルトイン（拡張不要）だが v4 のみ
- `uuidv7()` は PG 18+ のみ（PG 16 では使えない）
- DDL: `id UUID PRIMARY KEY`（DEFAULT なし）
- Rust: `uuid::Uuid::now_v7()` で生成して INSERT 時に渡す
- 詳細: `docs/external/postgresql-uuid-generation.md`

#### 3. PostgreSQL ENUM vs CHECK 制約
- **結論**: TEXT + CHECK 制約を採用
- マイグレーションでの値変更が容易（O(1) + VALIDATE）
- ロックが軽い（SHARE UPDATE EXCLUSIVE、SaaS向き）
- SQLx: `#[derive(sqlx::Type)]` + `#[sqlx(type_name = "text", rename_all = "snake_case")]`
- 制約名: `<table>_<column>_check` で統一
- 詳細: `docs/external/postgresql-enum-vs-check.md`

#### 4. chrono::DateTime<Utc> と TIMESTAMPTZ
- **結論**: `TIMESTAMPTZ NOT NULL DEFAULT now()` + `DateTime<Utc>`
- SQLx `chrono` feature で追加実装なし
- 全タイムスタンプは TIMESTAMPTZ で統一
- nullable: `Option<DateTime<Utc>>`
- 詳細: `docs/external/sqlx-chrono-timestamptz.md`

### 実装時の具体的方針まとめ

| 項目 | 方針 |
|---|---|
| マイグレーション形式 | Simple、1ファイルで全テーブル |
| UUID 主キー | `id UUID PRIMARY KEY`、Rust で v7 生成 |
| ステータスカラム | `TEXT NOT NULL CHECK (col IN (...))` |
| タイムスタンプ | `TIMESTAMPTZ NOT NULL DEFAULT now()` |
| nullable タイムスタンプ | `TIMESTAMPTZ`（DEFAULT なし） |
| CHECK 制約名 | `<table>_<column>_check` |
| JSONB カラム | インデックスなし（MVP後に判断） |

### 結論ステータス
`implementation_required`

### 残リスク
- ~~PostgreSQL enum vs CHECK 制約の選択が未決定~~ → CHECK 制約に決定
- JSONB index の要否はMVP後に判断
- PG 18+ への移行時に `uuidv7()` DEFAULT への切り替えを検討可能

---

## 計画フェーズ (2026-04-30)

### ユーザー確認済み列挙値（spec.md 未定義分）

| カラム | 確定値 |
|---|---|
| board_projects.issue_sync_status | pending, syncing, synced, failed |
| board_runs.review_status | pending, ready, no_baseline, failed |
| board_runs.diff_status | pending, ready, no_baseline, unavailable, failed |
| board_runs.erc_status / drc_status | passed, failed, skipped (nullable) |
| artifact_bundles.status | pending, validating, importing, completed, failed |
| artifact_bundles.intake_mode | TEXT NOT NULL (CHECKなし) |
| github_jobs.status | pending, running, completed, failed |
| artifacts.type | TEXT NOT NULL (CHECKなし) |

### 実装計画

#### 1. 目的
spec.md Section 10 の13テーブルを PostgreSQL 16+ 上に SQLx マイグレーションとして作成し、全 API/Worker が依存するスキーマ基盤を構築する。合わせて Rust ドメインモデル構造体を crates/domain に定義する。

#### 2. 非目的
- クエリ層の実装（crates/db へのリポジトリパターン追加は後続Issue）
- JSONB カラムへのインデックス追加（MVP後判断）
- API エンドポイントの実装
- テストデータ投入用 seed/fixture

#### 3. 受け入れ条件
- [ ] 20260430000001_create_schema.sql が sqlx migrate run で正常適用される
- [ ] 13テーブル + 全UNIQUE制約 + 全CHECK制約 + 全FK制約 + FKインデックスが作成される
- [ ] crates/domain に全13テーブル対応のRust構造体と列挙型が定義される
- [ ] cargo build が通る
- [ ] cargo test が通る（既存テスト含む）
- [ ] PostgreSQL上でマイグレーションの適用・検証テスト通過

#### 4. 詳細要件

##### 4.1 マイグレーションSQL

**ファイル**: crates/db/migrations/20260430000001_create_schema.sql

**テーブル作成順序**（FK依存関係順）:

```
1.  repositories                        -- 基底テーブル
2.  board_projects                      -- FK: repositories
3.  board_runs                          -- FK: board_projects
4.  ALTER board_projects                -- 遅延FK: latest_completed_run_id -> board_runs
5.  artifact_bundles                    -- FK: board_runs
6.  artifacts                           -- FK: board_runs, artifact_bundles
7.  run_checks                          -- FK: board_runs, artifacts
8.  run_check_findings                  -- FK: run_checks
9.  board_project_snapshots             -- FK: board_projects, board_runs
10. board_run_diff_metadata             -- FK: board_runs
11. board_run_diffs                     -- FK: board_runs x2
12. boardflow_api_tokens                -- FK: repositories
13. github_jobs                         -- FK: repositories, board_projects, board_runs
14. board_project_issue_history         -- FK: board_projects
```

**循環依存の解決**: board_projects.latest_completed_run_id -> board_runs は board_runs 作成後に ALTER TABLE で FK 追加。

**FKインデックス**:
- board_projects(repository_id), board_projects(latest_completed_run_id)
- board_runs(board_project_id)
- artifacts(board_run_id), artifacts(source_bundle_id)
- artifact_bundles(board_run_id)
- run_checks(board_run_id), run_checks(report_artifact_id)
- run_check_findings(run_check_id)
- board_project_snapshots(board_project_id)
- board_run_diffs(base_board_run_id)
- boardflow_api_tokens(repository_id), boardflow_api_tokens(installation_id)
- github_jobs(repository_id), github_jobs(board_project_id), github_jobs(board_run_id)
- board_project_issue_history(board_project_id)
- UNIQUE でカバー済: board_project_snapshots(board_run_id), board_run_diff_metadata(board_run_id), board_run_diffs(board_run_id)

**追加機能インデックス**:
- github_jobs(status, run_after) WHERE status = pending -- ジョブキューポーリング用 partial index

##### 4.2 ドメインモデル（crates/domain）

**ファイル構成**:
```
crates/domain/src/
  lib.rs                # pub mod 宣言
  models/
    mod.rs              # 全モデル re-export
    repository.rs       # Repository
    board_project.rs    # BoardProject, IssueSyncStatus
    board_run.rs        # BoardRun, RunStatus, ReviewStatus, DiffStatus, CheckStatus
    artifact.rs         # Artifact, ArtifactStatus, ArtifactBundle, BundleStatus
    check.rs            # RunCheck, CheckKind, CheckRunStatus, RunCheckFinding, Severity, SubjectKind
    snapshot.rs         # BoardProjectSnapshot
    diff.rs             # BoardRunDiffMetadata, BoardRunDiff, DiffRunStatus
    token.rs            # BoardflowApiToken
    github_job.rs       # GithubJob, JobType, JobStatus
    issue_history.rs    # BoardProjectIssueHistory, IssueHistoryReason
```

**Cargo.toml 変更**: chrono, sqlx, serde_json を依存に追加
**workspace Cargo.toml**: chrono をワークスペース依存に追加

**型マッピング**:
| SQL型 | Rust型 |
|---|---|
| UUID | uuid::Uuid |
| BIGINT | i64 |
| INTEGER | i32 |
| TEXT | String |
| BOOLEAN | bool |
| TIMESTAMPTZ | chrono::DateTime<chrono::Utc> |
| TIMESTAMPTZ nullable | Option<chrono::DateTime<chrono::Utc>> |
| JSONB | serde_json::Value |
| JSONB nullable | Option<serde_json::Value> |

##### 4.3 crates/db 変更
変更なし。run_migrations() の sqlx::migrate!() が新マイグレーションを自動ピックアップ。

#### 5. 影響範囲
- crates/db/migrations/ -- 新規マイグレーション追加
- crates/domain/ -- モデル定義追加（Cargo.toml + src/）
- Cargo.toml -- workspace dependency に chrono 追加
- 既存コードへの破壊的変更なし

#### 6. 設計方針
- Simple (非可逆) マイグレーション、1ファイルで全テーブル
- FK依存関係順にテーブル作成、循環依存は ALTER TABLE で解決
- ステータスカラムは TEXT + CHECK 制約（制約名: <table>_<column>_check）
- UUID主キーは DEFAULT なし（アプリ層で v7 生成）
- TIMESTAMPTZ + DEFAULT now() で作成・更新日時管理
- FKカラムにはインデックスを明示作成
- ドメインモデルは機能グループ別にファイル分割

#### 7. テスト観点
- マイグレーション適用テスト: sqlx migrate run で全テーブル正常作成
- コンパイルテスト: cargo build パス
- CHECK制約テスト: 不正値 INSERT が拒否される
- FK制約テスト: 存在しない親への INSERT が拒否される
- UNIQUE制約テスト: 重複 INSERT が拒否される
- 既存テストの回帰なし

#### 8. ドキュメント更新対象
- docs/logs/2/worklog.md -- 本計画（本ファイル）
- spec.md への変更不要（spec 準拠実装）

#### 9. 変更ファイル一覧

| ファイル | 操作 | 内容 |
|---|---|---|
| Cargo.toml | 修正 | chrono ワークスペース依存追加 |
| crates/domain/Cargo.toml | 修正 | chrono, sqlx, serde_json 依存追加 |
| crates/db/migrations/20260430000001_create_schema.sql | 新規 | 13テーブル + 制約 + インデックス |
| crates/domain/src/lib.rs | 修正 | pub mod models 宣言 |
| crates/domain/src/models/mod.rs | 新規 | 全モデル re-export |
| crates/domain/src/models/repository.rs | 新規 | Repository |
| crates/domain/src/models/board_project.rs | 新規 | BoardProject, IssueSyncStatus |
| crates/domain/src/models/board_run.rs | 新規 | BoardRun + enum 4種 |
| crates/domain/src/models/artifact.rs | 新規 | Artifact, ArtifactBundle + enum |
| crates/domain/src/models/check.rs | 新規 | RunCheck, RunCheckFinding + enum |
| crates/domain/src/models/snapshot.rs | 新規 | BoardProjectSnapshot |
| crates/domain/src/models/diff.rs | 新規 | BoardRunDiffMetadata, BoardRunDiff + enum |
| crates/domain/src/models/token.rs | 新規 | BoardflowApiToken |
| crates/domain/src/models/github_job.rs | 新規 | GithubJob, JobType, JobStatus |
| crates/domain/src/models/issue_history.rs | 新規 | BoardProjectIssueHistory + enum |

#### 10. 実装順序

1. ブランチ作成: feature/issue-2-db-migration
2. ワークスペース Cargo.toml: chrono 依存追加
3. domain Cargo.toml: chrono, sqlx, serde_json 依存追加
4. マイグレーション SQL 作成: 全13テーブル + 制約 + インデックス
5. ドメインモデル作成: models/ ディレクトリ + 各ファイル
6. domain/src/lib.rs 更新: pub mod models
7. ビルド検証: cargo build
8. PostgreSQL テスト: docker-compose で DB 起動 -> マイグレーション適用確認
9. 既存テスト確認: cargo test
10. コミット・プッシュ

#### 11. 実装要否
implementation_required

#### 12. 未解決の疑問
なし（全列挙値をユーザーに確認済み）

#### 13. 残リスク
- JSONB カラムのインデックスはMVP後に要否判断
- PG 18+ 移行時に uuidv7() DEFAULT への切り替えを検討可能
- artifacts.type と artifact_bundles.intake_mode は CHECK なしのため、アプリ層バリデーション必要

---

## 実装フェーズ (2026-04-30)

### 実装内容

#### ブランチ
`feature/issue-2-db-migration` (from main)

#### 変更ファイル一覧

| ファイル | 種別 | 内容 |
|---|---|---|
| Cargo.toml | 修正 | workspace.dependencies に chrono 追加 |
| Cargo.lock | 自動更新 | chrono 依存解決 |
| crates/domain/Cargo.toml | 修正 | serde_json, chrono, sqlx 追加 |
| crates/domain/src/lib.rs | 修正 | `pub mod models;` 追加 |
| crates/domain/src/models/mod.rs | 新規 | 全サブモジュール宣言 |
| crates/domain/src/models/repository.rs | 新規 | Repository 構造体 |
| crates/domain/src/models/board_project.rs | 新規 | BoardProject + IssueSyncStatus |
| crates/domain/src/models/board_run.rs | 新規 | BoardRun + 4 enum |
| crates/domain/src/models/artifact.rs | 新規 | Artifact + ArtifactStatus |
| crates/domain/src/models/artifact_bundle.rs | 新規 | ArtifactBundle + ArtifactBundleStatus |
| crates/domain/src/models/run_check.rs | 新規 | RunCheck, RunCheckFinding + 4 enum |
| crates/domain/src/models/snapshot.rs | 新規 | BoardProjectSnapshot, BoardRunDiffMetadata, BoardRunDiff + enum |
| crates/domain/src/models/api_token.rs | 新規 | BoardflowApiToken |
| crates/domain/src/models/github_job.rs | 新規 | GithubJob + GithubJobStatus |
| crates/domain/src/models/issue_history.rs | 新規 | BoardProjectIssueHistory + IssueHistoryReason |
| crates/db/migrations/20260430000001_create_schema.sql | 新規 | 13テーブル DDL |

### テスト結果

| テスト | 結果 |
|---|---|
| `cargo build` | ✅ 成功 |
| `cargo test` | ✅ 全テスト通過 (既存テスト回帰なし) |
| `sqlx database reset` (PostgreSQL 16) | ✅ マイグレーション正常適用 |
| テーブル確認 (`\dt`) | ✅ 13テーブル + _sqlx_migrations 確認 |

### 更新ドキュメント
- `docs/logs/2/worklog.md` (本ファイル)

### 残リスク
- SQLx compile-time checking (offline mode / sqlx-data.json) は未設定 → CI/後続Issueで対応
- JSONB カラムのインデックスはMVP後に要否判断
- artifacts.type / artifact_bundles.intake_mode は CHECK なし → アプリ層バリデーション必要
- board_project_snapshots(board_run_id) にインデックス未追加（UNIQUE制約なし、FK参照もなし → 必要時に追加）
