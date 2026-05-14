# Cursor Pagination の base64 + JSON エンコーディングパターン

## 要約

base64 エンコードされた JSON ペイロードを opaque cursor として使うパターンは、cursor-based pagination における業界標準的な手法。Facebook の Relay Connection Specification に由来し、REST API でも広く採用されている。BoardFlow の現在の実装（base64 URL_SAFE_NO_PAD + serde_json）はこのパターンに合致しており、変更の必要はない。

## 確認した情報

### パターンの概要

1. **カーソルの構成**: ソートキー（タイムスタンプ等）+ tie-breaker（ID等）を JSON にシリアライズし、base64url でエンコード
2. **Opaque 原則**: クライアントはカーソルの内部構造を知る必要がない。base64 エンコードにより実装詳細を隠蔽
3. **URL Safe**: `base64url`（`+/` の代わりに `-_` を使用）によりクエリパラメータに安全に埋め込み可能

### 業界での採用例

- **Relay Connection Specification** (GraphQL): カーソルを base64 エンコードされた opaque string として定義
- **Stripe API**: cursor-based pagination に opaque token を使用
- **GitHub API v4**: GraphQL の Connection パターンで base64 カーソルを使用
- **OpenTalk (Rust)**: `opentalk_types_api_v1::pagination::Cursor<T>` として base64 + postcard でエンコード
- 多くの Express.js / Node.js チュートリアルが `Buffer.from(JSON.stringify(payload)).toString('base64url')` パターンを推奨

### セキュリティ上の注意

- カーソルに機密データを含めないこと（ID とソートフィールド値のみ）
- デコード後のバリデーションは必須（ユーザー入力と同様に扱う）
- 改ざん防止が必要な場合は HMAC 署名を追加（現時点の BoardFlow では不要）

### BoardFlow の現在の実装との比較

| 側面 | 業界標準 | BoardFlow 現状 |
|------|---------|---------------|
| エンコーディング | base64url + JSON | base64 URL_SAFE_NO_PAD + serde_json ✅ |
| ペイロード | ソートキー + tie-breaker | timestamp + UUID/i64 ✅ |
| Opaque 性 | クライアントは構造を知らない | ✅ |
| バリデーション | デコード失敗時は None/400 | `Option` で処理 ✅ |
| limit + 1 フェッチ | `has_more` 判定に推奨 | 実装済み ✅ |

## BoardFlow への示唆

- 現在の実装は業界標準パターンに完全に合致しており、エンコーディング方式の変更は不要
- リファクタリングでは純粋にコードの重複排除と移動に集中すべき
- 将来的に `(i32, Uuid)` 型カーソル（`run_check_finding` で使用）もジェネリックに扱えるようにすると拡張性が向上するが、Issue #98 のスコープ外

## 採用/不採用判断

**現状維持（採用済み）**: base64 + serde_json パターンは業界標準であり、BoardFlow の既存実装は適切。

## 制約と pitfall

- base64 は暗号化ではないため、ユーザーがデコードして内部構造を推測可能（現状は ID とタイムスタンプのみなのでリスク低）
- カーソルペイロードのスキーマを変更すると、古いカーソルが無効になる（デコード失敗 → `None` で安全に処理される）
- `serde_json::to_string().unwrap()` は Serialize 実装がある限り panic しないが、理論上は `expect` でメッセージを付けるのが親切

## 未解決の疑問

- なし

## 参照URL

- https://stackoverflow.com/questions/28389893/why-is-it-a-common-practice-to-encode-pagination-cursors-or-id-values-as-string
- https://www.getknit.dev/blog/api-pagination-best-practices
- https://medium.com/@sohail_saifi/api-pagination-cursor-vs-offset-vs-keyset-pagination-5f9a6e864ba4
- https://docs.rs/opentalk-types-api-v1/latest/opentalk_types_api_v1/pagination/
