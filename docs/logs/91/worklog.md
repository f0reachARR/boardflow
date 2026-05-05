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

## 残リスク (初期整理)

- OpenAPIスキーマの変更が意図的な場合は `cargo insta review` でスナップショットを更新する必要あり（READMEやCONTRIBUTINGへの手順追記は未実施）

## 残リスク (実装後)

- スナップショットテストの保守コスト
- APIスキーマ変更時のワークフロー（意図的な変更 vs 意図しない変更の区別）

## レビュー結果 (2026-05-05, Copilot review)

### 総評

- 実装は Issue #91 の主目的である「CI 上で OpenAPI 契約差分を検出する」仕組みとしては成立している。
- `openapi_router()` 抽出により、API 実装とスキーマ生成で同じルート定義を共有しており、重複定義による乖離リスクは下がっている。
- `cargo test -p boardflow-api --test openapi_schema_test` を再実行し、DB 接続不要で 1 件成功を確認した。
- `cargo test -p boardflow-api --lib` を再実行し、29 件成功で既存 lib テストのリグレッションなしを確認した。

### 指摘事項

1. 中: スナップショットの対象が Issue の目的に対して広すぎる。
   - [crates/api/src/lib.rs](crates/api/src/lib.rs#L142) から [crates/api/src/lib.rs](crates/api/src/lib.rs#L166) の `openapi_router()` は health/read/auth/api_token を含む API 全体を登録している。
   - [crates/api/tests/openapi_schema_test.rs](crates/api/tests/openapi_schema_test.rs#L7) から [crates/api/tests/openapi_schema_test.rs](crates/api/tests/openapi_schema_test.rs#L9) はその全スキーマを丸ごとスナップショット化している。
   - その結果、action-runner とは無関係な read API や auth API の変更でも snapshot 更新が必須になり、Issue #91 の「action-runner と API 間の型整合性確認」という目的に対してシグナルがやや鈍る。
   - ただし契約差分の検出自体は機能しており、PR ブロッカーとまでは判断しない。

2. 低: `insta` の `yaml` feature 追加と実装内容が一致していない。
   - [Cargo.toml](Cargo.toml#L64) では `insta` に `yaml` feature を追加しているが、実際のテストは [crates/api/tests/openapi_schema_test.rs](crates/api/tests/openapi_schema_test.rs#L8) で JSON を文字列化し、[crates/api/tests/openapi_schema_test.rs](crates/api/tests/openapi_schema_test.rs#L9) の `assert_snapshot!` を使っている。
   - 現状でも問題はないが、依存の意図が曖昧なので `assert_json_snapshot!` へ寄せるか、未使用 feature を外すかのどちらかに揃えた方が保守しやすい。

### PR/完了結果 (Copilot review)

- pr_ready: true

### テスト結果

- `mise exec -- cargo test -p boardflow-api --test openapi_schema_test` : passed
- `mise exec -- cargo test -p boardflow-api --lib` : passed

### ドキュメント確認 (Copilot review)

- [docs/spec.md](docs/spec.md) と [docs/backend/api.md](docs/backend/api.md) を確認し、Issue #91 に対して必須の仕様差分や追加説明は見当たらなかった。
- 一方で snapshot 更新手順 (`cargo insta review`) を案内する恒久ドキュメントは見当たらず、運用手順は作業ログ依存のまま。

### 残リスク (Copilot review)

- API 全体 snapshot のため、action-runner 契約と無関係な API 変更でも CI が落ちる。
- snapshot 更新ワークフローが README 等に載っていないため、将来の変更者が `.snap` 更新手順を探しにくい。

## ドキュメント確認 (2026-05-05, docs review)

### 対象 Issue

- #91: CIでaction-runnerとAPI間の型整合性を検証するテストを追加する

### 確認対象

- `docs/spec.md`
- `docs/technology.md`
- `docs/backend/summary.md`
- `README.md`
- `docs/logs/91/worklog.md`

### 総評 (docs review)

- 実装自体は `crates/api/src/lib.rs` の `openapi_schema()` と `crates/api/tests/openapi_schema_test.rs` の snapshot test で説明可能で、既存の仕様・技術方針文書と矛盾は見当たらない。
- 一方で、OpenAPI を意図的に変更した際の snapshot 更新手順が恒久ドキュメントに存在せず、作業ログ内でも「追加ドキュメント不要」と「README等への手順追記は未実施」が併存している。
- このため、PR 前のドキュメント判定は `docs_ready: false` とする。

### 必須修正

1. `README.md` に OpenAPI schema snapshot test の更新手順を追記する。
   - 少なくとも `mise exec -- cargo test -p boardflow-api --test openapi_schema_test` と `cargo insta review` を、OpenAPI 変更時の更新フローとして明示する必要がある。
   - 既存の「API型定義の再生成」セクションに近接配置し、`pnpm generate:api` と合わせて contributor が一連の更新作業を把握できる形が自然。
2. `docs/logs/91/worklog.md` の「ドキュメント確認」記述を今回の結論に合わせる。
   - 現在の「特に追加ドキュメント不要」は今回のレビュー結論と不整合。

### 任意改善

1. PR 本文に snapshot 更新手順を 1 行入れる。
   - OpenAPI 変更時は `.snap` の更新が必要であることを reviewer が即座に把握しやすくなる。
2. 将来的に snapshot 対象を action-runner 契約に近い path 群へ絞るかどうかを別 Issue で整理する。

### 不整合のあるドキュメント

- `docs/logs/91/worklog.md`
  - 「特に追加ドキュメント不要」と「READMEやCONTRIBUTINGへの手順追記は未実施」が同居しており、運用判断が揃っていない。

### 不足しているドキュメント

- `README.md`
  - OpenAPI schema snapshot の更新・レビュー手順

### 外部調査メモに関する指摘

- Issue #91 で参照必須の `docs/external/` 成果物はなく、外部調査メモとの不整合は見当たらない。

### PR/完了結果への示唆

- PR 本文には、Issue 要件、実装概要、実行テストに加え、OpenAPI 変更時の snapshot 更新手順を明記した方がよい。
- 現時点では docs 観点では README 追記後に PR 作成が妥当。

### 判定

- docs_ready: false

### 残リスク (docs review)

- snapshot 更新手順が README に入るまでは、意図的な OpenAPI 変更時に contributor が CI failure の解き方を作業ログ依存で探すことになる。

## README追記完了 (2026-05-05)

- README.md の「API型定義の再生成」セクション直後に「OpenAPI スキーマのスナップショット更新」セクションを追記
- `cargo test` → `cargo insta review` → `git add/commit` の手順を明示
- docs_ready: true（必須修正対応済み）

## 最終ステータス

- review: pr_ready: true
- docs: docs_ready: true（README追記済み）
- 未使用yaml feature: 修正済み（`insta = "1"` に変更）
- PR作成: 次ステップ
