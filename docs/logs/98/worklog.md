# Issue #98: Pagination Cursor 共通化 — 作業ログ

## Issue

- **ID**: #98
- **タイトル**: pagination cursor処理を共通化する — read.rs内のcursor encode/decodeをpagination.rsに移動
- **種別**: 内部リファクタリング（挙動変更なし）

---

## 2026-05-14: リサーチフェーズ

### 経緯

Issue #98 は `crates/api/src/routes/read.rs` と `crates/api/src/routes/api_token.rs` に重複しているカーソル処理コードを `crates/api/src/pagination.rs` に共通化するリファクタリング。後続の Issue #99（read.rs 分割）がこの成果を前提とする可能性がある。

### ユーザー要望

- 既存 Issue に従ってリファクタリングを進める
- 挙動変更は避け、純粋なコード移動・抽出に留める

### 調査対象

1. **Rust モジュール構造のベストプラクティス（pub use 再エクスポート戦略）**
2. **base64 + serde_json による cursor エンコーディングパターンが一般的かどうか**

### 調査結果

#### 1. Rust `pub use` 再エクスポート

- `pub use` は内部モジュール構造と公開 API を分離する標準的なパターン
- 今回はクレート内部のリファクタリングなので `pub(crate)` で十分。`pub use` による再エクスポートは不要
- `crates/api/src/lib.rs` に `pub mod pagination;` を追加し、型・関数を `pub(crate)` で公開する方式が最適
- 詳細: `docs/external/rust-module-pub-use-reexport.md`

#### 2. base64 + JSON カーソルパターン

- base64url + JSON ペイロードを opaque cursor として使うのは業界標準（Relay Connection Spec, GitHub API v4, Stripe 等）
- BoardFlow の現在の実装（`URL_SAFE_NO_PAD` + `serde_json`）はこのパターンに完全に合致
- エンコーディング方式の変更は不要。リファクタリングは純粋にコードの重複排除に集中すべき
- 詳細: `docs/external/cursor-pagination-base64-pattern.md`

### 既存コードベースの確認結果

| ファイル | 重複内容 |
|---------|---------|
| `crates/api/src/routes/read.rs` | `CursorPayload`, `encode_cursor`, `decode_cursor`, `RepositoryCursorPayload`, `encode_repository_cursor`, `decode_repository_cursor`, `PaginationParams`, `PaginatedResponse<T>` |
| `crates/api/src/routes/api_token.rs` | `CursorPayload`, `encode_cursor`, `decode_cursor`, `ApiTokenPaginationParams`, `ApiTokenListResponse` |

- `crates/api/src/pagination.rs` はまだ存在しない
- DB層のクエリ関数はカーソルをタプル `(DateTime<Utc>, Uuid)` / `(DateTime<Utc>, i64)` で受け取る（変更不要）

### 計画（実装エージェントへの引き継ぎ）

1. `crates/api/src/pagination.rs` を新規作成
2. 以下を移動:
   - `CursorPayload`, `RepositoryCursorPayload` (構造体)
   - `encode_cursor`, `decode_cursor` (UUID ベース)
   - `encode_repository_cursor`, `decode_repository_cursor` (i64 ベース)
   - `PaginationParams` (クエリパラメータ)
   - `PaginatedResponse<T>` (レスポンス型)
3. `crates/api/src/lib.rs` に `pub mod pagination;` を追加
4. `read.rs` から重複コードを削除し `use crate::pagination::*;` に置換
5. `api_token.rs` から重複コードを削除:
   - `CursorPayload`, `encode_cursor`, `decode_cursor` を削除
   - `ApiTokenPaginationParams` → 共通 `PaginationParams` に統合可能か検討
   - `ApiTokenListResponse` → `PaginatedResponse<ApiTokenListItem>` に置換可能か検討
6. テスト実行で挙動変更がないことを確認

### 後続エージェントへの注意点

- `PaginatedResponse<T>` の `#[derive(ToSchema)]` が utoipa のジェネリクスで問題になる可能性あり → 実装時に確認
- `ApiTokenListResponse` と `PaginatedResponse<T>` のフィールドが同一か確認の上、統合の可否を判断
- `run_check_finding.rs` には `(i32, Uuid)` 型カーソルがあるが、Issue #98 のスコープ外
- Issue #99（read.rs 分割）との順序依存に注意

### 参照URL

- https://github.com/rust-lang/api-guidelines/discussions/176
- https://stackoverflow.com/questions/28389893/why-is-it-a-common-practice-to-encode-pagination-cursors-or-id-values-as-string

### 結論ステータス

**`implementation_required`** — 外部ライブラリの変更や新規導入は不要。現在の base64 + serde_json パターンは業界標準に合致しており、純粋な内部コード移動・重複排除として実装に進むべき。

### 残リスク

- `utoipa` の `ToSchema` derive がジェネリック型 `PaginatedResponse<T>` で正しく動作するか（実装時に検証）
- OpenAPI スナップショットの差分が生じる可能性（`cargo insta review` で確認が必要）

---

## 2026-05-14: 計画フェーズ

### ユーザー判断結果

1. **FindingsCursor のスコープ**: 含める（pagination.rs に移動して一元化する）
2. **ApiTokenListResponse の統合**: 置換する（PaginatedResponse<ApiTokenListItem> に統合。OpenAPI差分は許容）

### 実装計画

#### 目的

- `read.rs` と `api_token.rs` に重複しているカーソル encode/decode 処理を `pagination.rs` に共通化する
- `ApiTokenListResponse` を `PaginatedResponse<ApiTokenListItem>` に統合して型の重複を解消する
- `FindingsCursorPayload` 系も `pagination.rs` に移動して cursor 処理を一箇所に集約する

#### 非目的

- 挙動の変更（レスポンス値、エンコーディング方式の変更）
- DB 層 (`crates/db`) の変更
- `run_check_finding.rs` のクエリ変更
- テスト以外の新規ロジック追加

#### 受け入れ条件

1. `crates/api/src/pagination.rs` が存在し、全 cursor encode/decode 関数を含む
2. `read.rs` と `api_token.rs` から cursor 関連の重複コードが除去されている
3. `ApiTokenListResponse` が削除され、`PaginatedResponse<ApiTokenListItem>` に置換されている
4. `cargo test --workspace` が通る（OpenAPI スナップショットは `cargo insta review` で更新）
5. `cargo clippy --workspace --all-targets -- -D warnings` がクリーン
6. `cargo fmt --all -- --check` がクリーン
7. フロントエンドの `pnpm generate:api` で `schema.d.ts` が再生成可能

#### 詳細要件

##### Step 1: `crates/api/src/pagination.rs` 新規作成

移動する要素（すべて `pub(crate)`）:

| 要素 | 元ファイル | 可視性 |
|------|-----------|--------|
| `CursorPayload` | read.rs, api_token.rs | 非公開（モジュール内 private） |
| `RepositoryCursorPayload` | read.rs | 非公開（モジュール内 private） |
| `FindingsCursorPayload` | read.rs | 非公開（モジュール内 private） |
| `encode_cursor()` | read.rs, api_token.rs | `pub(crate)` |
| `decode_cursor()` | read.rs, api_token.rs | `pub(crate)` |
| `encode_repository_cursor()` | read.rs | `pub(crate)` |
| `decode_repository_cursor()` | read.rs | `pub(crate)` |
| `encode_findings_cursor()` | read.rs | `pub(crate)` |
| `decode_findings_cursor()` | read.rs | `pub(crate)` |
| `PaginationParams` | read.rs | `pub(crate)` struct, `pub` フィールド (utoipa IntoParams) |
| `PaginatedResponse<T>` | read.rs | `pub` struct (OpenAPI schema 公開) |

依存クレート（pagination.rs の use）:
- `base64`, `chrono`, `serde`, `serde_json`, `utoipa`, `uuid`
- `crate::error::AppError` (PaginationParams のメソッドで使用)

##### Step 2: `crates/api/src/lib.rs` にモジュール登録

```rust
pub mod pagination;
```

`artifact_token` の次（アルファベット順）に追加。

##### Step 3: `crates/api/src/routes/read.rs` の変更

- 削除: `CursorPayload`, `RepositoryCursorPayload`, `FindingsCursorPayload` 構造体
- 削除: `encode_cursor`, `decode_cursor`, `encode_repository_cursor`, `decode_repository_cursor`, `encode_findings_cursor`, `decode_findings_cursor` 関数
- 削除: `PaginationParams` 構造体とその impl
- 削除: `PaginatedResponse<T>` 構造体
- 追加: `use crate::pagination::{...};` （使用する要素を列挙）
- 不要になる use: `base64::Engine`, `base64::engine::general_purpose::URL_SAFE_NO_PAD`（read.rs で他に使っていないか確認）
- `FindingsQueryParams` は read.rs に残す（`severity` フィールドが固有のため）

##### Step 4: `crates/api/src/routes/api_token.rs` の変更

- 削除: `CursorPayload` 構造体
- 削除: `encode_cursor`, `decode_cursor` 関数
- 削除: `ApiTokenPaginationParams` 構造体 → `PaginationParams` に置換
- 削除: `ApiTokenListResponse` 構造体 → `PaginatedResponse<ApiTokenListItem>` に置換
- 追加: `use crate::pagination::{PaginationParams, PaginatedResponse, encode_cursor};`
- `list_api_tokens()` 関数内のインライン limit/cursor 処理を `PaginationParams` のメソッド呼び出しに置換:
  - `params.limit.unwrap_or(50).clamp(1, 100)` → `params.effective_limit()`
  - インライン decode → `params.decoded_cursor(&request_id)?`
- utoipa マクロの `params(ApiTokenPaginationParams)` → `params(PaginationParams)`
- utoipa マクロの `body = ApiTokenListResponse` → `body = PaginatedResponse<ApiTokenListItem>`
- 戻り値型の `Json<ApiTokenListResponse>` → `Json<PaginatedResponse<ApiTokenListItem>>`
- 構築箇所の `ApiTokenListResponse { ... }` → `PaginatedResponse { ... }`
- 不要になる use: `base64::Engine`, `base64::engine::general_purpose::URL_SAFE_NO_PAD`

##### Step 5: テスト・スナップショット更新

1. `cargo test -p boardflow-api` → OpenAPI スナップショット差分検出
2. `cargo insta review` → 差分確認・承認
   - `ApiTokenListResponse` → `PaginatedResponse_ApiTokenListItem` への名前変更
3. `cargo test --workspace` → 全テスト通過確認
4. `cargo clippy --workspace --all-targets -- -D warnings`
5. `cargo fmt --all -- --check`

##### Step 6: フロントエンド型再生成

1. `cd boardflow && pnpm generate:api` で `schema.d.ts` を再生成
2. `pnpm typecheck` で型エラーがないことを確認

#### 変更の順序（依存関係考慮）

1. **pagination.rs 作成** — 他ファイルに依存しない新規ファイル
2. **lib.rs にモジュール登録** — pagination.rs が存在する前提
3. **read.rs の変更** — pagination.rs からインポートに切り替え
4. **api_token.rs の変更** — pagination.rs からインポートに切り替え + 型統合
5. **コンパイル・テスト** — cargo test, clippy, fmt
6. **スナップショット更新** — cargo insta review
7. **フロントエンド再生成** — pnpm generate:api + typecheck

#### 影響範囲

| ファイル | 変更種別 |
|---------|---------|
| `crates/api/src/pagination.rs` | **新規作成** |
| `crates/api/src/lib.rs` | 変更（1行追加） |
| `crates/api/src/routes/read.rs` | 変更（コード削除 + import 追加） |
| `crates/api/src/routes/api_token.rs` | 変更（重複削除 + 型統合 + import 追加） |
| `crates/api/tests/snapshots/openapi_schema_test__openapi_schema.snap` | 変更（スナップショット更新） |
| `boardflow/src/lib/api/schema.d.ts` | 変更（再生成） |

#### 設計方針

- **可視性**: Cursor Payload 構造体は `pagination.rs` 内 private。encode/decode 関数と PaginationParams/PaginatedResponse は `pub(crate)`。PaginatedResponse は OpenAPI に出るため `pub`。
- **インポート方式**: `use crate::pagination::{具体的な名前};` でワイルドカードは使わない
- **PaginationParams のメソッド**: `decoded_findings_cursor()` メソッドを追加し、FindingsQueryParams での cursor デコードも簡潔にする（ただし FindingsQueryParams 自体は read.rs に残す）

#### テスト観点

1. **コンパイル**: `cargo build -p boardflow-api` が通ること
2. **既存テスト**: `cargo test -p boardflow-api` が通ること（スナップショット更新後）
3. **ワークスペース全体**: `cargo test --workspace` が通ること
4. **Lint**: `cargo clippy --workspace --all-targets -- -D warnings` がクリーン
5. **Format**: `cargo fmt --all -- --check` がクリーン
6. **OpenAPI スナップショット**: 差分が `ApiTokenListResponse` → `PaginatedResponse_ApiTokenListItem` の名前変更のみであること
7. **フロントエンド型チェック**: `pnpm typecheck` がクリーン
8. **pagination.rs 単体テスト**: encode → decode のラウンドトリップテストを追加（推奨、ただし必須ではない。元コードにもテストがないため、挙動変更なしの移動として最小限でもOK）

#### ドキュメント更新対象

- `docs/logs/98/worklog.md` — 本計画・実装結果を追記
- その他のドキュメント更新は不要（内部リファクタリングのため）

#### 実装要否

**`implementation_required`**

#### 未解決の疑問

- **PaginationParams に decoded_findings_cursor() を追加するか**: FindingsQueryParams が severity フィールドを持つため PaginationParams とは構造が異なるが、cursor デコードのヘルパーメソッドだけ PaginationParams に追加するか、standalone 関数のまま呼ぶか。→ standalone 関数として `decode_findings_cursor()` を `pub(crate)` で公開すれば十分。PaginationParams にメソッド追加は不要（FindingsQueryParams は PaginationParams を使わないため）。

#### リスクと注意点

1. **OpenAPI スキーマ名変更**: `ApiTokenListResponse` → `PaginatedResponse_ApiTokenListItem` はフロントエンドの生成型に影響するが、直接参照はなく `pnpm generate:api` で解決
2. **utoipa ジェネリクス**: `PaginatedResponse<T>` の `#[derive(ToSchema)]` は既に read.rs で動作実績あり。api_token.rs での `PaginatedResponse<ApiTokenListItem>` も同じパターンで問題なし
3. **Issue #99 との依存**: #99（read.rs 分割）はこの成果を前提とするため、#98 が先にマージされる必要がある

### 作業ログパス

`docs/logs/98/worklog.md`

---

## 2026-05-14: 実装フェーズ

### 実装内容

計画通り、pagination cursor 処理の共通化を実施。

#### 作成ファイル

| ファイル | 内容 |
|---------|------|
| `crates/api/src/pagination.rs` | cursor encode/decode 関数、PaginationParams、PaginatedResponse<T> を集約した新モジュール |

#### 変更ファイル

| ファイル | 変更内容 |
|---------|---------|
| `crates/api/src/lib.rs` | `pub mod pagination;` を追加（1行） |
| `crates/api/src/routes/read.rs` | CursorPayload, RepositoryCursorPayload, FindingsCursorPayload, encode/decode 関数6個、PaginationParams, PaginatedResponse を削除。`use crate::pagination::{...}` に置換。不要な `base64` import を削除 |
| `crates/api/src/routes/api_token.rs` | CursorPayload, encode_cursor, decode_cursor, ApiTokenPaginationParams, ApiTokenListResponse を削除。`use crate::pagination::{PaginatedResponse, PaginationParams, encode_cursor}` に置換。`list_api_tokens()` のインライン limit/cursor 処理を `PaginationParams` メソッドに置換。utoipa アノテーション更新 |
| `crates/api/tests/snapshots/openapi_schema_test__openapi_schema.snap` | `ApiTokenListResponse` → `PaginatedResponse_ApiTokenListItem` に自動更新 |

#### pagination.rs に移動した要素

- `CursorPayload` (private) — UUID ベース cursor
- `RepositoryCursorPayload` (private) — i64 ベース cursor
- `FindingsCursorPayload` (private) — i32+UUID ベース cursor
- `encode_cursor()` / `decode_cursor()` — pub(crate)
- `encode_repository_cursor()` / `decode_repository_cursor()` — pub(crate)
- `encode_findings_cursor()` / `decode_findings_cursor()` — pub(crate)
- `PaginationParams` — pub (axum Query extractor として必要)
  - `effective_limit()`, `decoded_cursor()`, `decoded_repository_cursor()` メソッド
- `PaginatedResponse<T>` — pub (OpenAPI schema 公開)

### テスト結果

#### 新規追加テスト（24件）— pagination.rs 内

| 観点 | テスト名 | 保証内容 |
|------|---------|---------|
| cursor roundtrip | `test_encode_decode_cursor_roundtrip` | UUID cursor の encode→decode が同値を返す |
| cursor 異常系 | `test_decode_cursor_invalid_base64` | 不正 base64 で None |
| cursor 異常系 | `test_decode_cursor_invalid_json` | 不正 JSON で None |
| cursor 異常系 | `test_decode_cursor_invalid_uuid` | 不正 UUID で None |
| cursor 異常系 | `test_decode_cursor_invalid_timestamp` | 不正 timestamp で None |
| cursor 境界値 | `test_decode_cursor_empty_string` | 空文字列で None |
| repo cursor roundtrip | `test_encode_decode_repository_cursor_roundtrip` | repository cursor の encode→decode が同値を返す |
| repo cursor 異常系 | `test_decode_repository_cursor_invalid_base64` | 不正 base64 で None |
| repo cursor 異常系 | `test_decode_repository_cursor_invalid_gid` | 不正 gid で None |
| findings cursor roundtrip | `test_encode_decode_findings_cursor_roundtrip` | findings cursor の encode→decode が同値を返す |
| findings cursor 異常系 | `test_decode_findings_cursor_invalid_base64` | 不正 base64 で None |
| findings cursor 異常系 | `test_decode_findings_cursor_invalid_uuid` | 不正 UUID で None |
| PaginationParams | `test_effective_limit_default` | None → 50 |
| PaginationParams | `test_effective_limit_clamped_min` | 0 → 1 |
| PaginationParams | `test_effective_limit_clamped_max` | 200 → 100 |
| PaginationParams | `test_effective_limit_normal` | 25 → 25 |
| PaginationParams | `test_decoded_cursor_none` | cursor None → Ok(None) |
| PaginationParams | `test_decoded_cursor_valid` | 有効 cursor → Ok(Some(...)) |
| PaginationParams | `test_decoded_cursor_invalid_returns_error` | 不正 cursor → AppError |
| PaginationParams | `test_decoded_repository_cursor_none` | cursor None → Ok(None) |
| PaginationParams | `test_decoded_repository_cursor_valid` | 有効 cursor → Ok(Some(...)) |
| PaginationParams | `test_decoded_repository_cursor_invalid_returns_error` | 不正 cursor → AppError |
| cursor 互換性 | `test_uuid_cursor_rejected_by_repository_decoder` | UUID cursor を repository decoder に渡すと None |
| cursor 互換性 | `test_repository_cursor_rejected_by_uuid_decoder` | repository cursor を UUID decoder に渡すと None |

#### 既存テスト結果

- `cargo test --workspace --exclude boardflow-config -- --skip test_app_config_from_env`: **全テスト成功**（kicad の `export_pcb_pdf_rejects_empty_output_file` 失敗は main でも再現する既存不具合）
- `cargo fmt --all -- --check`: **クリーン**
- `cargo clippy --workspace --all-targets -- -D warnings`: **クリーン**
- `cargo insta accept`: OpenAPI スナップショット更新済み

### OpenAPI スナップショットの変更

- `ApiTokenListResponse` スキーマが削除され、`PaginatedResponse_ApiTokenListItem` に置換
- 構造は同一（items, next_cursor, has_more）で、items 内の ApiTokenListItem がインライン展開される形式に変更
- API のレスポンス値自体に変化なし

### 残リスク

1. **フロントエンド型再生成**: `pnpm generate:api` で `schema.d.ts` の再生成が必要。`ApiTokenListResponse` を直接参照している箇所があれば型名変更が必要
2. **kicad テスト既存不具合**: `export_pcb_pdf_rejects_empty_output_file` が main でも失敗 — 本 Issue とは無関係
3. **config_test 環境依存**: `test_app_config_from_env` は DATABASE_URL 設定時に失敗 — 本 Issue とは無関係

---

## 2026-05-14: レビューフェーズ

### レビュー結果

- **総評**: cursor encode/decode と pagination 共通型の抽出自体は、親コミットとの差分比較でも実質的に純粋移動に留まっている。`list_api_tokens()` の cursor 算出、`read.rs` の repository / board project / board run / findings の pagination 挙動にもレビュー上の回帰は見当たらない。
- **PR 判定**: `pr_ready: false`

### 必須修正

1. **フロントエンド生成型が未更新**
   - OpenAPI スナップショットは `PaginatedResponse_ApiTokenListItem` へ更新済みだが、`boardflow/src/lib/api/schema.d.ts` にはまだ `ApiTokenListResponse` が残っている。
   - この状態では API スキーマと生成型が不一致で、後続の frontend 実装や #99 以降の参照で古い型名に依存し続ける。
   - 対応: `cd boardflow && pnpm generate:api` を実行し、必要なら `pnpm typecheck` まで確認する。

### 任意改善

1. **可視性方針の明文化と整合**
   - 実装では `crates/api/src/lib.rs` で `pub mod pagination;` としており、`PaginationParams` / `PaginatedResponse<T>` も public になっている。
   - 一方で research / plan には `pub(crate)` で十分という記述が残っているため、コードと記録の整合が取れていない。
   - 現状コードが直ちに誤りとは言えないが、外部公開を意図していないなら visibility 方針を明確化したほうが後続 Issue の判断がぶれない。
   - **意図の明記**: `pub mod pagination` は `boardflow-api` クレート自体が binary + lib 構成であり、integration test (`tests/*.rs`) から型を参照するために `pub` が必要。`pub(crate)` では `tests/` ディレクトリからアクセスできないため、`pub` は意図的な選択である。研究メモの `pub(crate) で十分` は誤りであり、実装が正しい。

---

## 2026-05-14: レビュー指摘修正フェーズ

### 修正内容

1. **フロントエンド生成型の更新** (`boardflow/src/lib/api/schema.d.ts`)
   - `ApiTokenListResponse` (型定義 + 参照) → `PaginatedResponse_ApiTokenListItem` に手動で更新
   - APIサーバー起動なしに手動修正（`pnpm generate:api` はサーバー依存のため）

### テスト結果

- `pnpm typecheck`: **PASS** — 型チェック成功、型不一致エラーなし

### 更新ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `boardflow/src/lib/api/schema.d.ts` | `ApiTokenListResponse` → `PaginatedResponse_ApiTokenListItem` に型名変更（定義 + 参照の2箇所） |
| `docs/logs/98/worklog.md` | レビュー指摘修正結果の追記、pub 可視性の意図を明記 |

### 残課題

- APIサーバー起動後に `pnpm generate:api` で正式に再生成し、手動修正と差分がないことを確認すべき

### テスト評価

- `pagination.rs` の単体テスト追加は十分で、正常系・異常系・decoder 取り違えまで押さえている。
- 既存 API integration test も cursor pagination 系が揃っており、抽出後の挙動確認として妥当。
- ただし frontend 生成型更新後の `pnpm typecheck` 実行証跡は未確認。

### ドキュメント確認

- `docs/backend/api.md` の cursor pagination / token list 仕様と、今回のレスポンス構造・opaque cursor 方針は整合している。
- OpenAPI スナップショットの schema 名変更は utoipa の generic schema 展開として自然で、構造変化は見当たらない。
- `docs/logs/98/worklog.md` には「frontend 再生成が必要」という残リスクが記録されているが、実ファイル更新は未了。

### 残リスク

1. `schema.d.ts` が未再生成のままマージされると、API 定義とフロントエンド型定義が乖離した状態になる。
2. `pagination` モジュールの公開範囲について、コードと計画のどちらを正とするかが未確定。

### PR/完了結果

- 現時点では `pr_ready: false`
- `pnpm generate:api` 実施と生成差分確認後に再レビュー不要で PR 化可能
