# Issue #102 作業ログ — runner.rs 責務別モジュール分割

## Issue までの経緯

`crates/action-runner/src/runner.rs`（1309行）が GitHub Actions 入力解析→project discovery→plan API→KiCad export→artifact manifest→bundle upload→summary 出力まで広く担当。KiCad 出力の追加・変更時に差分が大きくなり保守性が低い。

## ユーザー要望

Issue本文に従い、runner.rs を `project_discovery`, `plan`, `artifact_pipeline`, `manifest_builder`, `submission` に分割するリファクタリング。ロジック移動のみ、挙動変更なし。

---

## 調査フェーズ（2026-05-14）

### 1. runner.rs の現在の構造

#### 型定義
| 型 | 行 | 用途 |
|---|---|---|
| `ValidProject` | L25-34 | 検証済みプロジェクト情報（private） |
| `ArtifactEntry` | L36-109 | artifact JSON エントリ（private、`Serialize`） |

#### 関数
| 関数 | 行範囲 | 行数 | 責務 |
|---|---|---|---|
| `run()` | L111-448 | ~337 | メインオーケストレーション |
| `process_project()` | L450-1107 | ~657 | 単一プロジェクト処理 |
| `build_plan_files()` | L1109-1140 | ~31 | plan用ファイル一覧構築 |
| `build_manifest_checks()` | L1142-1309 | ~168 | ERC/DRC manifest check構築 |

#### `run()` の責務区分
1. **入力解析** (L114-131): `inputs::parse_inputs()`, `inputs::parse_github_context()` — 既に inputs.rs に委譲済み
2. **イベントチェック** (L133-145): pull_request の場合 skip
3. **プロジェクト検出** (L147-156): `detect::find_boardflow_ymls()` 呼び出し
4. **プロジェクト検証** (L158-299): YML解析、ファイル解決、excludes マージ → `valid_projects: Vec<ValidProject>` 構築（~140行）
5. **Plan構築** (L301-357): tree hash計算、`PlanRequest` 構築
6. **Plan API呼び出し** (L359-366): `api.plan()` 呼び出し
7. **プロジェクト処理ループ** (L368-430): `process_project()` 呼び出し + 結果集約
8. **Summary出力** (L432-448): `summary::write_job_summary()` + exit code

#### `process_project()` の責務区分
1. **Board Run 作成** (L456-511): temp dir生成、tree hash計算、API呼び出し、artifact_bundle存在チェック
2. **KiCad Exports** (L513-995): ERC, DRC, PCB PDF, Sch PDF, SVG×2, Gerber, Drill, Fab zip, BOM, Position, 3D renders×2, iBOM — 各 export が ok/err で `ArtifactEntry` を push（~480行）
3. **Source files** (L997-1041): KiCad ソースファイルを artifact に追加
4. **Diff metadata** (L1043-1074): `bundle::generate_*` 呼び出し群
5. **Manifest構築** (L1076-1089): `build_manifest_checks()`, `build_plan_files()`, `bundle::create_manifest()` 呼び出し
6. **Submission** (L1091-1107): staging dir構築、bundle zip作成、upload、import

### 2. 外部インターフェース（公開境界）

- **唯一の公開API**: `pub async fn run() -> i32`（`main.rs` からのみ呼び出し）
- `ValidProject`, `ArtifactEntry`, `process_project`, `build_plan_files`, `build_manifest_checks` は全て private
- テストファイルは `runner` モジュールを直接テストしていない

### 3. テスト構造と依存関係

| テストファイル | テスト対象 | runner.rs への依存 |
|---|---|---|
| `api_test.rs` | `crate::api::ApiClient` | なし |
| `bundle_test.rs` | `crate::bundle::*` | なし |
| `inputs_test.rs` | `crate::inputs::*` | なし |
| `summary_test.rs` | `crate::summary::*` | なし |

**結論**: テストは全て runner.rs 以外のモジュールを対象としており、runner.rs の分割はテストのインポートパスに影響しない。

### 4. bundle.rs との関連

- `bundle.rs` は runner.rs から `crate::bundle::*` として呼び出される公開関数群を持つ
- `ArtifactEntry` は runner.rs 内で定義され、`serde_json::to_value()` で `serde_json::Value` に変換後、`bundle::create_manifest()` 等に渡される
- `ArtifactEntry` は `bundle.rs` が直接依存していないため、移動先は自由（artifact_pipeline が自然）

---

## 分割方針

### モジュール構成案

```
crates/action-runner/src/
├── main.rs                    # エントリポイント（変更なし）
├── runner.rs                  # 薄いオーケストレーター（run() のみ）
├── runner/
│   ├── project_discovery.rs   # ValidProject, プロジェクト検出・検証, build_plan_files
│   ├── plan.rs                # PlanRequest 構築
│   ├── artifact_pipeline.rs   # ArtifactEntry, KiCad export 各ステップ, source file収集
│   ├── manifest_builder.rs    # build_manifest_checks, diff metadata, manifest構築
│   └── submission.rs          # board run 作成, staging/bundle/upload/import
├── api.rs                     # 変更なし
├── bundle.rs                  # 変更なし
├── error.rs                   # 変更なし
├── inputs.rs                  # 変更なし
└── summary.rs                 # 変更なし
```

### 代替案: runner.rs をディレクトリモジュール化

`runner.rs` → `runner/mod.rs` + サブモジュール。`main.rs` の `runner::run()` パスは維持される。

### 各モジュールの責務と行数見積もり

| モジュール | 主な内容 | 概算行数 |
|---|---|---|
| `runner/mod.rs` | `run()` オーケストレーション（入力解析、イベントチェック、ループ、summary） | ~120 |
| `project_discovery.rs` | `ValidProject`, 検出・検証ロジック, `build_plan_files()` | ~200 |
| `plan.rs` | `PlanRequest` 構築 | ~70 |
| `artifact_pipeline.rs` | `ArtifactEntry`, KiCad export全ステップ + source files | ~550 |
| `manifest_builder.rs` | `build_manifest_checks()`, diff metadata呼び出し, manifest構築 | ~220 |
| `submission.rs` | board run作成, bundle zip, upload, import | ~80 |

### 分割境界の詳細

#### project_discovery.rs に移動
- `struct ValidProject` と関連バリデーション
- `run()` の L147-299（プロジェクト検出・検証ループ）を関数化
- `build_plan_files()` ヘルパー

#### plan.rs に移動
- `run()` の L301-357（hash計算 + PlanRequest構築）を関数化

#### artifact_pipeline.rs に移動
- `struct ArtifactEntry` と `available()`, `failed()`, `source()` コンストラクタ
- `process_project()` の L513-1041（全 KiCad export + source files）を step 関数群に分割
  - `run_erc_step()`, `run_drc_step()`, `export_pcb_pdf_step()`, etc.
  - または `generate_artifacts()` として一括

#### manifest_builder.rs に移動
- `build_manifest_checks()` 関数
- `process_project()` の L1043-1089（diff metadata + manifest作成）を関数化

#### submission.rs に移動
- `process_project()` の L456-511（board run作成）+ L1091-1107（staging/upload/import）を関数化

### リスクと注意点

1. **`process_project` の引数が多い**: KiCad CLI, API client, ValidProject, GitHubContext, ActionInputs, board_project_id — context struct にまとめるか、そのまま引き回すか判断が必要
2. **`artifacts: Vec<serde_json::Value>` の受け渡し**: artifact_pipeline → manifest_builder → submission で共有される可変ベクタ。artifact_pipeline が返す形にするのが自然
3. **`checks_failed` フラグ**: ERC/DRC の結果に依存。artifact_pipeline の戻り値に含める
4. **一時ディレクトリのライフタイム**: `tempfile::TempDir` の所有権は `process_project` レベルで維持し、各ステップには `&Path` で渡す

---

## 実装計画（Plan フェーズ 2026-05-14）

### 実装要否: `implementation_required`

- 外部ライブラリ調査は不要（純粋な Rust コード内リファクタリング）
- 分割方針と境界は明確
- テストへの影響なし（runner.rs の関数は全て private でテストから直接参照されていない）

---

### 目的

`runner.rs`（1309行）を責務別モジュールに分割し、KiCad artifact 追加時に変更対象が明確になるようにする。

### 非目的

- ロジックの変更・改善
- 新機能の追加
- テストの追加・変更
- `api.rs`, `bundle.rs`, `inputs.rs`, `summary.rs`, `error.rs` への変更

### 受け入れ条件

1. `runner.rs` が `runner/mod.rs` + 5 サブモジュールに分割されている
2. `main.rs` の `runner::run()` 呼び出しパスが維持されている
3. 既存テスト 4 ファイル（api_test, bundle_test, inputs_test, summary_test）が全パス
4. `cargo fmt --all -- --check` パス
5. `cargo clippy --workspace --all-targets -- -D warnings` パス
6. `cargo test --workspace` パス（config_test は DATABASE_URL 依存で対象外）
7. GitHub Actions 出力、summary、API payload の互換性が完全維持

---

### 詳細要件: モジュール構成と移動仕様

#### ファイル構造

```
crates/action-runner/src/
├── main.rs                        # 変更なし
├── runner/
│   ├── mod.rs                     # run() + process_project() オーケストレーション
│   ├── project_discovery.rs       # ValidProject, 検出・検証, build_plan_files
│   ├── plan.rs                    # PlanRequest 構築
│   ├── artifact_pipeline.rs       # ArtifactEntry, KiCad export, source file 収集
│   ├── manifest_builder.rs        # build_manifest_checks, diff metadata, manifest
│   └── submission.rs              # board run 作成, staging/bundle/upload/import
├── api.rs                         # 変更なし
├── bundle.rs                      # 変更なし
├── error.rs                       # 変更なし
├── inputs.rs                      # 変更なし
└── summary.rs                     # 変更なし
```

---

#### Module 1: `runner/project_discovery.rs` (~200行)

**移動する型:**
- `ValidProject` struct (L25-34) → `pub(super)` visibility、全フィールド `pub(super)`

**移動する関数:**
- `build_plan_files()` (L1109-1140) → `pub(super) fn build_plan_files(project_dir: &Path, excludes: &[String]) -> Vec<PlanProjectFile>`
- `run()` の L147-299 を抽出 → `pub(super) fn discover_and_validate(gh: &GitHubContext, action_inputs: &ActionInputs) -> Result<(Vec<ValidProject>, u32), String>`
  - `Err(msg)` は「No .boardflow.yml found」の場合
  - `Ok((vec, error_count))` は検出成功の場合

**必要な use:**
```rust
use std::path::{Path, PathBuf};
use boardflow_api_types::plan::PlanProjectFile;
use boardflow_kicad::config::{self, BoardflowConfig};
use boardflow_kicad::{detect, hash};
use crate::inputs::{ActionInputs, GitHubContext};
use crate::summary;
```

**ビジビリティ:**
- `ValidProject`: `pub(super)` (mod.rs, plan.rs, artifact_pipeline.rs, manifest_builder.rs, submission.rs から参照)
- `ValidProject` 全フィールド: `pub(super)` (各モジュールから `vp.project_dir` 等でアクセス)
- `discover_and_validate`: `pub(super)` (mod.rs から呼出)
- `build_plan_files`: `pub(super)` (plan.rs, manifest_builder.rs から呼出)

---

#### Module 2: `runner/plan.rs` (~70行)

**抽出する処理:**
- `run()` の L301-357 (tree hash 計算 + PlanRequest 構築) → `pub(super) fn build_plan_request(...) -> PlanRequest`

**関数シグネチャ:**
```rust
pub(super) fn build_plan_request(
    valid_projects: &[ValidProject],
    gh: &GitHubContext,
    action_inputs: &ActionInputs,
) -> PlanRequest
```

- 内部で `project_discovery::build_plan_files()` を呼ぶ
- tree hash 計算失敗時は warning + skip（現行同様）

**必要な use:**
```rust
use boardflow_api_types::plan::{
    PlanActionInput, PlanGitInput, PlanMode, PlanProjectInput,
    PlanRepositoryInput, PlanRequest,
};
use boardflow_kicad::hash;
use crate::inputs::{ActionInputs, GitHubContext};
use crate::summary;
use super::project_discovery::{self, ValidProject};
```

**ビジビリティ:**
- `build_plan_request`: `pub(super)` (mod.rs から呼出)

---

#### Module 3: `runner/artifact_pipeline.rs` (~550行)

**移動する型:**
- `ArtifactEntry` struct + impl (L36-109) → `pub(super)` visibility
  - `available()`, `failed()`, `source()`: `pub(super)`

**抽出する処理:**

1. `pub(super) async fn run_exports(...)` — `process_project()` の L503-995 (ERC〜iBOM)
2. `pub(super) fn collect_source_files(...)` — `process_project()` の L997-1041

**関数シグネチャ:**
```rust
/// KiCad export pipeline の結果
pub(super) struct ExportResult {
    pub artifacts: Vec<serde_json::Value>,
    pub checks_failed: bool,
    pub erc_json: PathBuf,
    pub drc_json: PathBuf,
    pub bom_dir: PathBuf,
}

pub(super) async fn run_exports(
    kicad: &KicadCli,
    vp: &ValidProject,
    inputs: &ActionInputs,
    output_path: &Path,
) -> Result<ExportResult, ActionError>

pub(super) fn collect_source_files(
    vp: &ValidProject,
) -> Vec<serde_json::Value>
```

**必要な use:**
```rust
use std::fs;
use std::path::{Path, PathBuf};
use boardflow_domain::models::artifact::ArtifactType;
use boardflow_kicad::cli::{KicadCli, PcbSide};
use boardflow_kicad::hash;
use serde::Serialize;
use tracing::warn;
use crate::bundle;
use crate::error::ActionError;
use crate::inputs::ActionInputs;
use super::project_discovery::ValidProject;
```

**設計判断:**
- `run_exports` は `Result` を返すが、個別 export の失敗は内部で `ArtifactEntry::failed()` として記録（現行動作維持）
- `Result::Err` は `fs::create_dir_all` の IO エラーのみ
- `ExportResult.erc_json` / `drc_json` / `bom_dir` は manifest_builder が diff metadata 生成に使用

---

#### Module 4: `runner/manifest_builder.rs` (~220行)

**移動する関数:**
- `build_manifest_checks()` (L1142-1309) → `pub(super) fn build_manifest_checks(erc_path: &Path, drc_path: &Path) -> Vec<serde_json::Value>`

**抽出する処理:**

1. `pub(super) fn generate_diff_metadata(...)` — `process_project()` の L1043-1074
2. `pub(super) fn build_manifest(...)` — `process_project()` の L1076-1089

**関数シグネチャ:**
```rust
pub(super) fn generate_diff_metadata(
    vp: &ValidProject,
    erc_json: &Path,
    drc_json: &Path,
    bom_dir: &Path,
    artifacts: &[serde_json::Value],
    output_path: &Path,
) -> Result<PathBuf, ActionError>
// Returns diff_dir path

pub(super) fn build_manifest(
    vp: &ValidProject,
    gh_sha: &str,
    tree_hash: &str,
    artifacts: &[serde_json::Value],
    erc_json: &Path,
    drc_json: &Path,
    diff_dir: &Path,
    output_path: &Path,
) -> Result<PathBuf, ActionError>
// Returns manifest_path
```

**必要な use:**
```rust
use std::fs;
use std::path::{Path, PathBuf};
use boardflow_kicad::hash;
use crate::bundle;
use crate::error::ActionError;
use super::project_discovery::{self, ValidProject};
```

**ビジビリティ:**
- `build_manifest_checks`: `pub(super)`
- `generate_diff_metadata`: `pub(super)`
- `build_manifest`: `pub(super)`

---

#### Module 5: `runner/submission.rs` (~80行)

**抽出する処理:**

1. `pub(super) async fn create_board_run(...)` — `process_project()` の L456-511
2. `pub(super) async fn submit_bundle(...)` — `process_project()` の L1091-1107

**関数シグネチャ:**
```rust
use boardflow_api_types::board_run::{
    CreateBoardRunRequest, CreateBoardRunResponse, ImportArtifactBundleRequest,
};
use boardflow_domain::public_ids::{BoardProjectId, BoardRunId};

pub(super) async fn create_board_run(
    api: &ApiClient,
    vp: &ValidProject,
    gh: &GitHubContext,
    board_project_id: BoardProjectId,
) -> Result<CreateBoardRunResponse, ActionError>

pub(super) async fn submit_bundle(
    api: &ApiClient,
    board_run_id: BoardRunId,
    upload_url: &str,
    staging_object_key: &str,
    vp: &ValidProject,
    output_path: &Path,
    manifest_path: &Path,
) -> Result<(), ActionError>
```

**`submit_bundle` の api.fail() 呼び出し:** 現行コードの `api.fail()` エラーハンドリングはそのまま submission.rs 内に含める（挙動変更なし）。

**必要な use:**
```rust
use std::fs;
use std::path::Path;
use boardflow_api_types::board_run::{
    CreateBoardRunRequest, CreateBoardRunResponse, ImportArtifactBundleRequest,
};
use boardflow_domain::public_ids::{BoardProjectId, BoardRunId};
use boardflow_kicad::hash;
use tracing::info;
use crate::api::ApiClient;
use crate::bundle;
use crate::error::ActionError;
use crate::inputs::GitHubContext;
use super::project_discovery::ValidProject;
```

---

#### Module 0: `runner/mod.rs` (~120行)

**残す処理:**
- `pub async fn run() -> i32` — 入力解析、イベントチェック、discover_and_validate 呼出、build_plan_request 呼出、plan API 呼出、process_project ループ、summary 出力
- `async fn process_project(...)` — 各サブモジュールの呼出をオーケストレーション

**`process_project()` のリファクタ後フロー:**
```rust
async fn process_project(
    kicad: &KicadCli,
    api: &ApiClient,
    vp: &project_discovery::ValidProject,
    gh: &GitHubContext,
    inputs: &ActionInputs,
    board_project_id: BoardProjectId,
) -> Result<bool, ActionError> {
    let pro_stem = ...;
    let output_dir = tempfile::Builder::new()...;
    let output_path = output_dir.path();

    // 1. Board run 作成
    let create_resp = submission::create_board_run(api, vp, gh, board_project_id).await?;
    let board_run_id = create_resp.board_run_id;
    let artifact_bundle = match &create_resp.artifact_bundle {
        Some(b) => b,
        None => { info!(...); return Ok(false); }
    };

    // 2. KiCad exports
    let export_result = artifact_pipeline::run_exports(kicad, vp, inputs, output_path).await?;
    let mut artifacts = export_result.artifacts;
    let checks_failed = export_result.checks_failed;

    // 3. Source files
    artifacts.extend(artifact_pipeline::collect_source_files(vp));

    // 4. Diff metadata
    let diff_dir = manifest_builder::generate_diff_metadata(
        vp, &export_result.erc_json, &export_result.drc_json,
        &export_result.bom_dir, &artifacts, output_path,
    )?;

    // 5. Manifest
    let tree_hash = hash::compute_tree_hash(&vp.project_dir, &vp.excludes)
        .map_err(|e| ActionError::Bundle(format!("tree hash: {e}")))?;
    let manifest_path = manifest_builder::build_manifest(
        vp, &gh.sha, &tree_hash, &artifacts,
        &export_result.erc_json, &export_result.drc_json,
        &diff_dir, output_path,
    )?;

    // 6. Submit
    submission::submit_bundle(
        api, board_run_id, &artifact_bundle.upload_url,
        &artifact_bundle.object_key, vp, output_path, &manifest_path,
    ).await?;

    info!("Successfully processed project: {}", vp.rel_pro_path);
    Ok(checks_failed)
}
```

**mod 宣言:**
```rust
mod artifact_pipeline;
mod manifest_builder;
mod plan;
mod project_discovery;
mod submission;
```

**必要な use:**
```rust
use boardflow_api_types::plan::PlanDecision;
use boardflow_domain::public_ids::BoardProjectId;
use boardflow_kicad::cli::KicadCli;
use boardflow_kicad::hash;
use tracing::{error, info};
use crate::api::ApiClient;
use crate::error::ActionError;
use crate::inputs::{self, ActionInputs, GitHubContext};
use crate::summary::{self, ProjectResult};
```

---

### 影響範囲

| ファイル | 変更 |
|---|---|
| `src/runner.rs` | 削除（`src/runner/mod.rs` に置換） |
| `src/runner/mod.rs` | 新規（薄いオーケストレーター） |
| `src/runner/project_discovery.rs` | 新規 |
| `src/runner/plan.rs` | 新規 |
| `src/runner/artifact_pipeline.rs` | 新規 |
| `src/runner/manifest_builder.rs` | 新規 |
| `src/runner/submission.rs` | 新規 |
| `src/main.rs` | **変更なし**（`mod runner; runner::run()` はそのまま動作） |
| `src/api.rs` | 変更なし |
| `src/bundle.rs` | 変更なし |
| `src/error.rs` | 変更なし |
| `src/inputs.rs` | 変更なし |
| `src/summary.rs` | 変更なし |
| `tests/*_test.rs` | **変更なし** |

---

### 実装ステップ（順序重要）

#### Step 1: ブランチ作成
```
git checkout main
git pull origin main
git checkout -b refactor/issue-102-split-runner
```

#### Step 2: runner.rs → runner/mod.rs 変換
```
mkdir -p crates/action-runner/src/runner
mv crates/action-runner/src/runner.rs crates/action-runner/src/runner/mod.rs
```
- `cargo test -p boardflow-action-runner` で変換が正常であることを確認

#### Step 3: project_discovery.rs を抽出
1. `runner/project_discovery.rs` を作成
2. `ValidProject` struct を移動（フィールドを `pub(super)` に変更）
3. `build_plan_files()` を移動（`pub(super)` に変更）
4. `run()` の L147-299 を `discover_and_validate()` として抽出
5. `mod.rs` に `mod project_discovery;` と `use project_discovery::ValidProject;` を追加
6. `mod.rs` 内の `ValidProject` 参照を更新
7. `cargo check -p boardflow-action-runner` でコンパイル確認

#### Step 4: plan.rs を抽出
1. `runner/plan.rs` を作成
2. `run()` の L301-357 を `build_plan_request()` として抽出
3. `mod.rs` に `mod plan;` を追加
4. `mod.rs` の `run()` で `plan::build_plan_request()` を呼出
5. `cargo check -p boardflow-action-runner` でコンパイル確認

#### Step 5: artifact_pipeline.rs を抽出
1. `runner/artifact_pipeline.rs` を作成
2. `ArtifactEntry` struct + impl を移動
3. `ExportResult` struct を定義
4. `process_project()` の KiCad export 部分を `run_exports()` として抽出
5. source file 収集部分を `collect_source_files()` として抽出
6. `mod.rs` に `mod artifact_pipeline;` を追加
7. `cargo check -p boardflow-action-runner` でコンパイル確認

#### Step 6: manifest_builder.rs を抽出
1. `runner/manifest_builder.rs` を作成
2. `build_manifest_checks()` を移動
3. diff metadata 生成部分を `generate_diff_metadata()` として抽出
4. manifest 構築部分を `build_manifest()` として抽出
5. `mod.rs` に `mod manifest_builder;` を追加
6. `cargo check -p boardflow-action-runner` でコンパイル確認

#### Step 7: submission.rs を抽出
1. `runner/submission.rs` を作成
2. board run 作成部分を `create_board_run()` として抽出
3. staging/bundle/upload/import 部分を `submit_bundle()` として抽出（`api.fail()` 含む）
4. `mod.rs` に `mod submission;` を追加
5. `cargo check -p boardflow-action-runner` でコンパイル確認

#### Step 8: 最終検証
```
mise exec -- cargo fmt --all -- --check
mise exec -- cargo clippy --workspace --all-targets -- -D warnings
mise exec -- cargo test --workspace
```
- config_test は DATABASE_URL 依存のため個別失敗を許容

#### Step 9: 元ファイルの確認・クリーンアップ
- `runner.rs` が残っていないことを確認
- 不要な import が mod.rs に残っていないことを確認
- `cargo clippy` で unused import 警告がないことを確認

---

### 設計方針

1. **移動のみ、ロジック変更なし** — 条件分岐、エラーハンドリング、ログ出力を一切変えない
2. **`pub(super)` 基本** — サブモジュール間の参照は `pub(super)`, crate 外に公開するのは `run()` のみ（`pub`）
3. **process_project は mod.rs に残す** — オーケストレーション関数として各モジュールを呼び出す薄いラッパー
4. **tree hash 計算は呼び出し元に残す** — `process_project` 内の tree hash 計算は mod.rs に残し、submission と manifest_builder に引数で渡す（`hash` crate の import を submission/manifest_builder に増やさない）
5. **ArtifactEntry は artifact_pipeline の所有物** — serde_json::Value に変換後、Vec として他モジュールに渡す

### テスト観点

1. **既存テスト不変**: 4 テストファイルは runner.rs 内部を直接参照していないため、import パス変更なし
2. **コンパイル検証**: 各 Step で `cargo check` を実行し、段階的に正当性を確認
3. **最終検証**: `cargo test --workspace` で全テストパス（config_test の DATABASE_URL 依存は対象外）
4. **互換性**: API payload (`PlanRequest`, `CreateBoardRunRequest`, `ImportArtifactBundleRequest`)、summary 出力、exit code の挙動は型シグネチャ・ロジックが同一のため維持される

### ドキュメント更新対象

- `docs/logs/102/worklog.md` — 実装中に追記
- その他のドキュメント変更は不要（runner.rs は内部実装であり、spec/API ドキュメントに影響しない）

### 未解決の疑問

なし。research フェーズで全ての構造が確認済み。

### 残リスク

1. `tree_hash` が `process_project` と `create_board_run` の2箇所で計算される冗長性は現行コードの動作をそのまま維持するため変更しない
2. `ArtifactEntry` の `serde_json::to_value().unwrap()` パターンは現行コードのまま（パニックリスクは元から存在するが本 Issue の scope 外）

---

## 実装フェーズ

### 実施日: 2026-05-14

### 実装手順

1. **ブランチ作成**: main から `refactor/issue-102-split-runner` を作成
2. **runner.rs → runner/mod.rs 変換**: ディレクトリ作成、ファイル移動、`cargo check` 確認 → コミット
3. **サブモジュール5ファイル作成 + mod.rs 書き換え**: 一括で実施

### 作成したファイル

| ファイル | 責務 | 主な公開シンボル |
|---------|------|----------------|
| `runner/project_discovery.rs` | プロジェクト検出・検証 | `ValidProject`, `discover_and_validate()`, `build_plan_files()` |
| `runner/plan.rs` | Plan APIリクエスト構築 | `build_plan_request()` |
| `runner/artifact_pipeline.rs` | KiCad export全ステップ | `ArtifactEntry`, `run_artifact_pipeline()` |
| `runner/manifest_builder.rs` | diff metadata + manifest | `build_diff_and_manifest()`, `build_manifest_checks()` |
| `runner/submission.rs` | Board run作成 + bundle送信 | `create_board_run()`, `submit_bundle()` |

### mod.rs の変更

- `run()` と `process_project()` を薄いオーケストレーターとして維持
- 旧コード1309行 → mod.rs 206行 + サブモジュール合計約1050行

### テスト結果

- `cargo fmt --all -- --check` → パス
- `cargo clippy --workspace --all-targets -- -D warnings` → パス
- `cargo test -p boardflow-action-runner` → 全テストパス
  - api_test: 8 passed, 1 ignored
  - bundle_test: 13 passed
  - inputs_test: 8 passed
  - summary_test: 6 passed
- `cargo test --workspace` → action-runner 全パス。kicad クレートの既存テスト1件 (`export_pcb_pdf_rejects_empty_output_file`) が fail だが本Issue対象外
- config_test の DATABASE_URL 依存失敗はこの実行では発生せず

### ドキュメント更新

- `docs/logs/102/worklog.md` を更新（本ファイル）
- 内部リファクタリングのため spec/API ドキュメントの変更は不要

### 残リスク

- なし。純粋なロジック移動であり、挙動変更なし。
