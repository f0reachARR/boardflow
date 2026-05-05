# Rust 共有 API 型 crate パターン — action-runner / API 間の型共有

## 要約

action-runner と API サーバが同一モノレポ内にある場合、API リクエスト/レスポンスの型定義を独立 crate (`crates/api-types/`) に切り出して双方から参照するパターンが最も適切。OpenAPI コード生成や domain crate 直接共有と比較し、依存の軽さ・型安全性・保守性のバランスが最善。

## 確認した情報

### 現状の型定義の重複

| 型名 | action-runner (`api.rs`) | API (`routes/plan.rs`, `routes/board_run.rs`) | 差異 |
|------|--------------------------|-----------------------------------------------|------|
| `ProjectDecision` | `decision: String`, `board_project_id: Option<String>` | `decision: PlanDecision` (enum), `board_project_id: String` | enum vs String、Option の有無 |
| `CreateBoardRunResponse` | `board_run_id: String`, `artifact_bundle: ArtifactBundleInfo` | `board_run_id: String`, `status: String`, `artifact_bundle: Option<ArtifactBundleInfo>` | `status` フィールド欠落、Option の有無 |
| `ArtifactBundleInfo` | `upload_url: String`, `object_key: String` | `upload_mode: String`, `object_key: String`, `upload_url: String`, `method: String`, `expires_at: String` | 3 フィールド欠落 |
| `PlanProject` / `PlanFile` | `#[allow(dead_code)]` 付き `Serialize` のみ | `PlanProjectInput` / `PlanProjectFile` として `Deserialize + ToSchema` | 名前不一致、使用状況不明確 |

### action-runner の API 呼び出しパターン

- `serde_json::json!({...})` マクロでリクエストを手動構築（`runner.rs` L318-333, L418-428）
- レスポンスは `serde_json::Value` として受信後、手動で `from_value` パース（`api.rs` L113-130）
- 型の不一致は実行時にしか検出できない

### API 側の型定義（`crates/api/src/routes/`）

- 全リクエスト/レスポンス型に `utoipa::ToSchema` が derive されている
- `axum::Json<T>` として直接使用 → axum に依存
- DB 操作は型定義と分離されている（handler 内で変換）

### domain crate の現状（`crates/domain/`）

- `sqlx::FromRow`, `sqlx::Type` が derive されたデータベースモデル
- `BoardRunStatus`, `ArtifactBundleStatus` 等の enum は API レスポンスにも使われている
- `sqlx` への依存が必須 → action-runner に引き込むと Docker イメージサイズ増大

## 各選択肢の評価

### A: 共有 crate (`crates/api-types/`)

API 層のリクエスト/レスポンス型のみを独立 crate に切り出す。

**Pros:**
- コンパイル時に型不整合を検出（API 側の型変更で action-runner もコンパイルエラー）
- 依存が最小（`serde`, `serde_json` のみ必須。`utoipa` は feature flag で条件付き）
- 既存の Cargo workspace に自然に統合
- Docker Action バイナリサイズへの影響がほぼゼロ
- 型のリネームや追加・削除が一箇所で完結
- OpenAPI スキーマ生成（`ToSchema`）も feature flag で保持可能

**Cons:**
- 手動で型定義を書く必要がある（自動生成ではない）
- API ハンドラのリクエスト/レスポンスが変わるたびに api-types の更新が必要
- `api` と `action-runner` で微妙にフィールドが異なる場合の設計判断が必要

**実装方針:**

```toml
# crates/api-types/Cargo.toml
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

```rust
// crates/api-types/src/plan.rs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PlanRequest {
    pub repository: PlanRepositoryInput,
    pub git: PlanGitInput,
    pub action: PlanActionInput,
    pub mode: PlanMode,
    pub projects: Vec<PlanProjectInput>,
}
```

依存グラフ:
```
api-types (serde のみ)
  ├── api (feature = "openapi" で utoipa 有効)
  └── action-runner (default features、serde のみ)
```

### B: OpenAPI コード生成 (`progenitor` 等)

OpenAPI スキーマ (JSON/YAML) からクライアントコードを自動生成する。

**Pros:**
- スキーマ変更に完全自動追従
- 型安全な HTTP クライアントが生成される（リクエストビルダー付き）
- 公式の Oxide Computer 製ライブラリで信頼性が高い

**Cons:**
- ビルドパイプラインが複雑化（スキーマ生成 → コード生成の 2 段階）
- `progenitor` は `reqwest` ベースの完全なクライアントを生成するため、既存の `ApiClient` のリトライ/タイムアウトロジックとの統合が困難
- コンパイル時間が大幅に増加（proc macro によるスキーマパース。Adam Chalmers の "Investigating crazy compile times" ブログで報告あり）
- 生成コードが不透明で、デバッグ時に追跡しにくい
- Docker Action バイナリサイズが増大（`progenitor-client` + 依存一式）
- 同一 Cargo workspace 内のモノレポでは over-engineering（型をソースコードとして直接共有できる）
- `utoipa` 出力のスキーマ形式と `progenitor` の入力仕様に互換性の問題がある可能性

**結論: 不採用**

同一モノレポ内で型を直接共有できる状況では、スキーマ経由のコード生成は不必要な複雑さを持ち込む。プロジェクト間の境界が組織的に分離されている（別リポジトリ、別チーム）場合に適切。

### C: domain crate 直接共有

既存の `crates/domain/` を `action-runner` から直接参照する。

**Pros:**
- 既存 crate をそのまま使える（新規作成不要）
- `BoardRunStatus` 等の enum は共有価値がある

**Cons:**
- `sqlx` への依存が必須（`sqlx::FromRow`, `sqlx::Type` が derive されている）
- `sqlx` を引き込むと Docker Action バイナリサイズが大幅増大（TLS + PostgreSQL ドライバ含む）
- domain モデルは DB 層の関心事（カラム型、リレーション）を含んでおり、API 契約とは責務が異なる
- API レスポンスの形式（prefixed ID `br_xxx`、ステータス文字列等）は domain モデルと直接対応しない

**結論: 不採用（ただし一部 enum の再 export は検討余地あり）**

domain crate は DB レイヤーのモデル。API 契約の型と責務が異なる。`BoardRunStatus` のような enum は api-types で再定義するか、domain crate を feature flag で `sqlx` 非依存にできるか検討の余地はあるが、現状ではコストが高い。

## BoardFlow への示唆

1. **共有 crate (`api-types`) を採用すべき** — 最小の依存で最大の型安全性を得られる
2. `utoipa::ToSchema` は feature flag `openapi` で条件コンパイルし、action-runner には `serde` のみ依存で提供
3. `runner.rs` のペイロード構築を `serde_json::json!({})` から構造体リテラルに変更し、コンパイル時型チェックの恩恵を受ける
4. API 側のレスポンス型の `Option<ArtifactBundleInfo>` を正しく反映し、フィールド欠落バグを防止

## 採用/不採用判断

| 選択肢 | 判断 | 理由 |
|---------|------|------|
| A: 共有 crate | **採用** | 最小依存、コンパイル時安全、モノレポに自然 |
| B: progenitor | 不採用 | over-engineering、バイナリサイズ増大、ビルド複雑化 |
| C: domain 直接共有 | 不採用 | sqlx 依存、責務混在 |

## 制約と pitfall

- **feature flag の正確な設計が重要**: `utoipa` は api crate のみで有効にし、action-runner では有効にしない。`cfg_attr` を間違えるとコンパイルエラーの切り分けが困難になる
- **API のフィールド追加/削除時の互換性**: `#[serde(default)]` や `Option<T>` を適切に使い、後方互換を維持する設計が必要
- **型のネーミング規約**: API 側の `PlanProjectInput` と action-runner 側の `PlanProject` は同じ型を指すため、統一した命名（Input/Output/Request/Response の接尾辞）を決めるべき
- **Docker バイナリサイズ**: `serde` + `serde_json` のみの依存なら action-runner バイナリへの影響は無視できるレベル（数十 KB 程度）
- **既存テスト（wiremock）の更新**: action-runner テストのモックレスポンスが新しい型に合わせて更新必要

## 未解決の疑問

- `PlanDecision` enum を api-types で定義する場合、action-runner がパース失敗した場合のフォールバック戦略（`#[serde(other)]` の活用?）
- `domain` crate の enum（`BoardRunStatus` 等）を api-types でも別途定義するか、feature flag で共有するか
- 将来的にフロントエンド向け OpenAPI コード生成（Issue #62）との整合をどう保つか

## 参照 URL

- [Rust workspace 公式ドキュメント](https://doc.rust-lang.org/cargo/reference/workspaces.html)
- [utoipa docs.rs — ToSchema derive](https://docs.rs/utoipa/latest/utoipa/derive.ToSchema.html)
- [progenitor GitHub](https://github.com/oxidecomputer/progenitor)
- [serde feature flags](https://serde.rs/feature-flags.html)
- [Adam Chalmers - Investigating crazy compile times（OpenAPI生成のコンパイル時間問題）](https://blog.adamchalmers.com/crazy-compile-time/)
- [ag-ui/utoipa feature flag パターン](https://github.com/ag-ui-protocol/ag-ui/issues/1407)
