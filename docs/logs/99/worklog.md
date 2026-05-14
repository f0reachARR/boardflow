# Issue #99: read.rs モジュール分割

## Issueまでの経緯

- `crates/api/src/routes/read.rs` が1679行に肥大化
- 読み取り系 API の DTO、カーソル処理、ID変換、認可、DB呼び出し、レスポンス変換、viewer source構築がすべて1ファイル
- #98 (PR #116) で cursor/pagination ロジックは `crates/api/src/pagination.rs` に抽出済み
- 機械的な挙動変更なしの分割を実施するフェーズ

## ユーザー要望

- 既存Issueの指示に従い、機能単位のmoduleに分割
- `openapi_router()` の route 登録が引き続き通る形にする
- 挙動変更なし

---

## 調査結果（2026-05-14）

### read.rs 構造分析

#### ファイル行数: 1679行

#### 定義されている型（enum / struct）

| 型名 | 種別 | 機能グループ | 行 |
|---|---|---|---|
| `BoardProjectState` | enum | dto | L91-99 |
| `ViewerAvailabilityStatus` | enum | dto (viewer系) | L101-111 |
| `ViewerSourceKind` | enum | dto (viewer系) | L113-118 |
| `RepositoryListItem` | struct | dto (repositories) | L168-176 |
| `RepositoryDetailResponse` | struct | dto (repositories) | L179-188 |
| `BoardProjectListItem` | struct | dto (board_projects) | L192-202 |
| `BoardProjectDetailResponse` | struct | dto (board_projects) | L205-225 |
| `RepositoryRef` | struct | dto (shared) | L227-231 |
| `BoardRunListItem` | struct | dto (board_runs) | L234-250 |
| `BoardRunDetailResponse` | struct | dto (board_runs) | L253-270 |
| `CheckInfo` | struct | dto (board_runs) | L273-279 |
| `ArtifactSummary` | struct | dto (board_runs) | L282-287 |
| `ArtifactListItem` | struct | dto (artifacts) | L290-307 |
| `ArtifactListResponse` | struct | dto (artifacts) | L934-936 |
| `ViewerSourcesResponse` | struct | dto (viewer_sources) | L312-316 |
| `BoardRunDiffResponse` | struct | dto (diff) | L320-329 |
| `DiffMetadataResponse` | struct | dto (diff) | L332-343 |
| `ViewerMap` | struct | dto (viewer_sources) | L345-352 |
| `ViewerStatus` | struct | dto (viewer_sources) | L355-365 |
| `ViewerSource` | struct | dto (viewer_sources) | L368-381 |
| `ViewerDownload` | struct | dto (viewer_sources) | L384-393 |
| `FindingsQueryParams` | struct | dto (findings) | L1537-1542 |
| `FindingListItem` | struct | dto (findings) | L1547-1559 |
| `CoordinateMmResponse` | struct | dto (findings) | L1562-1565 |

#### 定義されている関数

| 関数名 | 種別 | 機能グループ | 行 |
|---|---|---|---|
| `access_result_to_error` | pub helper | shared (access) | L29-52 |
| `access_error_to_app_error` | private helper | shared (access) | L54-67 |
| `find_artifact` | private helper | viewer_sources | L69-73 |
| `find_artifacts` | private helper | viewer_sources | L75-80 |
| `single_viewer_status` | private helper | viewer_sources | L83-89 |
| `parse_board_run_status` | private helper | shared (dto conversion) | L120-130 |
| `check_kind_str` | private helper | findings | L132-137 |
| `parse_check_kind` | private helper | findings | L139-144 |
| `finding_severity_str` | private helper | findings | L146-152 |
| `parse_finding_severity` | private helper | findings | L154-161 |
| `derive_board_project_state` | private helper | board_projects | L397-411 |
| `viewer_status` | private helper | viewer_sources | L1402-1422 |
| `list_repositories` | pub handler | repositories | L425-479 |
| `get_repository` | pub handler | repositories | L500-551 |
| `list_board_projects` | pub handler | board_projects | L565-641 |
| `get_board_project` | pub handler | board_projects | L656-717 |
| `list_board_runs` | pub handler | board_runs | L733-810 |
| `get_board_run` | pub handler | board_runs | L825-928 |
| `list_artifacts` | pub handler | artifacts | L949-1024 |
| `get_viewer_sources` | pub handler | viewer_sources | L1041-1395 |
| `get_board_run_diff` | pub handler | diff | L1440-1517 |
| `list_findings` | pub handler | findings | L1573-1679 |

### 外部参照

#### lib.rs の openapi_router() から（10ルート）
1. `routes::read::list_repositories`
2. `routes::read::get_repository`
3. `routes::read::list_board_projects`
4. `routes::read::get_board_project`
5. `routes::read::list_board_runs`
6. `routes::read::get_board_run`
7. `routes::read::list_artifacts`
8. `routes::read::get_viewer_sources`
9. `routes::read::get_board_run_diff`
10. `routes::read::list_findings`

#### 他ファイルからの参照
- `crates/api/src/routes/api_token.rs` L16: `use crate::routes::read::access_result_to_error;`

#### テストからの参照
- `crates/api/tests/read_api_test.rs`: HTTP経由のインテグレーションテスト（`create_app_with_config` 経由で全ルート含む）。read.rsの型を直接importしていない。
- `crates/api/tests/snapshots/openapi_schema_test__openapi_schema.snap`: OpenAPIスナップショットにDTO名が反映

#### pagination.rs（#98で抽出済み）
read.rs は以下を pagination.rs から import:
- `PaginatedResponse`, `PaginationParams`
- `decode_findings_cursor`, `encode_cursor`, `encode_findings_cursor`, `encode_repository_cursor`

### 共有ヘルパー（複数グループから使われるもの）

| ヘルパー | 使用元 |
|---|---|
| `access_result_to_error` | repositories, board_projects, board_runs, artifacts, viewer_sources, diff, findings, **api_token.rs** |
| `access_error_to_app_error` | repositories のみ（list_repositories） |
| `parse_board_run_status` | repositories, board_projects |
| `derive_board_project_state` | board_projects のみ |
| `find_artifact` / `find_artifacts` | viewer_sources のみ |
| `single_viewer_status` / `viewer_status` | viewer_sources のみ |
| `check_kind_str` / `parse_check_kind` | findings のみ |
| `finding_severity_str` / `parse_finding_severity` | findings のみ |

---

## 推奨分割方針

### Module構成

`crates/api/src/routes/read.rs` → `crates/api/src/routes/read/` ディレクトリに変換:

```
crates/api/src/routes/read/
├── mod.rs              # re-export のみ (pub use)
├── access.rs           # access_result_to_error, access_error_to_app_error
├── dto.rs              # 全DTO型 + BoardProjectState enum等共有enum
├── repositories.rs     # list_repositories, get_repository
├── board_projects.rs   # list_board_projects, get_board_project, derive_board_project_state
├── board_runs.rs       # list_board_runs, get_board_run
├── artifacts.rs        # list_artifacts, ArtifactListResponse
├── viewer_sources.rs   # get_viewer_sources, find_artifact, find_artifacts,
│                       # single_viewer_status, viewer_status
├── diff.rs             # get_board_run_diff
└── findings.rs         # list_findings, FindingsQueryParams, check_kind_str,
                        # parse_check_kind, finding_severity_str, parse_finding_severity
```

### 共有ヘルパー配置

- **`access.rs`**: `access_result_to_error` と `access_error_to_app_error` はほぼ全handlerから使われる。独立moduleが妥当。`api_token.rs` からの参照は `crate::routes::read::access_result_to_error` のまま維持可能（mod.rsでre-export）。
- **`dto.rs`**: 全DTO型 + `parse_board_run_status`（repositoriesとboard_projectsの両方で使用）をここに置く。
- 各機能module固有のヘルパー（`find_artifact`, `check_kind_str`等）はそのmodule内に留める。

### mod.rs の re-export 方針

`lib.rs` の `openapi_router()` は `routes::read::list_repositories` 等でアクセスしているため、`mod.rs` で全pub関数/型を re-export する:

```rust
pub use access::*;
pub use artifacts::*;
pub use board_projects::*;
pub use board_runs::*;
pub use diff::*;
pub use dto::*;
pub use findings::*;
pub use repositories::*;
pub use viewer_sources::*;
```

これにより `routes::read::` パスが変わらず、`lib.rs`・`api_token.rs`・テストの変更が不要。

### 推奨移動順序

1. `read.rs` → `read/mod.rs` に rename（内容そのまま）
2. `dto.rs` 切り出し（型定義のみ、依存なし）
3. `access.rs` 切り出し（共有ヘルパー）
4. `repositories.rs` 切り出し
5. `board_projects.rs` 切り出し
6. `board_runs.rs` 切り出し
7. `artifacts.rs` 切り出し
8. `viewer_sources.rs` 切り出し（最大のhandler、355行）
9. `diff.rs` 切り出し
10. `findings.rs` 切り出し
11. `mod.rs` を re-export のみに整理

各ステップで `cargo test --workspace` が通ることを確認。

---

---

## 実装計画（2026-05-14 plan フェーズ）

### 目的

`crates/api/src/routes/read.rs`（1679行）を機能単位のサブモジュールに分割し、変更しやすく・レビューしやすい構造にする。

### 非目的

- ハンドラのロジック変更、service層への移動
- DTO型名やAPIパス・レスポンス形式の変更
- viewer_sources ハンドラ内部のさらなるリファクタリング
- テストの追加（既存テストが通ればよい）

### 受け入れ条件

1. `read.rs` の巨大な単一ファイル状態が解消されている
2. 既存 API パス、レスポンス形式、OpenAPI 出力が意図せず変わらない
3. `cargo fmt --all -- --check` が通る
4. `cargo clippy --workspace --all-targets -- -D warnings` が通る
5. `cargo test --workspace` が通る（DB不要テストのみ。api_token_test はDB依存のため環境次第）
6. OpenAPIスナップショット (`crates/api/tests/snapshots/openapi_schema_test__openapi_schema.snap`) に差分がないこと

### 詳細要件

#### ファイル構成

```
crates/api/src/routes/read/
├── mod.rs              # submodule宣言 + pub use re-export
├── access.rs           # access_result_to_error, access_error_to_app_error
├── dto.rs              # 全DTO型 + parse_board_run_status
├── repositories.rs     # list_repositories, get_repository
├── board_projects.rs   # list_board_projects, get_board_project, derive_board_project_state
├── board_runs.rs       # list_board_runs, get_board_run
├── artifacts.rs        # list_artifacts, ArtifactListResponse
├── viewer_sources.rs   # get_viewer_sources + viewer系ヘルパー全て
├── diff.rs             # get_board_run_diff
└── findings.rs         # list_findings + findings系DTO/ヘルパー全て
```

#### 各モジュールの内容詳細

**`access.rs`** (約40行)
- `pub fn access_result_to_error(...)` — L29-52
- `fn access_error_to_app_error(...)` — L54-67
- import: `crate::error::{AppError, ErrorCode}`, `crate::github_access::{AccessError, AccessResult}`

**`dto.rs`** (約250行)
- enum: `BoardProjectState`, `ViewerAvailabilityStatus`, `ViewerSourceKind`
- struct: `RepositoryListItem`, `RepositoryDetailResponse`, `BoardProjectListItem`, `BoardProjectDetailResponse`, `RepositoryRef`, `BoardRunListItem`, `BoardRunDetailResponse`, `CheckInfo`, `ArtifactSummary`, `ArtifactListItem`, `ViewerSourcesResponse`, `BoardRunDiffResponse`, `DiffMetadataResponse`, `ViewerMap`, `ViewerStatus`, `ViewerSource`, `ViewerDownload`
- `fn parse_board_run_status(...)` — repositories / board_projects の両方から使用
- import: `boardflow_domain` の型、`serde`, `utoipa::ToSchema`, `boardflow_domain::public_ids::*`

**`repositories.rs`** (約140行)
- `pub async fn list_repositories(...)`
- `pub async fn get_repository(...)`
- import: `super::access::*`, `super::dto::*`, `crate::pagination::*`, `crate::error::*`, `crate::extractors::*`, `crate::github_access::*`

**`board_projects.rs`** (約170行)
- `pub async fn list_board_projects(...)`
- `pub async fn get_board_project(...)`
- `fn derive_board_project_state(...)` — このモジュール内でのみ使用
- import: 上記repositories同等 + `super::dto::parse_board_run_status`

**`board_runs.rs`** (約200行)
- `pub async fn list_board_runs(...)`
- `pub async fn get_board_run(...)`
- import: 上記同等

**`artifacts.rs`** (約100行)
- `pub struct ArtifactListResponse` — ハンドラのすぐ上で定義されておりhandlerと一体
- `pub async fn list_artifacts(...)`
- import: 上記同等

**`viewer_sources.rs`** (約370行)
- `pub async fn get_viewer_sources(...)`
- `fn find_artifact(...)`, `fn find_artifacts(...)`, `fn single_viewer_status(...)`, `fn viewer_status(...)`
- import: 上記同等 + `crate::{ArtifactBaseUrl, ArtifactSecret}`, `crate::artifact_token::*`, `chrono::Utc`

**`diff.rs`** (約80行)
- `pub async fn get_board_run_diff(...)`
- import: 上記同等

**`findings.rs`** (約170行)
- `pub struct FindingsQueryParams` — ハンドラ固有のクエリパラメータ
- `pub struct FindingListItem`, `pub struct CoordinateMmResponse`
- `pub async fn list_findings(...)`
- `fn check_kind_str(...)`, `fn parse_check_kind(...)`, `fn finding_severity_str(...)`, `fn parse_finding_severity(...)`
- import: 上記同等 + `boardflow_domain::models::run_check::*`

**`mod.rs`** (約20行)
```rust
mod access;
mod artifacts;
mod board_projects;
mod board_runs;
mod diff;
mod dto;
mod findings;
mod repositories;
mod viewer_sources;

pub use access::*;
pub use artifacts::*;
pub use board_projects::*;
pub use board_runs::*;
pub use diff::*;
pub use dto::*;
pub use findings::*;
pub use repositories::*;
pub use viewer_sources::*;
```

### 影響範囲

| ファイル | 変更内容 |
|---|---|
| `crates/api/src/routes/read.rs` | 削除（→ `read/` ディレクトリに置換） |
| `crates/api/src/routes/read/mod.rs` | 新規作成（re-export） |
| `crates/api/src/routes/read/access.rs` | 新規作成 |
| `crates/api/src/routes/read/dto.rs` | 新規作成 |
| `crates/api/src/routes/read/repositories.rs` | 新規作成 |
| `crates/api/src/routes/read/board_projects.rs` | 新規作成 |
| `crates/api/src/routes/read/board_runs.rs` | 新規作成 |
| `crates/api/src/routes/read/artifacts.rs` | 新規作成 |
| `crates/api/src/routes/read/viewer_sources.rs` | 新規作成 |
| `crates/api/src/routes/read/diff.rs` | 新規作成 |
| `crates/api/src/routes/read/findings.rs` | 新規作成 |
| `crates/api/src/routes/mod.rs` | **変更なし**（`pub mod read;` はディレクトリmoduleでも有効） |
| `crates/api/src/lib.rs` | **変更なし**（re-exportにより `routes::read::*` パス維持） |
| `crates/api/src/routes/api_token.rs` | **変更なし**（re-exportにより `crate::routes::read::access_result_to_error` 維持） |

### 設計方針

1. **glob re-export (`pub use xxx::*`)** で全pub関数/型を `mod.rs` から公開。`routes::read::` パスが不変になり、外部参照の変更ゼロ。
2. **サブモジュール間の参照は `super::` を使用**。例: `repositories.rs` は `super::access::access_result_to_error` を使う（ただし `super::dto::*` は `use super::dto::*;` でまとめて import）。
3. **`#[utoipa::path]` マクロはハンドラ関数と同じファイルに残す**。マクロは関数定義のattributeであり、module移動しても動作に影響しない。`routes!()` マクロは re-export されたパスで解決される。
4. **ハンドラ固有のDTO型はhandlerと同じmoduleに置く選択肢もある**が、Issue指示の「DTO分離」方針に従い `dto.rs` に集約する。ただし `FindingsQueryParams`, `FindingListItem`, `CoordinateMmResponse` は findings 固有なので `findings.rs` に、`ArtifactListResponse` は `artifacts.rs` に配置する。

### ステップバイステップの実装順序

#### Step 0: ブランチ作成
```bash
git checkout main
git pull origin main
git checkout -b refactor/issue-99-split-read-module
```

#### Step 1: read.rs → read/mod.rs (rename)
- `mv crates/api/src/routes/read.rs crates/api/src/routes/read/mod.rs`
- 検証: `cargo check -p boardflow-api`（内容同一なので確実に通る）

#### Step 2: access.rs 切り出し
- `access_result_to_error` (pub), `access_error_to_app_error` (fn) を `read/access.rs` に移動
- `mod.rs` に `mod access; pub use access::*;` 追加
- `mod.rs` 内の他関数から `access_result_to_error` / `access_error_to_app_error` への呼び出しはmodule内なのでそのまま動く（`pub use` で同スコープに展開）
- 検証: `cargo check -p boardflow-api`

#### Step 3: dto.rs 切り出し
- 全enum (`BoardProjectState`, `ViewerAvailabilityStatus`, `ViewerSourceKind`) を移動
- 全struct (RepositoryListItem 〜 ViewerDownload の17型) を移動
- `parse_board_run_status` 関数を移動（repositories と board_projects で共用）
- `mod.rs` に `mod dto; pub use dto::*;` 追加
- 検証: `cargo check -p boardflow-api`

#### Step 4: repositories.rs 切り出し
- `list_repositories`, `get_repository` を移動
- 各ハンドラの `#[utoipa::path]` attribute も含めて移動
- 必要な import を `repositories.rs` 先頭に記述
- 検証: `cargo check -p boardflow-api`

#### Step 5: board_projects.rs 切り出し
- `list_board_projects`, `get_board_project`, `derive_board_project_state` を移動
- 検証: `cargo check -p boardflow-api`

#### Step 6: board_runs.rs 切り出し
- `list_board_runs`, `get_board_run` を移動
- 検証: `cargo check -p boardflow-api`

#### Step 7: artifacts.rs 切り出し
- `ArtifactListResponse`, `list_artifacts` を移動
- 検証: `cargo check -p boardflow-api`

#### Step 8: viewer_sources.rs 切り出し
- `get_viewer_sources` と viewer系ヘルパー (`find_artifact`, `find_artifacts`, `single_viewer_status`, `viewer_status`) を移動
- 検証: `cargo check -p boardflow-api`

#### Step 9: diff.rs 切り出し
- `get_board_run_diff` を移動
- 検証: `cargo check -p boardflow-api`

#### Step 10: findings.rs 切り出し
- `FindingsQueryParams`, `FindingListItem`, `CoordinateMmResponse`, `list_findings` を移動
- findings固有ヘルパー (`check_kind_str`, `parse_check_kind`, `finding_severity_str`, `parse_finding_severity`) を移動
- 検証: `cargo check -p boardflow-api`

#### Step 11: mod.rs 最終整理
- `mod.rs` から残った import / 関数をすべて削除し、re-export のみに整理
- 検証: `cargo check -p boardflow-api`

#### Step 12: 最終検証
```bash
mise exec -- cargo fmt --all -- --check
mise exec -- cargo clippy --workspace --all-targets -- -D warnings
mise exec -- cargo test --workspace
# OpenAPI スナップショットに差分がないことを確認
git diff crates/api/tests/snapshots/
```

### import文の調整方針

各サブモジュールの共通importパターン:
```rust
// access系
use super::access::{access_error_to_app_error, access_result_to_error};
// （または `use super::access::*;` で省略）

// DTO型
use super::dto::*;

// crate共通
use crate::error::{AppError, RequestId};
use crate::extractors::AuthenticatedSession;
use crate::github_access::DynGithubAccessChecker;
use crate::pagination::{PaginatedResponse, PaginationParams, ...};

// 外部crate
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use sqlx::PgPool;
```

各ハンドラの import は元の `read.rs` 冒頭から必要なものだけを選んで配置。不要なimportが残らないよう clippy の `unused_imports` で検出。

### テスト観点

1. **コンパイル通過**: 各Step後に `cargo check` で即座に確認
2. **ユニットテスト**: `cargo test -p boardflow-api --lib` で api crate のユニットテスト通過
3. **インテグレーションテスト**: `cargo test -p boardflow-api --test read_api_test`（DB必要）
4. **OpenAPIスナップショット**: `cargo test -p boardflow-api --test openapi_schema_test` でスナップショットに差分なし確認
5. **workspace全体テスト**: `cargo test --workspace`（api_token_test は DB環境依存で失敗しうるが、read関連テストが通ればOK）
6. **フォーマット/lint**: `cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets -- -D warnings`

### ドキュメント更新対象

- なし（API仕様変更なし、OpenAPIスナップショット変更なし）
- この worklog のみ更新

### 実装要否

`implementation_required`

### 未解決の疑問

なし。すべて調査で解決済み。
- `routes!()` マクロは re-export パスで動作する → utoipa-axum の仕組み上、`pub use` で公開されたアイテムは元の module path でも re-export path でも参照可能
- `api_token.rs` の `use crate::routes::read::access_result_to_error` は re-export で維持可能 → Rust の module system の標準動作
- OpenAPI スキーマは DTO の `#[derive(ToSchema)]` と handler の `#[utoipa::path]` で生成されるため、module 移動で影響なし

### リスク分析

| リスク | 影響 | 対策 |
|---|---|---|
| `routes!()` マクロが re-export パスを解決できない | ビルド失敗 | Step 4 で最初のハンドラ移動時に即座に判明。もし失敗したら `mod.rs` に explicit re-export ではなく `pub mod repositories;` として `lib.rs` 側のパスを `routes::read::repositories::list_repositories` に変更する |
| OpenAPI スナップショットが変わる | テスト失敗 | `cargo insta review` で差分を確認し、型名・パスが同一なら accept |
| unused import warnings が clippy で検出 | clippy 失敗 | 各サブモジュールで必要な import のみ記述。Step 毎の clippy 確認で早期検出 |
| `viewer_sources.rs` が 370行と依然大きい | コード品質 | Issue #99 の scope は機械的分割。内部リファクタは別 Issue で対応 |

### 更新した作業ログパス

`docs/logs/99/worklog.md`

---

## 実装結果（2026-05-14）

### 実施内容

1. `read.rs` → `read/mod.rs` にリネーム（cargo check で確認）
2. 9つのサブモジュールを作成:
   - `access.rs`: `access_result_to_error`（pub）, `access_error_to_app_error`（pub(crate)に変更）
   - `dto.rs`: 全DTO型 + `parse_board_run_status`（pub(crate)）, `derive_board_project_state`（pub(crate)）
   - `repositories.rs`: `list_repositories`, `get_repository`
   - `board_projects.rs`: `list_board_projects`, `get_board_project`
   - `board_runs.rs`: `list_board_runs`, `get_board_run`
   - `artifacts.rs`: `ArtifactListResponse` + `list_artifacts`
   - `viewer_sources.rs`: `get_viewer_sources` + viewer系ヘルパー4つ（private）
   - `diff.rs`: `get_board_run_diff`
   - `findings.rs`: `FindingsQueryParams`, `FindingListItem`, `CoordinateMmResponse` + `list_findings` + findings系ヘルパー4つ（private）
3. `mod.rs` を `pub use xxx::*;` のみに置換
4. 不要な import を除去（`BoardProjectState` in board_projects.rs, `SubjectKind` in dto.rs）

### 可視性変更（モジュール分割に伴う最小限の変更）
- `access_error_to_app_error`: `fn` → `pub(crate) fn`
- `parse_board_run_status`: `fn` → `pub(crate) fn`
- `derive_board_project_state`: `fn` → `pub(crate) fn`

### 検証結果
- `cargo fmt --all -- --check`: PASS
- `cargo clippy --workspace --all-targets -- -D warnings`: PASS（warning 0）
- `cargo test --workspace`: config_test 以外全 PASS（config_test は DATABASE_URL 環境変数が設定済みのため失敗する既知の問題）
- OpenAPI スナップショット: 変化なし

### 残リスク
- なし。純粋なコード移動のみ。

---

## レビュー結果（2026-05-14 review フェーズ）

### レビュー観点

- コード品質: モジュール分割の凝集度、結合度、可視性、re-export
- 機能保持: 旧 `read.rs` の型・関数・ルート登録・OpenAPI
- 外部参照維持: `api_token.rs` の import、snapshot、テスト
- 規約準拠: `cargo fmt` / `cargo clippy`、AGENTS.md 方針
- ドキュメント整合: `docs/spec.md`、`README.md`、`docs/external/`、worklog

### 調査結果

- `main...HEAD` の差分は `crates/api/src/routes/read.rs` の削除と `crates/api/src/routes/read/` 配下9ファイル + `mod.rs` の追加、およびこの `worklog.md` のみで、Issue #99 の範囲に閉じていた。
- `crates/api/src/lib.rs` の `openapi_router()` は引き続き10本すべての read 系ハンドラを `routes::read::*` で登録していた。
- `crates/api/src/routes/api_token.rs` の `use crate::routes::read::access_result_to_error;` は維持され、re-export 経由の参照も成立している。
- `crates/api/src/routes/read/mod.rs` は private submodule + `pub use` で facade を構成しており、外部パスを変えずに内部構造だけを分割していた。
- 旧 `read.rs` にあった handler / DTO / helper の定義名は、`pub(crate)` 化された内部 helper を含めて分割後にも存在していた。
- `crates/api/tests/snapshots/openapi_schema_test__openapi_schema.snap` は `main...HEAD` で差分なし。
- `README.md` と `docs/spec.md` に今回の分割に伴って更新必須となる仕様差分は見当たらなかった。
- `CONTRIBUTING.md` は repository 内に存在せず、確認対象なし。

### テスト結果

- `main...HEAD` で OpenAPI snapshot 差分なしを確認。
- `env -u DATABASE_URL mise exec -- cargo test -p boardflow-api` を現作業ツリーで実行すると `config_test` が失敗した。
- 同じ `config_test` を `main` の一時 worktree で実行すると PASS した。
- この差は Issue #99 の差分ではなく、現作業ツリー固有のローカル環境要因（未追跡 `.env` 読み込み等）の影響と判断した。Issue #99 の変更ファイルに `config` 系差分は存在しない。

### ドキュメント確認

- `docs/spec.md`: read API の仕様変更を示す差分なし
- `README.md`: 開発・OpenAPI運用手順に今回の分割で追記必須な差分なし
- `docs/external/rust-module-pub-use-reexport.md` と外部調査: private submodule を `pub use` で再公開して既存パスを維持する構成は妥当

### レビュー結果

- 重大な指摘: なし
- 必須修正: なし
- 任意改善:
   - `derive_board_project_state` は `board_projects.rs` 専用 helper なので、将来的には `dto.rs` ではなく `board_projects.rs` に寄せると責務分離がより明確になる
   - `cargo test -p boardflow-api` がローカル `.env` に影響される点は、Issue #99 とは別件だがテスト安定性の課題として整理するとよい
- テスト不足:
   - read 系モジュール分割専用の構造テストは追加されていないが、HTTP integration test と OpenAPI snapshot で挙動保持は十分にカバーされている
- plan / research / docs との不整合:
   - 実装計画では `derive_board_project_state` を `board_projects.rs` に置く想定だったが、実装では `dto.rs` に置かれている。挙動影響はなく、設計上の軽微な差分に留まる

### PR/完了結果

- `pr_ready: true`
- 理由: Issue #99 の目的である巨大な `read.rs` の機能単位分割は達成されており、ルート登録・外部参照・OpenAPI snapshot は維持されていた。確認できた `config_test` の失敗は差分起因ではなく、現作業ツリーのローカル環境依存と判断した。

### 残リスク

- 現作業ツリーではローカル `.env` の影響で `config_test` が誤検知的に失敗しうる。CI や clean checkout では再現しない可能性が高いが、開発者体験上のノイズは残る。

### 更新した作業ログパス

- `docs/logs/99/worklog.md`

---

## ドキュメント確認結果（2026-05-14 docs フェーズ）

### 確認対象

- `docs/spec.md`
- `docs/backend/api.md`
- `docs/backend/summary.md`
- `docs/technology.md`
- `README.md`
- `AGENTS.md`
- `docs/logs/99/worklog.md`

### 確認結果

- Issue #99 の変更は `crates/api/src/routes/read.rs` の内部構造を `crates/api/src/routes/read/` 配下へ分割したもので、API パス、DTO 名、OpenAPI 出力、開発手順、技術方針の変更は確認できなかった。
- `docs/spec.md` はプロダクト仕様・API 振る舞いを説明しており、今回の純粋なモジュール分割で更新すべき箇所はなかった。
- `docs/backend/api.md` は read API の契約を説明しているが、`openapi_router()` の登録パスと OpenAPI snapshot が維持されているため、更新不要と判断した。
- `docs/backend/summary.md` と `docs/technology.md` は技術選定・責務分担の説明であり、内部ファイル分割による差分はなかった。
- `README.md` と `AGENTS.md` のビルド、テスト、開発フローにも変更はなく、追記不要だった。
- `docs/logs/99/worklog.md` は経緯、要望、調査、計画、実装、テスト、レビュー、PR可否、残リスクまで記録されており、Issue #99 の作業ログとして十分だった。

### ドキュメント更新要否

- 必須更新: なし
- 任意更新: なし
- 外部調査メモ更新: なし（今回の Issue は内部コード調査のみで完結しており、`docs/external/` の追加更新は不要）

### 判定

- `docs_ready: true`
- 理由: 実装内容が純粋なコード移動に留まり、既存ドキュメントの説明対象である API 仕様、技術方針、開発手順、運用手順に変更が発生していないため。

### 残リスク

- なし。現時点でドキュメントと実装の不整合は確認されなかった。

### 更新した作業ログパス

- `docs/logs/99/worklog.md`

---

## PR作成結果（2026-05-14 pr フェーズ）

### PR作成前確認

- review: `pr_ready: true` ✅
- docs: `docs_ready: true` ✅
- 未コミット変更: `docs/logs/99/worklog.md` のみ（worklog追記分）
- CI対象テスト（非DB）: 全PASS
- OpenAPIスナップショット: 差分なし
- ブランチ: `refactor/issue-99-split-read-module` → `main`
- 対象コミット: `6d905e4 refactor: split routes/read.rs into feature-based submodules`

### PR内容

- タイトル: `refactor: split read.rs into feature-based modules`
- Closes #99
- 分割構成: 10ファイル（mod.rs + 9サブモジュール）

### 残リスク

- `config_test` がローカル環境の `.env` 読み込みにより失敗するケースあり（Issue #99 の差分起因ではない）

### PR URL

https://github.com/f0reachARR/boardflow/pull/120
