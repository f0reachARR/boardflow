# Rust モジュール `pub use` 再エクスポート戦略

## 要約

Rust の `pub use` を使った再エクスポートは、内部モジュール構造と公開APIを分離するための標準的なパターン。コードを新モジュールに移動した後、元の場所から `pub use` で再エクスポートすると、既存の利用側コードを壊さずにリファクタリングできる。

## 確認した情報

### ベストプラクティス

1. **Facade パターン**: サブモジュールの詳細実装を隠蔽し、クレートルートや親モジュールから `pub use` で必要な型・関数だけを公開する
2. **段階的リファクタリング**: コードを新モジュールに移動し、旧モジュールから `pub use new_module::Item;` で再エクスポートすることで、利用側の `use` パスを変更せずに移行できる
3. **最小公開原則**: 内部実装の型は `pub(crate)` にとどめ、クレート外に公開する必要がある型だけ `pub use` する
4. **rust-lang/api-guidelines**: 公開依存のクレートは `pub extern crate` で再エクスポートすべき（今回は該当しない）

### パス構文

- `pub use crate::pagination::PaginationParams;` — 絶対パス（クレートルートから）
- `pub use self::pagination::encode_cursor;` — 相対パス（現在のモジュールから）

### BoardFlow への適用

今回のケースでは `crate::pagination` にコードを移動し、`pub(crate)` で公開すればクレート内のルートハンドラから直接参照できる。クレート外への公開は不要なので `pub use` による再エクスポートも不要。ただし将来 #99（read.rs 分割）で利便性のため `routes/mod.rs` から再エクスポートする可能性がある。

## BoardFlow への示唆

- `pagination.rs` を `crates/api/src/pagination.rs` に作成し、`lib.rs` で `pub mod pagination;` として宣言
- 関数・型は `pub(crate)` で公開（クレート外に公開する必要なし）
- `routes/read.rs` と `routes/api_token.rs` から `use crate::pagination::*;` でインポート
- 旧モジュールからの `pub use` 再エクスポートは不要（内部リファクタリングのため既存の外部APIに影響なし）

## 採用/不採用判断

**採用**: `pub mod pagination;` + `pub(crate)` 公開が最適。再エクスポートは現時点では不要。

## 制約と pitfall

- `pub use` で同名の型を複数モジュールから再エクスポートすると名前衝突が起きる
- `pub(crate)` にしておけばクレート外への意図しない公開を防げる
- テストコードからのアクセスには `pub(crate)` で十分

## 未解決の疑問

- なし

## 参照URL

- https://github.com/rust-lang/api-guidelines/discussions/176
- https://dev.to/sgchris/how-to-structure-a-rust-project-idiomatically-500k
- https://smithery.ai/skills/davincible/rust-architecture-patterns
- https://doc.rust-lang.org/reference/items/use-declarations.html
