# Issue #88 - action-runner APIクライアントの型定義をOpenAPIスキーマ由来の共有crateへ移行する調査

## 経緯

- ユーザーから「action-runnerでOpenAPI定義が活用されていない」という要望が提出された
- Issue #73（boardflow-action entrypoint Rust移行）の後続として、API型の整合性問題が顕在化

## ユーザー要望

- `crates/action-runner/` のAPI呼び出しがバックエンドAPIのOpenAPI定義と整合していない
- OpenAPIの型定義を活用してAPIクライアントを生成するか、共有の型定義を使うべき

## 調査結果（詳細）

### 現状の問題

1. **リクエスト構築が `serde_json::json!` マクロベース**: `runner.rs` L318-333（plan ペイロード）、L418-428（create_board_run ペイロード）で JSON を手動構築。フィールド名の typo や型の不一致がコンパイル時に検出されない
2. **レスポンス型の乖離**:
   - action-runner `ArtifactBundleInfo`: `upload_url`, `object_key` の 2 フィールドのみ
   - API 側 `ArtifactBundleInfo`: `upload_mode`, `object_key`, `upload_url`, `method`, `expires_at` の 5 フィールド
   - action-runner `CreateBoardRunResponse`: `artifact_bundle` が非 Option
   - API 側: `artifact_bundle: Option<ArtifactBundleInfo>`
3. **enum vs String**: API 側 `PlanDecision` は `Build | Skip | Error` enum だが、action-runner 側は `decision: String` で受信後に文字列比較
4. **dead code**: `PlanProject`, `PlanFile` が `#[allow(dead_code)]` 付き — `Serialize` のみで HTTP クライアントに渡されていない（json! マクロで手動構築しているため）
5. **`repository` フィールドの不一致**: Plan API は `github_repository_id` を必須とするが action-runner は送信していない

### ソースコード参照

| ファイル | 行 | 内容 |
|----------|-----|------|
| `crates/action-runner/src/api.rs` L18-37 | レスポンス型定義（`ProjectDecision`, `CreateBoardRunResponse`, `ArtifactBundleInfo`） |
| `crates/action-runner/src/api.rs` L192-208 | リクエスト型定義（`PlanProject`, `PlanFile` — dead code） |
| `crates/action-runner/src/runner.rs` L305-333 | plan_payload の json! マクロ構築 |
| `crates/action-runner/src/runner.rs` L418-428 | create_board_run の json! マクロ構築 |
| `crates/api/src/routes/plan.rs` L12-95 | Plan API リクエスト/レスポンス型（ToSchema 付き） |
| `crates/api/src/routes/board_run.rs` L48-96 | BoardRun API リクエスト/レスポンス型（ToSchema 付き） |

### 外部調査結果

#### progenitor（OpenAPI クライアント生成）
- Oxide Computer 社製。OpenAPI 3.0.x からクライアントを生成
- reqwest ベースの完全クライアントを生成するため、既存の ApiClient リトライ/タイムアウトロジックとの統合が困難
- proc macro 方式はコンパイル時間が大幅に増加（Adam Chalmers ブログで問題報告あり）
- 同一モノレポ内では over-engineering

#### utoipa feature flag パターン
- `cfg_attr(feature = "openapi", derive(utoipa::ToSchema))` で条件付きコンパイル可能
- ag-ui プロジェクトでも同様のパターンが issue で議論されている
- `serde` のみ必須、`utoipa` はオプショナル feature で提供可能

#### Rust workspace 共有 crate パターン
- Cargo workspace members 間で `path = "../api-types"` として依存を宣言
- 同一 Cargo.lock を共有するため、依存バージョンの不整合なし
- workspace レベルの依存管理で serde/utoipa のバージョンを統一

### アプローチ候補

- **A: 共有crate (`crates/api-types/`)**: 同一モノレポなので最も自然。DB/Webフレームワーク非依存の型定義crateを切り出す
- **B: OpenAPIコード生成 (`progenitor`等)**: 外部ツール依存、ビルド複雑化、バイナリサイズ増大
- **C: `domain` crate共有**: 既存だがsqlx依存あり、action-runnerには過剰

### 推奨アプローチ: A（共有 crate）

**理由:**
1. 同一 Cargo workspace 内なので型の直接共有が最もシンプル
2. serde のみ依存で Docker Action バイナリサイズへの影響なし
3. API 型変更時にコンパイルエラーとして即座に検出
4. `utoipa::ToSchema` は feature flag で API 側のみ有効化可能
5. 将来の CI 型整合性テスト（Issue #91）の基盤にもなる

**推奨 crate 構成:**
```
crates/api-types/
├── Cargo.toml          # serde 必須, utoipa optional
├── src/
│   ├── lib.rs
│   ├── plan.rs         # PlanRequest, PlanResponse, PlanDecision 等
│   └── board_run.rs    # CreateBoardRunRequest/Response, ImportRequest 等
```

## Issue作成内容

本Issue (#88) を調査Issueとして作成。実装Issueは #89, #90, #91 として分割:
- #89: `crates/api-types/` 作成
- #90: action-runner を api-types ベースにリファクタリング
- #91: CI 型整合性テスト追加

## 結論ステータス

`research_only`

## 参照 URL

- https://doc.rust-lang.org/cargo/reference/workspaces.html
- https://docs.rs/utoipa/latest/utoipa/derive.ToSchema.html
- https://github.com/oxidecomputer/progenitor
- https://blog.adamchalmers.com/crazy-compile-time/
- https://serde.rs/feature-flags.html
- https://github.com/ag-ui-protocol/ag-ui/issues/1407

## 残リスク

- `crates/domain/` の型とAPI型の関係性が複雑（domain→DB操作、api-types→API契約）
- `utoipa` feature flag の条件コンパイルが複雑になる可能性
- `PlanDecision` enum の後方互換（新 variant 追加時の `#[serde(other)]` 戦略）
- フロントエンド OpenAPI 型生成（Issue #62）との整合維持
