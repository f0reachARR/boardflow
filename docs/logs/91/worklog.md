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

## 実装内容

TBD

## テスト結果

TBD

## レビュー結果

TBD

## ドキュメント確認

TBD

## PR/完了結果

TBD

## 残リスク

- スナップショットテストの保守コスト
- APIスキーマ変更時のワークフロー（意図的な変更 vs 意図しない変更の区別）
