# Issue #89 - API型定義の共有crate（boardflow-api-types）を作成する

## 経緯

- Issue #88 の調査結果から、共有crate方式が最適と判断
- 同一Rustモノレポ内なので外部コード生成ツールは不要
- 調査ドキュメント: `docs/external/rust-shared-api-types-crate.md`

## ユーザー要望

- APIの型定義を一箇所で管理し、サーバ/クライアント間で共有する

## Issue作成内容

`crates/api-types/` を新規作成し、Plan API / BoardRun API の Request/Response 型を定義する。
`crates/api/` がこの crate の型を利用するようリファクタリング。

---

## 実装計画

### 目的

- API リクエスト/レスポンス型を単一 crate に集約し、`crates/api/` と `crates/action-runner/` で共有する
- コンパイル時に型不整合を検出できるようにする
- action-runner の `serde_json::json!({})` による手動ペイロード構築を型付き構造体リテラルに置き換える

### 非目的

- domain crate のリファクタリング
- API ハンドラロジックの変更
- OpenAPI スキーマの内容変更（互換性維持）
- action-runner の HTTP クライアント設計変更（リトライ等）

### 受け入れ条件

1. `crates/api-types/` が workspace に追加され、`cargo build --workspace` が成功する
2. `crates/api/` が `boardflow-api-types` の型を使用し、OpenAPI スキーマ出力が変化しない
3. `crates/action-runner/` が `boardflow-api-types` の型を使用し、既存テストが全パス
4. action-runner でリクエストペイロードが構造体リテラル + `serde_json::to_value()` に置き換わっている
5. `cargo clippy --workspace` が warning なし

---

### 詳細要件

#### 1. `crates/api-types/Cargo.toml`

```toml
[package]
name = "boardflow-api-types"
version = "0.0.1"
edition = "2024"

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }

[dependencies.utoipa]
workspace = true
optional = true

[features]
default = []
openapi = ["utoipa"]
```

#### 2. `crates/api-types/src/lib.rs`

```rust
pub mod plan;
pub mod board_run;

pub use plan::*;
pub use board_run::*;
```

#### 3. `crates/api-types/src/plan.rs` — 移動対象の型

**リクエスト型（Deserialize + Serialize）:**

| 構造体 | フィールド | 備考 |
|--------|-----------|------|
| `PlanRequest` | repository, git, action, mode, projects | |
| `PlanRepositoryInput` | github_repository_id, owner, name | |
| `PlanGitInput` | ref_ (serde rename "ref"), branch, commit_sha, event_name | |
| `PlanActionInput` | workflow, run_id, run_attempt | |
| `PlanMode` | Auto, All | enum, rename_all = "snake_case" |
| `PlanProjectInput` | project_path, config_path, project_dir, tree_hash, files | |
| `PlanProjectFile` | path, sha256 | |

**レスポンス型（Serialize + Deserialize）:**

| 構造体 | フィールド | 備考 |
|--------|-----------|------|
| `PlanResponse` | repository, projects | |
| `PlanRepositoryOutput` | github_repository_id, owner, name | |
| `PlanProjectOutput` | project_path, board_project_id, decision, reason, latest_completed_run_id (Option) | skip_serializing_if |
| `PlanDecision` | Build, Skip, Error | enum, rename_all = "snake_case" |
| `PlanReason` | NewProject, HashChanged, ... | enum, rename_all = "snake_case" |

**重要: 全型に `Serialize` + `Deserialize` を付与。** API 側は Deserialize(リクエスト)またはSerialize(レスポンス)のみ使用していたが、action-runner 側はリクエスト型を Serialize、レスポンス型を Deserialize で使うため、両方必要。

#### 4. `crates/api-types/src/board_run.rs` — 移動対象の型

**リクエスト型:**

| 構造体 | フィールド | 備考 |
|--------|-----------|------|
| `CreateBoardRunRequest` | board_project_id, project_path, tree_hash, commit_sha, branch, ref_, github_run_id, github_run_attempt | |
| `FailBoardRunRequest` | status, error | |
| `FailErrorInfo` | message, details (Option<serde_json::Value>) | |
| `ImportArtifactBundleRequest` | staging_object_key, bundle_sha256, bundle_size_bytes | |

**レスポンス型:**

| 構造体 | フィールド | 備考 |
|--------|-----------|------|
| `CreateBoardRunResponse` | board_run_id, status, artifact_bundle (Option) | |
| `ArtifactBundleInfo` | upload_mode, object_key, upload_url, method, expires_at | |
| `FailBoardRunResponse` | board_run_id, status, failed_at | |
| `ImportArtifactBundleResponse` | bundle_id, status | |

#### 5. `cfg_attr` パターン（全型共通）

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PlanRequest { ... }
```

#### 6. workspace `Cargo.toml` への追加

```toml
members = [
    ...
    "crates/api-types",
]

[workspace.dependencies]
boardflow-api-types = { path = "crates/api-types" }
```

#### 7. `crates/api/Cargo.toml` への追加

```toml
boardflow-api-types = { workspace = true, features = ["openapi"] }
```

`crates/api/src/routes/plan.rs` と `board_run.rs` から型定義を削除し、`use boardflow_api_types::*;` で置き換え。

#### 8. `crates/action-runner/Cargo.toml` への追加

```toml
boardflow-api-types = { workspace = true }
```

（default features = serde のみ、utoipa なし）

#### 9. action-runner リファクタリング

**`crates/action-runner/src/api.rs`:**
- `ProjectDecision`, `CreateBoardRunResponse`, `ArtifactBundleInfo`, `PlanProject`, `PlanFile` を削除
- `use boardflow_api_types::{PlanProjectOutput, CreateBoardRunResponse, ArtifactBundleInfo, PlanProjectInput, PlanProjectFile};` に置き換え
- `plan()` の戻り値を `Vec<ProjectDecision>` → `Vec<PlanProjectOutput>` に変更
- `plan()` 内のパース: `resp["projects"]` → `Vec<PlanProjectOutput>`

**`crates/action-runner/src/runner.rs`:**
- plan_payload: `serde_json::json!({})` → `PlanRequest` 構造体リテラル + `serde_json::to_value()`
- create_payload: `serde_json::json!({})` → `CreateBoardRunRequest` 構造体リテラル + `serde_json::to_value()`
- import_payload: `serde_json::json!({})` → `ImportArtifactBundleRequest` 構造体リテラル + `serde_json::to_value()`
- `PlanProject` → `PlanProjectInput` にリネーム
- `PlanFile` → `PlanProjectFile` にリネーム
- `decision.decision` の比較: `d.decision.as_str()` → `matches!(d.decision, PlanDecision::Build)` 等

**注意: `PlanRequest.repository.github_repository_id`**
- 現在の action-runner は plan_payload に `github_repository_id` を含めていない
- API 側 `PlanRepositoryInput` は `github_repository_id: String` を必須にしている
- この不整合は runner.rs で `gh.repository_id` (環境変数 `GITHUB_REPOSITORY_ID`) を設定するか、フィールドを `Option<String>` にするか判断が必要
- → 現状 API が動作している以上、`github_repository_id` はトークン認証で解決しており、リクエストでは空文字を許容している可能性。実際のAPI側ハンドラを確認済み: parse して使用。 **action-runner で `""` を渡している or API側で別経路。要確認して `Option<String>` + `#[serde(default)]` にするか、action-runner で値を設定する。**

---

### 影響範囲

| ファイル | 変更内容 |
|---------|---------|
| `Cargo.toml` (workspace) | members + workspace.dependencies に追加 |
| `crates/api-types/` (新規) | Cargo.toml, src/lib.rs, src/plan.rs, src/board_run.rs |
| `crates/api/Cargo.toml` | boardflow-api-types 依存追加 |
| `crates/api/src/routes/plan.rs` | 型定義削除、use 追加 |
| `crates/api/src/routes/board_run.rs` | 型定義削除、use 追加 |
| `crates/action-runner/Cargo.toml` | boardflow-api-types 依存追加 |
| `crates/action-runner/src/api.rs` | 型定義削除、use 追加、メソッドシグネチャ変更 |
| `crates/action-runner/src/runner.rs` | 構造体リテラル化、型名変更 |

---

### 設計方針

1. **全型に `Serialize` + `Deserialize` を付与** — API(サーバ) はリクエストを Deserialize、レスポンスを Serialize。action-runner(クライアント) はその逆。
2. **`#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]`** — utoipa は API crate のみで有効化。
3. **action-runner の `CreateBoardRunResponse.artifact_bundle` を `Option<ArtifactBundleInfo>` に修正** — 現在の action-runner 側は非Option だが、API の実レスポンスは `Option`。共有型で整合させ、action-runner 側で `.artifact_bundle.as_ref().ok_or(...)` でハンドリング。
4. **`PlanDecision` enum を action-runner でも使用** — 現在の `decision: String` → `decision: PlanDecision` に変更。パターンマッチで処理。
5. **段階的移行**: まず api-types crate 作成 → API 側移行（OpenAPIスキーマ差分確認） → action-runner 側移行の順。

---

### テスト観点

1. **既存テスト**: `cargo test --workspace` で全テストパス確認
2. **OpenAPI スキーマ互換**: API のテストで生成される OpenAPI JSON が変化しないことを差分比較
3. **action-runner integration テスト**: `crates/action-runner/tests/` の wiremock テストがパスすること
4. **feature flag テスト**:
   - `cargo build -p boardflow-api-types` (default = serde のみ)
   - `cargo build -p boardflow-api-types --features openapi` (utoipa 有効)
5. **型の Serialize/Deserialize ラウンドトリップ**: api-types に unit test を追加し、JSON ↔ 構造体の変換が正しいことを確認
6. **clippy**: `cargo clippy --workspace --all-targets -- -D warnings`

---

### ドキュメント更新対象

- `docs/backend/summary.md` — crate 一覧に `api-types` を追加
- `docs/spec.md` — アーキテクチャ図に api-types の位置を記載（必要に応じて）
- `docs/logs/89/worklog.md` — 本ファイル（実装進捗を追記）

---

### 実装ステップ（推奨順序）

| # | 内容 | 確認方法 |
|---|------|---------|
| 1 | `crates/api-types/` 作成 (Cargo.toml, src/) | `cargo build -p boardflow-api-types` |
| 2 | workspace Cargo.toml に member + dependency 追加 | `cargo build --workspace` |
| 3 | `crates/api/` の依存追加 + 型を api-types から import | `cargo build -p boardflow-api` |
| 4 | OpenAPI スキーマ差分確認 | テスト実行 or `cargo test -p boardflow-api` |
| 5 | `crates/action-runner/` の依存追加 + 型を api-types から import | `cargo build -p boardflow-action-runner` |
| 6 | action-runner のペイロード構築を構造体リテラル化 | `cargo test -p boardflow-action-runner` |
| 7 | clippy + 全テスト | `cargo clippy --workspace && cargo test --workspace` |

---

### 実装要否

`implementation_required`

### 未解決の疑問

1. **`github_repository_id` の扱い**: action-runner の plan payload に `github_repository_id` が含まれていない。API 側では `parse::<i64>()` しているが、空文字やフィールド欠落でエラーにならないのか？
   - **暫定方針**: action-runner 環境変数 `GITHUB_REPOSITORY_ID` が利用可能なので、それを `PlanRepositoryInput` に設定する。フィールドは `String` のまま維持。

2. **`CreateBoardRunResponse.artifact_bundle` の `Option` 化に伴う action-runner 側のエラーハンドリング**: 現在は非 Option で unwrap 相当。Option にした際に None の場合のエラーメッセージ設計。
   - **暫定方針**: `.ok_or_else(|| ActionError::Api("No artifact bundle info in response".into()))?` でフェイル。

### 残リスク

- `utoipa::ToSchema` の `cfg_attr` による条件コンパイルで、OpenAPI スキーマ出力のフィールド順序やメタデータに微差が出る可能性（utoipa のバージョンに依存）
- action-runner の wiremock テストが固定レスポンスを返している場合、型の変更で既存テストの修正が必要になる可能性

### 更新した作業ログパス

`docs/logs/89/worklog.md`
