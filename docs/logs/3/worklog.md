# Issue #3: 認証基盤とAPI Token

## 経緯
- バックエンド実装Issue分割タスクの一環として作成
- 全Action APIの横断的関心事（認証、エラー形式）

## ユーザー要望
- docs/以下の仕様に基づくRustバックエンド実装の第3段階

## Issue作成内容
- Bearer token認証、統一エラーレスポンス、リクエストID
- URL: https://github.com/f0reachARR/boardflow/issues/3

## 調査結果
- Axum 0.8: `FromRequestParts` traitでカスタムextractor実装
- SHA-256: `sha2` crate v0.10使用
- Request ID: UUID v7 (`uuid::Uuid::now_v7()`) + `req_` プレフィックス
- utoipa 5: `Modify` traitで`SecurityScheme`をOpenAPI docに追加
- token hash照合: constant-time比較不要（SHA-256ハッシュ比較のため timing attack リスク実質なし）

## 計画
1. DB query関数 (find_by_hash, update_last_used_at)
2. 統一エラーレスポンス (ErrorCode, AppError, ErrorResponse)
3. Request ID middleware (UUID v7, x-request-id header)
4. Bearer token認証 extractor (AuthenticatedToken)
5. utoipa SecurityScheme定義
6. テスト

## 実装内容
### 新規ファイル
- `crates/db/src/queries/mod.rs` - クエリモジュール
- `crates/db/src/queries/api_token.rs` - find_by_hash, update_last_used_at
- `crates/api/src/error.rs` - RequestId, ErrorCode, ErrorBody, ErrorResponse, AppError
- `crates/api/src/middleware/mod.rs` - ミドルウェアモジュール
- `crates/api/src/middleware/request_id.rs` - request_id_middleware
- `crates/api/src/extractors/mod.rs` - エクストラクタモジュール
- `crates/api/src/extractors/auth.rs` - AuthenticatedToken extractor
- `crates/api/tests/auth_test.rs` - 8テスト

### 変更ファイル
- `Cargo.toml` (workspace) - sha2, tower-http, tower, http追加
- `crates/api/Cargo.toml` - sha2, uuid, chrono追加
- `crates/db/Cargo.toml` - boardflow-domain, uuid, chrono追加
- `crates/db/src/lib.rs` - `pub mod queries;` 追加
- `crates/api/src/lib.rs` - modules追加, middleware layer, SecurityAddon

### ドキュメント更新
- `docs/backend/api.md` - revoke済みtokenのエラーコードを「認可エラー」から「認証エラー(unauthorized)」に修正

## テスト結果
- 全11テスト通過 (auth_test 8件 + config_test 1件 + integration_test 2件)
- `cargo check --workspace` 通過

## レビュー結果
- 1回目: `pr_ready: false` (request_id フォーマット不一致 + 未使用依存)
- 修正後: `pr_ready: true`

## ドキュメント確認
- `docs_ready: true` (revoke済みtokenのエラーコード表現を修正)

## 残リスク
- DB統合テスト: auth extractorのDB結合テストは別Issue追加推奨
- Bearer case-insensitive: RFC 7235ではscheme名はcase-insensitiveだが、GitHub Actions固定のためMVPでは問題なし

## 実装完了: 2026-04-30

### 実装内容

#### 依存関係追加
- Workspace: `sha2`, `tower-http`, `tower`, `http`
- api crate: `sha2`, `uuid`, `chrono`, `thiserror`, `tower-http`, `tower`, `http`
- db crate: `boardflow-domain`, `uuid`, `chrono`

#### 新規ファイル
- `crates/db/src/queries/mod.rs`, `crates/db/src/queries/api_token.rs`: DBクエリ層
- `crates/api/src/error.rs`: 統一エラーレスポンス (ErrorCode, AppError, ErrorResponse)
- `crates/api/src/middleware/request_id.rs`: UUID v7 request_id ミドルウェア
- `crates/api/src/extractors/auth.rs`: Bearer token認証エクストラクタ (SHA-256ハッシュ照合)
- `crates/api/tests/auth_test.rs`: ユニット/統合テスト (8件)

#### 変更ファイル
- `crates/db/src/lib.rs`: `pub mod queries;` 追加
- `crates/api/src/lib.rs`: middleware レイヤー追加、OpenAPI SecurityScheme 追加

### テスト結果
8 passed; 0 failed (request_id, エラーコードマッピング、エラーレスポンス形式)
