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
- **結論**: Simple (非可逆) 形式を採用 **[SUPERSEDED: reversible (.up.sql / .down.sql) 採用に変更]**
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
- **結論**: `TIMESTAMPTZ NOT NULL DEFAULT now()` + `DateTime<Utc>` **[SUPERSEDED: アプリ入力前提（DEFAULT now() なし）に変更]**
- SQLx `chrono` feature で追加実装なし
- 全タイムスタンプは TIMESTAMPTZ で統一
- nullable: `Option<DateTime<Utc>>`
- 詳細: `docs/external/sqlx-chrono-timestamptz.md`

### 実装時の具体的方針まとめ

| 項目 | 方針 |
|---|---|
| マイグレーション形式 | Simple、1ファイルで全テーブル **[SUPERSEDED: reversible 採用]** |
| UUID 主キー | `id UUID PRIMARY KEY`、Rust で v7 生成 |
| ステータスカラム | `TEXT NOT NULL CHECK (col IN (...))` |
| タイムスタンプ | `TIMESTAMPTZ NOT NULL DEFAULT now()` **[SUPERSEDED: アプリ入力前提、DEFAULT なし]** |
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

---

## レビューフェーズ (2026-04-30)

### 対象Issue
- Issue #2: DBマイグレーション・データモデル実装

### 調査結果
- spec.md Section 10、README、backend設計文書、docs/external の調査メモ、既存 worklog、実装済み SQL / domain model を照合した。
- 外部調査では、SQLx の migrate revert は reversible migration（.up.sql / .down.sql）前提であり、simple migration では満たせないことを再確認した。
- PostgreSQL 15+ では UNIQUE ... NULLS NOT DISTINCT が使えるため、nullable 列を含む重複防止は単純な複合 UNIQUE より明示設計が必要と確認した。

### テスト結果
- cargo build : 成功
- cargo test : 成功
- PostgreSQL 16 コンテナ上で 20260430000000_init.sql + 20260430000001_create_schema.sql を適用し、13テーブル作成を確認

### ドキュメント確認
- spec.md Section 10 の13テーブル定義との大枠整合は確認
- README は依然として高レベル概要のみで、本Issue向けの追加更新は不要
- docs/external/sqlx-migration-format.md の方針と、Issue本文の成功条件 sqlx migrate revert の間に不整合が残っている

### レビュー結果
- 判定: pr_ready: false

#### 必須修正
1. artifacts に、spec.md が要求する board_run_id + type + source_path 相当の同一run内重複防止制約を追加すること。
   - source_path が nullable のため、PostgreSQL 15+ の NULLS NOT DISTINCT を使うか、source_path IS NULL / IS NOT NULL を分けた一意インデックス等で要件を満たす必要がある。
2. Issue本文の成功条件にある sqlx migrate revert を満たす migration 形式へ修正するか、少なくとも Issue / 計画 / 成功条件のいずれかを更新して forward-only 方針へ明示的に再合意すること。
   - 現状は simple migration しか存在せず、受け入れ条件との差分が未解消。

#### 任意改善
1. boardflow_api_tokens(installation_id) インデックスは計画にあるが未実装のため、lookup 予定があるなら追加を検討する。
2. board_project_snapshots(board_run_id) の一意性または索引方針は、1 run 1 snapshot を前提にするなら明示した方がよい。
3. created_at / updated_at や一部 JSONB カラムの DEFAULT 方針は、Issue本文・research・実装で揺れがあるため後続Issue向けに明文化した方がよい。

#### plan / research / docs との不整合
- research / plan では simple migration 採用としている一方、Issue本文の成功条件には sqlx migrate revert が含まれている。
- 計画では boardflow_api_tokens(installation_id) と github_jobs(status, run_after) WHERE status = pending を明示しているが、前者は未実装、後者は run_after 単独 partial index 実装に変わっている。
- spec.md は artifacts の重複防止を要求しているが、実装 SQL に対応制約がない。

#### テスト不足
- artifacts の重複防止制約に関する失敗系テストがない。
- reversible migration を前提にするなら sqlx migrate revert の実測がない。
- CHECK / UNIQUE / FK の失敗系を検証する DB テストが未追加。

#### 残リスク
- 現状の artifacts は重複行を受け入れるため、import 冪等性や Web UI 集計の前提を崩す可能性がある。
- migration 形式の合意が曖昧なまま進むと、後続Issueで sqlx migrate revert を前提にした開発フローが成立しない。

---

## レビュー修正実装フェーズ (2026-04-30)

### 実施した修正

#### 修正1: artifacts の重複防止制約
- `CREATE UNIQUE INDEX idx_artifacts_run_type_path ON artifacts(board_run_id, type, source_path) NULLS NOT DISTINCT;` を up.sql に追加
- PostgreSQL 16 で NULLS NOT DISTINCT が使用可能であることを実機確認済み
- 同一 board_run_id + type + source_path（NULL含む）の重複が制約で防止される

#### 修正2: Reversible migration への変換
- `20260430000000_init.sql` → `20260430000000_init.up.sql` + `20260430000000_init.down.sql`
- `20260430000001_create_schema.sql` → `20260430000001_create_schema.up.sql` + `20260430000001_create_schema.down.sql`
- down.sql: 循環FK制約の DROP → テーブルの逆依存順 DROP
- `sqlx migrate run` + `sqlx migrate revert` の往復テスト通過

#### 修正3: boardflow_api_tokens(installation_id) インデックス追加
- `CREATE INDEX idx_boardflow_api_tokens_installation_id ON boardflow_api_tokens(installation_id);` を up.sql に追加

### 検証結果

| テスト | 結果 |
|---|---|
| `cargo build` | ✅ 成功 |
| `cargo test` | ✅ 全テスト通過 |
| `sqlx migrate run` | ✅ Applied 20260430000000/migrate init + 20260430000001/migrate create schema |
| テーブル確認 (13テーブル) | ✅ 全テーブル作成確認 |
| `idx_artifacts_run_type_path` 存在確認 | ✅ |
| `idx_boardflow_api_tokens_installation_id` 存在確認 | ✅ |
| `sqlx migrate revert` (create_schema) | ✅ 全テーブル DROP 確認 |
| `sqlx migrate revert` (init) | ✅ |
| 再度 `sqlx migrate run` | ✅ 再適用成功 |

### 変更ファイル

| ファイル | 操作 |
|---|---|
| `crates/db/migrations/20260430000000_init.sql` | 削除 |
| `crates/db/migrations/20260430000000_init.up.sql` | 新規（元の内容） |
| `crates/db/migrations/20260430000000_init.down.sql` | 新規（SELECT 1） |
| `crates/db/migrations/20260430000001_create_schema.sql` | 削除 |
| `crates/db/migrations/20260430000001_create_schema.up.sql` | 新規（元の内容 + 修正1,3 のインデックス追加） |
| `crates/db/migrations/20260430000001_create_schema.down.sql` | 新規（循環FK DROP + テーブル逆順 DROP） |

### 残リスク
- CHECK / UNIQUE / FK の失敗系 DB テストは未追加（後続Issueでの統合テスト実装時に対応）
- SQLx compile-time checking (offline mode) は未設定

### PR/完了結果
- pr_ready: false
- 再レビュー条件: artifacts 重複防止制約の追加、および migration 受け入れ条件の不整合解消

---

## 再レビューフェーズ (2026-04-30)

### 対象Issue
- Issue #2: DBマイグレーション・データモデル実装

### 調査結果
- spec.md Section 10、対象 migration 4ファイル、domain model 群、README、docs/external、既存 worklog を再照合した。
- cargo test はローカルで再実行し成功を確認した。
- PostgreSQL 16 上で sqlx migrate run → revert → run を再実行し、reversible migration と 13 テーブル再作成を確認した。

### レビュー結果
- 判定: pr_ready: false

#### 指摘
1. board_projects.latest_completed_run_id の FK が board_runs(id) の存在だけを保証しており、同じ board_project に属する run であることを DB が保証していない。
   - 現状は board_projects.id と board_runs.board_project_id の対応が制約に含まれていないため、別 BoardProject の run を latest_completed_run_id に入れても整合性違反にならない。
   - latest_completed_run_id は completed baseline の参照先なので、誤参照が入ると差分基準や表示整合性を壊す。

#### 必須修正
1. latest_completed_run_id が同一 board_project の board_run のみを参照できるよう、複合 FK か同等の整合性制約を追加する。
   - 例: board_runs に UNIQUE (id, board_project_id) を追加し、board_projects の (latest_completed_run_id, id) から board_runs(id, board_project_id) を参照する。

#### 任意改善
1. docs/external/sqlx-chrono-timestamptz.md では TIMESTAMPTZ NOT NULL DEFAULT now() を採用としている一方、DDL は created_at / updated_at をすべてアプリ入力前提にしている。schema か調査メモのどちらかに方針を寄せた方がよい。

#### テスト不足
1. latest_completed_run_id が別 board_project の run を拒否することを確認する DB レベルの失敗系テストがない。

#### plan / research / docs との不整合
- docs/external/sqlx-chrono-timestamptz.md の採用方針では DEFAULT now() を前提としているが、現行 DDL には反映されていない。

#### 残リスク
- アプリ層の更新ミスがあると、別 project の BoardRun を latest_completed_run_id に永続化できてしまう。

### PR/完了結果
- pr_ready: false
- 再レビュー条件: latest_completed_run_id の同一 board_project 整合性保証を追加すること

---

## レビュー指摘修正フェーズ (2026-04-30)

### 対応内容: latest_completed_run_id の整合性保証

**問題**: `board_projects.latest_completed_run_id` が `board_runs(id)` を単純FK参照しており、別プロジェクトの run を参照できてしまっていた。

**修正**:
1. `board_runs` テーブルに `CONSTRAINT board_runs_id_board_project_id_unique UNIQUE (id, board_project_id)` を追加
2. `board_projects_latest_completed_run_id_fk` を複合FKに変更:
   ```sql
   FOREIGN KEY (latest_completed_run_id, id) REFERENCES board_runs(id, board_project_id)
   ```
   これにより「latest_completed_run_id が指す board_run の board_project_id がこの board_project の id と一致すること」を DB レベルで保証。

**down.sql**: 変更不要（FK名同一、UNIQUE制約はDROP TABLEで消滅）

### 検証結果

| テスト | 結果 |
|---|---|
| `cargo build` | ✅ 成功 |
| `cargo test` | ✅ 全テスト通過 (3 passed, 0 failed) |
| `sqlx migrate run` | ✅ 成功 |
| `sqlx migrate revert` | ✅ 成功 |
| revert → run ラウンドトリップ | ✅ 成功 |

### コミット
- `506d844` fix(db): use composite FK for latest_completed_run_id integrity

### 残リスク
- DB レベルの失敗系テスト（別 project の run を latest_completed_run_id に入れて拒否確認）は後続Issueの統合テストで対応
- タイムスタンプ DEFAULT now() 方針の不整合は後続Issueで整理

### PR/完了結果
- pr_ready: true
- レビュー指摘事項: 解決済み

---

## 最終レビューフェーズ (2026-04-30)

### 対象Issue
- Issue #2: DBマイグレーション・データモデル実装

### 調査結果
- spec.md Section 10、docs/backend、docs/external、README、対象 migration 4ファイル、domain model 群、既存 worklog を再照合した。
- PostgreSQL の複合FKは参照先に UNIQUE 制約が必要であり、`board_runs(id, board_project_id)` への UNIQUE 追加と `board_projects(latest_completed_run_id, id)` からの参照により、同一 board_project 整合性が満たされていることを確認した。
- SQLx の reversible migration として `.up.sql` / `.down.sql` が揃っており、Issue 本文の `sqlx migrate revert` 成功条件とも整合した。

### テスト結果
- `cargo build`: 成功
- `cargo test`: 成功 (3 passed)
- `sqlx migrate info --source crates/db/migrations`: `20260430000000 init` / `20260430000001 create schema` の状態確認
- `sqlx migrate run --source crates/db/migrations`: 成功
- `sqlx migrate revert --source crates/db/migrations`: 成功
- revert 後の `sqlx migrate run --source crates/db/migrations` 再実行: 成功

### ドキュメント確認
- spec.md Section 10 の 13 テーブル定義とは整合している。
- docs/external の採用判断と実装を照合した結果、ENUM ではなく TEXT + CHECK、UUID v7 はアプリ層生成、reversible migration の採用は整合している。
- 一方で、worklog 初期計画に残る「Simple migration」「mod.rs で全 re-export」「board_project_snapshots(board_run_id) は UNIQUE でカバー済み」などの記述は実装後の現状と一致していない。

### レビュー結果
- 判定: pr_ready: true

#### 指摘
1. ブロッカーは解消済み。`latest_completed_run_id` は [crates/db/migrations/20260430000001_create_schema.up.sql](crates/db/migrations/20260430000001_create_schema.up.sql#L35) の複合 UNIQUE と [crates/db/migrations/20260430000001_create_schema.up.sql](crates/db/migrations/20260430000001_create_schema.up.sql#L208) の複合 FK で、同一 board_project の run だけを参照できる。
2. 非ブロッカーだが、タイムスタンプの DEFAULT 方針は research / worklog と DDL がまだずれている。DDL は [crates/db/migrations/20260430000001_create_schema.up.sql](crates/db/migrations/20260430000001_create_schema.up.sql#L10) などでアプリ入力前提、調査メモは DEFAULT now() 前提のままである。
3. 非ブロッカーだが、Issue #2 の worklog には実装前計画の古い記述が残っており、後続Issueの参照元としては誤読余地がある。

#### 必須修正
- なし

#### 任意改善
1. worklog の初期計画・中間レビュー記述のうち、最終実装と不一致な箇所を整理して、後続Issueで参照しやすい状態にする。
2. `created_at` / `updated_at` などの DB 側 DEFAULT 方針を、schema か調査メモのどちらかへ寄せて明文化する。
3. `board_project_snapshots(board_run_id)` の cardinality を 1:1 とみなすか 1:N を許容するかを仕様として明示する。

#### テスト不足
1. `latest_completed_run_id` の複合 FK が「別 board_project の run を拒否する」ことを直接確認する失敗系 DB テストは未追加。
2. CHECK / UNIQUE / FK の失敗系を自動検証する DB テストはまだなく、現状は migration 実行成功とコンパイル成功が主な担保になっている。

#### ドキュメント更新漏れ
- リポジトリ外向けドキュメントの必須更新漏れは見当たらない。
- ただし [docs/logs/2/worklog.md](docs/logs/2/worklog.md) 内の旧方針記述は整理余地がある。

#### plan / research / docs との不整合
- [docs/logs/2/worklog.md](docs/logs/2/worklog.md#L24) などの Simple migration 前提記述は、現在の reversible migration 実装と不整合。
- [docs/logs/2/worklog.md](docs/logs/2/worklog.md#L160) の「mod.rs で全モデル re-export」は、実装上はモジュール宣言のみで不整合。
- [docs/logs/2/worklog.md](docs/logs/2/worklog.md#L148) の `board_project_snapshots(board_run_id)` UNIQUE 前提は、現行 DDL と不整合。

### 残リスク
- DB 制約の失敗系挙動は手動確認ベースで、自動テストでの回帰検知はまだ弱い。
- タイムスタンプ既定値の設計方針が未収束のまま後続実装へ進むと、INSERT 実装ごとの責務分担がぶれる可能性がある。

### PR/完了結果
- pr_ready: true
- 最終レビュー完了。ブロッカーなし。
---

## ドキュメント確認フェーズ (2026-04-30)

### 対象Issue
- Issue #2: DBマイグレーション・データモデル実装

### 調査結果
- spec.md、README、docs/backend/summary.md、docs/external の4件、Issue #2 worklog、対象 migration 4ファイル、domain model 構成を照合した。
- 実装側では reversible migration（`.up.sql` / `.down.sql`）、UUID v7 のアプリ層生成、timestamp のアプリ入力前提が採用されている。
- しかし docs/external には、最終採用と一致しない記述が残っている。

### ドキュメント確認
- [docs/spec.md](docs/spec.md) は Issue #2 の実装内容と矛盾せず、更新不要。
- [docs/backend/summary.md](docs/backend/summary.md) は高レベル方針の記述に留まっており、本Issueでの必須更新は不要。
- [README.md](README.md) はリポジトリ概要のみで、Issue #2 に伴う必須更新は不要。
- [docs/external/sqlx-migration-format.md](docs/external/sqlx-migration-format.md) は Simple migration 採用を明記しており、現行の reversible migration 実装と不整合。
- [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) は `DEFAULT now()` 採用を明記しており、現行DDLの「アプリ入力前提（DEFAULT なし）」と不整合。
- [docs/logs/2/worklog.md](docs/logs/2/worklog.md) には、最終実装で置き換わった旧計画記述が残っており、履歴としては読めるが現状の参照元としては曖昧さが残る。

### 判定
- docs_ready: false

#### 必須修正
1. [docs/external/sqlx-migration-format.md](docs/external/sqlx-migration-format.md) の採用/不採用判断と BoardFlow への示唆を、最終実装に合わせて reversible migration 前提へ更新すること。
2. [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) の採用判断を、現行DDLに合わせて「アプリ入力前提」に更新するか、逆に schema / worklog / 実装方針を `DEFAULT now()` 採用に揃えるかを明示すること。

#### 任意改善
1. [docs/logs/2/worklog.md](docs/logs/2/worklog.md) の初期計画節に残る旧方針へ、最終的に superseded である旨を補記して後続Issueの参照を誤らせないようにする。
2. `board_project_snapshots(board_run_id)` の cardinality を、worklog か関連設計文書で 1:1 / 1:N のどちらかに明文化する。

#### 不整合のあるドキュメント
- [docs/external/sqlx-migration-format.md](docs/external/sqlx-migration-format.md)
- [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md)
- [docs/logs/2/worklog.md](docs/logs/2/worklog.md)

#### 不足しているドキュメント
- 新規追加が必須なドキュメントはなし。既存文書の整合性修正で足りる。

#### 外部調査メモに関する指摘
- [docs/external/sqlx-migration-format.md](docs/external/sqlx-migration-format.md) は「既存が simple なので今回も simple を採用」と結論しているが、その後の review 修正で reversible migration に変更されている。調査メモとして残すなら「当初判断 / 最終判断」の区別が必要。
- [docs/external/sqlx-chrono-timestamptz.md](docs/external/sqlx-chrono-timestamptz.md) は SQLx の型マッピング根拠としては有効だが、BoardFlow への採用判断だけが現在の schema とずれている。

### 残リスク
- docs/external の採用判断が未更新のままだと、後続Issueで migration 形式や timestamp 入力責務を誤って再判断する可能性がある。

### PR/完了結果
- docs_ready: false
- docs/external の採用判断を最終実装に合わせて更新後、再確認が必要。

---

## ドキュメント修正フェーズ (2026-04-30)

### 対象Issue
- Issue #2: DBマイグレーション・データモデル実装

### 実施内容

#### 修正1: docs/external/sqlx-migration-format.md
- 「BoardFlow への示唆」セクションを reversible migration 前提に更新
- 「採用/不採用判断」を **Reversible (可逆) 形式** に変更
- Issue #2 レビューで reversible を選択した経緯を理由に記載
- 旧 simple 前提の記述を削除

#### 修正2: docs/external/sqlx-chrono-timestamptz.md
- 「BoardFlow への示唆」セクションを「アプリ層で timestamp を設定する」方針に更新
- 「採用方針」セクションを新設し、DDL では DEFAULT を付けない方針を明記
- 理由: テスト時の再現性、アプリ側での一貫性確保

#### 修正3: docs/logs/2/worklog.md の旧計画整理
- 調査フェーズの Simple migration 前提記述に `[SUPERSEDED: reversible 採用]` 注記を追加
- DEFAULT now() 前提記述に `[SUPERSEDED: アプリ入力前提]` 注記を追加
- 実装方針まとめテーブルの該当行にも同様の注記を追加

### 変更ファイル

| ファイル | 操作 | 内容 |
|---|---|---|
| docs/external/sqlx-migration-format.md | 修正 | 採用判断を reversible に更新 |
| docs/external/sqlx-chrono-timestamptz.md | 修正 | 採用方針をアプリ入力前提に更新 |
| docs/logs/2/worklog.md | 修正 | 旧計画に SUPERSEDED 注記追加 |

### 残リスク
- なし（ドキュメントと実装の整合性を確保完了）

### PR/完了結果
- docs_ready: true
