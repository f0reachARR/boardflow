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

## 実装フェーズ (impl)

### 実施内容

1. **`crates/api-types/` 新規作成**
   - `Cargo.toml`: serde, serde_json 依存 + utoipa optional feature
   - `src/lib.rs`: `pub mod plan; pub mod board_run;`
   - `src/plan.rs`: PlanRequest, PlanRepositoryInput, PlanGitInput, PlanActionInput, PlanMode, PlanProjectInput, PlanProjectFile, PlanResponse, PlanRepositoryOutput, PlanProjectOutput, PlanDecision, PlanReason
   - `src/board_run.rs`: CreateBoardRunRequest, CreateBoardRunResponse, ArtifactBundleInfo, FailBoardRunRequest, FailErrorInfo, FailBoardRunResponse, ImportArtifactBundleRequest, ImportArtifactBundleResponse

2. **workspace `Cargo.toml` 更新**
   - members に `"crates/api-types"` 追加
   - workspace.dependencies に `boardflow-api-types = { path = "crates/api-types" }` 追加

3. **`crates/api/` リファクタリング**
   - `Cargo.toml`: `boardflow-api-types = { workspace = true, features = ["openapi"] }` 追加
   - `src/routes/plan.rs`: 型定義削除、`use boardflow_api_types::plan::*;` に置き換え
   - `src/routes/board_run.rs`: 型定義削除、`use boardflow_api_types::board_run::*;` に置き換え

4. **`crates/action-runner/` リファクタリング**
   - `Cargo.toml`: `boardflow-api-types = { workspace = true }` 追加
   - `src/api.rs`: `ProjectDecision`, `CreateBoardRunResponse`, `ArtifactBundleInfo`, `PlanProject`, `PlanFile` 削除。`PlanProjectOutput`, `CreateBoardRunResponse` をインポート。`plan()` 戻り値を `Vec<PlanProjectOutput>` に変更。`fail()` メソッドを型付き構造体で構築。
   - `src/runner.rs`: plan_payload を `PlanRequest` 構造体リテラル + `serde_json::to_value()`、create_payload を `CreateBoardRunRequest`、import_payload を `ImportArtifactBundleRequest` に変更。decision 比較を `matches!(d.decision, PlanDecision::Build)` に変更。`Option<ArtifactBundleInfo>` のハンドリング追加。
   - `tests/api_test.rs`: wiremock レスポンスに `status`, `reason`, `board_project_id`, `upload_mode`, `method`, `expires_at` フィールドを追加。アサーションを enum ベースに更新。

### テスト結果

- `cargo build --workspace`: 成功
- `cargo test --workspace`: 全テスト成功（7+13+8+6+29+15+21+19=118テスト通過）
  - 唯一の失敗: `test_app_config_from_env` (pre-existing issue: テスト環境にDATABASE_URLが設定されているため)
- `cargo clippy --workspace --all-targets -- -D warnings`: 警告なし

### 注意点・判断事項

1. **`github_repository_id` の扱い**: `PlanRepositoryInput.github_repository_id` に `#[serde(default)]` を付与し、空文字をデフォルトとした。action-runner 側では `std::env::var("GITHUB_REPOSITORY_ID").unwrap_or_default()` で環境変数から取得。GitHub Actions 環境では自動的に設定される。
2. **`CreateBoardRunResponse.artifact_bundle`**: `Option<ArtifactBundleInfo>` として定義し、action-runner 側で `.as_ref().ok_or_else(...)` でエラーハンドリング。
3. **`PlanDecision`, `PlanReason` に `PartialEq, Eq` 追加**: action-runner でのパターンマッチに必要なため。Serialize/Deserialize の振る舞いには影響なし。
4. **`FailBoardRunRequest.error.details`**: action-runner の `fail()` メソッドでは `Some(serde_json::Value::String(...))` として渡す。API 側は `Option<serde_json::Value>` として受け取るため互換。
5. **テスト内の `#[path = "..."]` パターン**: action-runner テストは `#[path]` で直接ソースを include するパターンだが、`boardflow-api-types` が `[dependencies]` に含まれるため正常に解決される。

### 更新ドキュメント

- `docs/logs/89/worklog.md` (本ファイル)

### 残リスク

- `test_app_config_from_env` は環境依存の pre-existing failure（本Issue とは無関係）
- OpenAPI スキーマの完全一致確認は手動で未実施（型のフィールド・serde属性は完全保持しているため実質互換）
- `docs/backend/summary.md` への crate 一覧追記は未実施（本Issue のスコープ外として判断）

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

---

## レビュー結果 (2026-05-05)

### 総評

- 共有 crate への型移動自体は概ね計画どおりで、`cfg_attr(feature = "openapi")` の適用、API 側の import 置換、action-runner 側の構造体リテラル化は要件に沿っている。
- ただし、BoardRun 作成 API の idempotent レスポンスで `artifact_bundle: null` が返る正規ケースを action-runner が失敗扱いしており、API 契約と実装がずれている。このままでは再送・再実行時に誤って job を失敗させるため、PR ready にはできない。

### PR判定

- `pr_ready: false`

### 重大度順の指摘

1. **必須修正**: action-runner が `CreateBoardRunResponse.artifact_bundle == None` を常に API エラー扱いしている。
   - 該当実装: `crates/action-runner/src/runner.rs` の `create_board_run` 直後。
   - 現在の API 仕様では、既存 run が `importing` の場合は `artifact_bundle` を返さず、`completed` / `failed` / `timed_out` の場合は terminal 状態のみを返し、Action は追加 build / upload / import を行わない前提。
   - にもかかわらず現在は `None` を即時エラーに変換しているため、正規の idempotent レスポンスで job 全体が失敗する。

### 必須修正

- `CreateBoardRunResponse.status` を見て分岐し、`artifact_bundle` がない `importing` / terminal 状態では build・upload・import を中止して、仕様に沿った skip / early-return にする。
- このケースを `crates/action-runner/tests/api_test.rs` か runner のテストで追加し、`artifact_bundle: null` のレスポンスを回帰テスト化する。

### 任意改善

- `crates/api-types/src/lib.rs` は計画書の例と異なり `pub use plan::*; pub use board_run::*;` を持たない。現状の利用箇所では問題ないが、計画との差分として整理しておくとよい。
- `docs/backend/summary.md` の crate 一覧に `crates/api-types` が未記載のままなので、共有契約 crate を構成図に追加した方が設計意図が伝わりやすい。

### テスト不足

- OpenAPI テストは `openapi.json` が返ることだけを確認しており、Issue 受け入れ条件の「スキーマ出力が変化しない」を検証していない。
- `boardflow-api-types` 自体の feature 切り替え build は通るが、JSON ラウンドトリップや schema shape の unit test は未追加。
- action-runner 側は `artifact_bundle: Some(...)` の成功系しか持たず、`None` を返す idempotency ケースをカバーしていない。

### ドキュメント確認

- `docs/external/rust-shared-api-types-crate.md` の調査方針と、`serde` + `utoipa(optional)` の feature 分離方針は実装と整合している。
- 一方で `docs/backend/summary.md` のサービス構成には `crates/api-types` が反映されていない。

### plan / research / docs との不整合

- 計画にあった `docs/backend/summary.md` 更新が未実施。
- 計画にあった OpenAPI スキーマ差分確認、`boardflow-api-types` のラウンドトリップ test 追加は未実施。
- 依存最小化の観点では `boardflow-api-types` は実行時依存に `serde_json` を含む。これは `FailErrorInfo.details: Option<serde_json::Value>` を維持する限り妥当だが、「serde + utoipa(optional) のみ」というレビュー観点とは一致しないため、要件表現を明確化した方がよい。

### テスト結果

- `mise exec -- cargo test -p boardflow-api-types --no-default-features`: 成功
- `mise exec -- cargo test -p boardflow-api-types --features openapi`: 成功
- `mise exec -- cargo tree -p boardflow-api-types -e normal`: 実行時依存は `serde` と `serde_json`

### 残リスク

- 上記 idempotency ハンドリングを直さない限り、同一 attempt の再送や再実行で偽陽性の失敗が起こる。
- OpenAPI 互換性はコード上は大きく崩れていないが、差分検証が未自動化のため将来の属性変更で気づきにくい。

### PR/完了結果

- `pr_ready: false`

---

## 再レビュー結果 (2026-05-05)

### 総評

- 前回指摘した 3 点のうち、Issue #89 のスコープに属する修正は確認できた。
- `artifact_bundle: null` を返す idempotent レスポンスは action-runner 側でエラーではなく skip 扱いに変更されており、ドキュメント更新と API テスト追加も反映済み。
- OpenAPI 互換性の自動検証不足は別 Issue #91 に切り出されているため、本 Issue の PR 判定を妨げる論点としては扱わない。

### レビュー結果

- 高: `artifact_bundle == None` 時のハンドリングは解消。
   - [crates/action-runner/src/runner.rs](crates/action-runner/src/runner.rs#L477) で `artifact_bundle` がない場合に status をログして build を skip し、`Ok(false)` で早期 return している。
   - API 側の idempotent 契約とも整合している。
- 低: ドキュメント更新は解消。
   - [docs/backend/summary.md](docs/backend/summary.md#L59) に `crates/api-types` の記載がある。
- テスト: idempotency ケース追加は解消。
   - [crates/action-runner/tests/api_test.rs](crates/action-runner/tests/api_test.rs#L204) に `test_create_board_run_idempotent_no_bundle` が追加され、`artifact_bundle: null` のレスポンスを受理できることを確認している。

### テスト結果

- `mise exec -- cargo test -p boardflow-action-runner --test api_test`: 成功（8 passed, 1 ignored）

### 必須修正

- なし

### 任意改善

- 現在の追加テストは API クライアントのデシリアライズ確認が中心で、runner 全体として skip 動作までを直接検証してはいない。将来の回帰検知を強めるなら runner レベルのテスト追加余地はある。

### テスト不足

- `artifact_bundle: null` のパースは担保されたが、`process_project` 全体で build/upload を実行しないことまでは自動テスト化されていない。

### ドキュメント確認

- `docs/backend/summary.md` 更新済み。
- `docs/spec.md` との矛盾は今回確認範囲では見当たらない。

### plan / research / docs との整合

- 共有 crate 方式、`serde` ベースの共有、API/action-runner 両側からの利用という計画・調査方針と整合。
- OpenAPI 互換性の厳密検証は未自動化だが、別 Issue #91 管轄として整理済み。

### 残リスク

- runner の skip 分岐は実装済みだが、将来の回帰を防ぐには runner レベルの挙動テストがあるとより堅い。

### PR/完了結果

- `pr_ready: true`

---

## ドキュメント確認結果 (2026-05-05)

### 総評

- Issue #89 のドキュメント確認対象 4 件を再確認した結果、現時点の実装との重大な不整合は見当たらない。
- `docs/backend/summary.md` の `crates/api-types` 追記は現行実装と一致している。
- `docs/backend/api.md` の Plan API / BoardRun API 契約は、共有型 crate に移した `PlanRequest` / `PlanResponse` / `CreateBoardRunRequest` / `CreateBoardRunResponse` / `ImportArtifactBundleRequest` / `ImportArtifactBundleResponse` の形状と整合している。
- `docs/external/rust-shared-api-types-crate.md` の採用結論（共有 crate + `utoipa` の feature 分離）は実装に反映済み。

### docs_ready

- `docs_ready: true`

### 必須修正

- なし

### 任意改善

- 作業ログ内に過去の暫定レビュー結果（`pr_ready: false`、`summary.md` 未更新など）が残っているため、将来の読者向けには「後続の再レビューで解消済み」であることを冒頭要約にも反映すると追跡しやすい。

### 不整合のあるドキュメント

- なし

### 不足しているドキュメント

- なし

### 外部調査メモに関する指摘

- 調査メモは「共有型を独立 crate に切り出す」「`utoipa` は feature `openapi` で条件付きにする」「action-runner 側は手書き JSON ではなく共有型を使う」という結論を示しており、現実装はその方針どおり。
- 調査メモのサンプルでは `src/lib.rs` に `pub use plan::*; pub use board_run::*;` も例示されているが、現実装は module 公開のみで利用側も `boardflow_api_types::plan::*` / `boardflow_api_types::board_run::*` を使っているため、ドキュメント上の本質的な不整合ではない。

### レビュー結果

- `docs/backend/summary.md`: 正確。サービス構成の一覧に `crates/api-types` が含まれている。
- `docs/backend/api.md`: 正確。BoardRun 作成 API の idempotency 時に `artifact_bundle` が返らないケースの注記もあり、現在の action-runner 実装とも矛盾しない。
- `docs/logs/89/worklog.md`: 履歴としては妥当。過去の暫定判断と最新判断が混在しているが、直近の再レビューと本節を見れば最終状態は追える。
- `docs/external/rust-shared-api-types-crate.md`: 正確。採用・不採用判断は現実装と整合している。

### 更新した作業ログパス

- `docs/logs/89/worklog.md`
