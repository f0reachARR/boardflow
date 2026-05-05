# Issue #91 - CIでaction-runnerとAPI間の型整合性を検証するテストを追加する

## 経緯

- #89 (api-types crateの分離) と #90 (action-runnerのapi-types利用) が完了済み
- api-types crateでAPI/action-runner間の型が共有されているが、OpenAPIスキーマレベルでの整合性は未検証

## ユーザー要望

- 型の整合性が継続的に保証される仕組みの構築
- APIサーバ起動なしで実行可能なテスト
- 既存CIに追加する形

## 調査結果 (2026-05-05)

- `crates/api/src/lib.rs` の `ApiDoc::openapi()` でスキーマ取得可能（サーバ起動不要）
- `insta` は未導入 → 今回導入
- CI (`cargo test --workspace`) で自動実行される
- action-runner は `boardflow-api-types` を使って API と型を共有
- OpenAPI スキーマのスナップショットを取れば、型変更時に差分検出可能

## 計画 (2026-05-05)

1. workspace Cargo.toml に `insta` を追加（features: yaml）
2. crates/api/Cargo.toml に dev-dependency として `insta` を追加
3. crates/api/tests/openapi_schema_test.rs を作成
   - `ApiDoc::openapi()` を呼び出してスキーマJSON取得
   - `insta::assert_snapshot!` でスナップショット保存
4. スナップショットファイルを生成・コミット
5. CIでは既存の `cargo test --workspace` で自動検証される（追加設定不要）

## 実装内容 (2026-05-05)

1. **workspace Cargo.toml**: `insta = { version = "1", features = ["yaml"] }` 追加
2. **crates/api/Cargo.toml**: `[dev-dependencies]` に `insta = { workspace = true }` 追加
3. **crates/api/src/lib.rs**:
   - `pub fn openapi_schema()` 追加（全ルート登録済みの完全なOpenAPIスキーマを返す）
   - `fn openapi_router() -> OpenApiRouter<PgPool>` を抽出し、`create_app_with_config` と `openapi_schema` でルート定義を共有（重複排除）
4. **crates/api/tests/openapi_schema_test.rs**: スナップショットテスト作成
5. **crates/api/tests/snapshots/openapi_schema_test__openapi_schema.snap**: 3064行のスナップショット（全paths + components含む）

## テスト結果 (2026-05-05)

- `cargo test -p boardflow-api --test openapi_schema_test` → 1 passed
- `cargo test -p boardflow-api --lib` → 29 passed（既存テスト全パス）
- `cargo clippy -p boardflow-api --all-targets -- -D warnings` → 警告なし

## レビュー結果

- ルート登録の重複を `openapi_router()` 関数で解消済み
- DBやサーバ起動不要で実行可能

## ドキュメント確認

- 特に追加ドキュメント不要（CIに設定変更なし、`cargo test --workspace` で自動検証される）

## PR/完了結果

- ブランチ: `feat/91-openapi-schema-snapshot-test`
- コミット: `feat(api): add OpenAPI schema snapshot test (#91)`

## 残リスク

- OpenAPIスキーマの変更が意図的な場合は `cargo insta review` でスナップショットを更新する必要あり（READMEやCONTRIBUTINGへの手順追記は未実施）

## 残リスク

- スナップショットテストの保守コスト
- APIスキーマ変更時のワークフロー（意図的な変更 vs 意図しない変更の区別）
