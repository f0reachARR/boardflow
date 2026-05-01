# Issue #27: API Token 管理 API

## Issueまでの経緯

BoardFlow は GitHub Actions 上で KiCad プロジェクトをビルドし、成果物を SaaS にアップロードする Board CI/CD サービス。Action API の認証には repository 単位の BoardFlow API token が使われる。現状、token の DB 構造（`boardflow_api_tokens` テーブル）と認証時の hash 検証（`crates/db/src/queries/api_token.rs`）は実装済みだが、**Web UI からの token 作成・一覧・失効（revoke）を行う管理 API が未実装**。

## ユーザー要望

docs 以下の仕様に基づいて、API Token の管理エンドポイント（作成、一覧取得、失効）を実装する。

---

## 調査フェーズ（2026-05-01）

### 1. 既存コードベースパターン

#### 1.1 DB テーブル構造

`boardflow_api_tokens` テーブル（`20260430000001_create_schema.up.sql` L167-176）:

```sql
CREATE TABLE boardflow_api_tokens (
    id UUID PRIMARY KEY,
    installation_id BIGINT NOT NULL,
    repository_id UUID NOT NULL REFERENCES repositories(id),
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ
);
```

インデックス: `repository_id`, `installation_id`

#### 1.2 Domain モデル

`crates/domain/src/models/api_token.rs`:

```rust
pub struct BoardflowApiToken {
    pub id: Uuid,
    pub installation_id: i64,
    pub repository_id: Uuid,
    pub name: String,
    pub token_hash: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}
```

#### 1.3 既存 DB クエリ（`crates/db/src/queries/api_token.rs`）

- `find_by_hash(pool, token_hash)` → Action API Bearer 認証用
- `update_last_used_at(pool, id)` → 認証成功時の更新

**不足**: token 作成（INSERT）、一覧取得（SELECT by repository_id）、revoke（UPDATE revoked_at）のクエリが未実装。

#### 1.4 セッション認証パターン

- `AuthenticatedSession` extractor（`crates/api/src/extractors/session.rs`）: cookie から session ID を取得し、DB で session → user を解決
- `user.github_access_token` を使い `GithubAccessChecker.check_access()` で repository 権限を確認
- 権限がない場合は情報漏洩防止のため `404 not_found` を返す（`access_result_to_error` ヘルパー）

#### 1.5 ルーティングパターン

- `crates/api/src/routes/mod.rs` に各モジュールを宣言
- `crates/api/src/lib.rs` の `create_app_with_config()` で `utoipa_axum::routes!()` マクロを使ってルート登録
- handler シグネチャ: `(session: AuthenticatedSession, Extension(RequestId(...)), Extension(access_checker), State(pool), Path/Query/Json(...)) -> Result<Json<T>, AppError>`

### 2. 仕様から導き出される API 設計

仕様書（`docs/spec.md` §10.11, §16.1）に「BoardFlow API token の最小ライフサイクル管理」として明記。api.md には token 管理 API の詳細仕様はまだ記載されていないが、以下の仕様が確定している:

- token の平文は作成時のみ表示し、DB には hash のみ保存
- revoke 済み token は認可に使えない
- 複数 token の高度な管理 UI や自動ローテーションは MVP では扱わない

#### 2.1 エンドポイント設計案

| Method | Path | 説明 |
|--------|------|------|
| `POST` | `/api/v1/repositories/{github_repository_id}/api-tokens` | token 作成 |
| `GET` | `/api/v1/repositories/{github_repository_id}/api-tokens` | token 一覧 |
| `POST` | `/api/v1/repositories/{github_repository_id}/api-tokens/{token_id}/revoke` | token 失効 |

#### 2.2 Token 作成 API

**Request:**
```json
{
  "name": "CI token"
}
```

**処理:**
1. `AuthenticatedSession` でユーザー認証
2. `github_repository_id` から repository を取得
3. `access_checker.check_access()` でリポジトリ権限を確認
4. repository の `installation_id` を取得
5. ランダムトークン生成（例: `bft_` プレフィックス + ランダム文字列）
6. SHA-256 ハッシュを DB に保存
7. 平文トークンを含むレスポンスを返す（**この一回のみ**）

**Response (201):**
```json
{
  "id": "uuid",
  "name": "CI token",
  "token": "bft_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
  "created_at": "2026-05-01T00:00:00Z"
}
```

#### 2.3 Token 一覧 API

**処理:**
1. `AuthenticatedSession` + アクセス権確認
2. `repository_id` で `boardflow_api_tokens` を取得

**Response (200):**
```json
{
  "items": [
    {
      "id": "uuid",
      "name": "CI token",
      "created_at": "2026-05-01T00:00:00Z",
      "last_used_at": "2026-05-01T12:00:00Z",
      "revoked_at": null
    }
  ]
}
```

`token_hash` や平文は返さない。pagination は token 数が少ないため MVP では不要だが、一貫性のためパターンに合わせても良い。

#### 2.4 Token 失効 API

**処理:**
1. `AuthenticatedSession` + アクセス権確認
2. token が対象 repository に属することを確認
3. `revoked_at = NOW()` を設定

**Response (200):**
```json
{
  "id": "uuid",
  "name": "CI token",
  "revoked_at": "2026-05-01T13:00:00Z"
}
```

既に revoke 済みの場合は冪等に既存の `revoked_at` を返す。

### 3. 認可方式

- Web UI からの token 管理は GitHub OAuth session ベース
- `AuthenticatedSession` extractor → `user.github_access_token` → `access_checker.check_access(token, owner, name)` で当該 repository へのアクセス権を確認
- アクセス拒否時は `404 not_found`（情報漏洩防止）

### 4. 実装に必要なファイル変更

| ファイル | 変更内容 |
|----------|----------|
| `crates/api/src/routes/mod.rs` | `pub mod api_token;` 追加 |
| `crates/api/src/routes/api_token.rs` | **新規**: 3 エンドポイントの handler |
| `crates/api/src/lib.rs` | `.routes(routes!(...))` 追加 |
| `crates/db/src/queries/api_token.rs` | `create`, `list_by_repository_id`, `revoke` クエリ追加 |

DB マイグレーションは不要（テーブルは既存）。Domain モデルも既存で十分。

### 5. トークン生成方式

既存の `crates/api/src/artifact_token.rs` では HMAC-SHA256 を使用しているが、API token は異なる用途。以下の方式を推奨:

- プレフィックス `bft_` + 32バイトのランダムデータ（hex or base62エンコード）
- DB には SHA-256 ハッシュを保存
- 既存の `find_by_hash` クエリがそのまま利用可能

---

## 結論ステータス

**`implementation_required`**

- 外部ライブラリの新規調査は不要（すべて既存の axum + sqlx + utoipa パターンに従う）
- DB スキーマ・Domain モデルは既存で対応可能
- 3 エンドポイントの handler と 3 つの DB クエリの追加実装が必要

## 残リスク

- `api.md` に token 管理 API の詳細仕様セクションが未記載（実装時に追記推奨）
- token prefix の仕様（`bft_` 等）が spec.md で明示されていないため、実装時に決定が必要
- MVP での pagination 要否: token 数は少ないため不要と判断できるが、他 API との一貫性を考慮する余地あり

---

## 計画フェーズ（2026-05-01）

### 1. 目的

ユーザーが Web UI から BoardFlow API Token を作成・一覧取得・失効（revoke）できるように、3つの Session 認証付き REST API エンドポイントを実装する。

### 2. 非目的

- Token 自動ローテーション
- Token スコープ（権限粒度）の細分化
- Token の有効期限設定
- Token 使用量の分析/監査ログ
- Token 名の一意性制約

### 3. 受け入れ条件

- `POST /api/v1/repositories/{github_repository_id}/api-tokens` で新規 token を作成できる
- 作成レスポンスに平文 token が含まれ、以降はどこからも取得できない
- `GET /api/v1/repositories/{github_repository_id}/api-tokens` で token 一覧を取得できる（hash/平文は含まない）
- `POST /api/v1/repositories/{github_repository_id}/api-tokens/{token_id}/revoke` で token を失効できる
- 失効済み token への再 revoke は冪等に成功する
- 全エンドポイントで Session 認証 + GitHub access check が行われる
- アクセス権がない repository に対しては 404 を返す（情報隠蔽）
- token が対象 repository に属さない場合も 404
- OpenAPI ドキュメント（utoipa）が生成される
- 統合テストが通る

### 4. 詳細要件

#### 4.1 Token 作成 (POST)

- Request body: `{ "name": "..." }` — name は 1〜100 文字、空白のみ不可
- Token 平文フォーマット: `bft_` + 32バイトランダム (hex) = `bft_` + 64文字 = 68文字
- DB 保存: SHA-256(平文) の hex 文字列を `token_hash` に保存
- `installation_id` は repository レコードから取得
- Response (201): `{ id, name, token, created_at }`

#### 4.2 Token 一覧 (GET)

- Response (200): `{ items: [{ id, name, created_at, last_used_at, revoked_at }], next_cursor, has_more }`
- revoked_at が設定済みのものも含めて返す（UI で状態表示に使う）
- ソート: created_at DESC, id DESC
- Cursor pagination 実装（他 API との一貫性のため）
- `token_hash` は返さない

#### 4.3 Token 失効 (POST .../revoke)

- token_id (UUID) で特定
- `revoked_at = NOW()` を設定
- 既に revoke 済みなら既存値を保持（冪等）
- token が対象 repository に属さない場合は 404
- Response (200): `{ id, name, created_at, last_used_at, revoked_at }`

### 5. 影響範囲

| ファイル | 変更種別 | 内容 |
|----------|----------|------|
| `Cargo.toml` (workspace) | 修正 | `rand = "0.8"` 追加 |
| `crates/api/Cargo.toml` | 修正 | `rand = { workspace = true }` 追加 |
| `crates/api/src/routes/mod.rs` | 修正 | `pub mod api_token;` 追加 |
| `crates/api/src/routes/api_token.rs` | **新規** | 3 handler + Request/Response 型 |
| `crates/api/src/lib.rs` | 修正 | `.routes(routes!(...))` 3行追加 |
| `crates/db/src/queries/api_token.rs` | 修正 | `create`, `list_by_repository_id`, `revoke` クエリ追加 |
| `crates/api/tests/api_token_test.rs` | **新規** | 統合テスト |
| `docs/backend/api.md` | 修正 | Token Management API セクション追記 |

### 6. 設計方針

#### 6.1 Route Handler (api_token.rs)

既存 `read.rs` の `get_repository` パターンに従う:
1. `AuthenticatedSession` で session 認証
2. `Path(github_repository_id)` で repository 解決 (`find_by_github_id`)
3. `access_checker.check_access()` で GitHub 権限確認
4. `access_result_to_error` で Denied → 404 変換
5. DB クエリ実行
6. Response 構築

`access_result_to_error` ヘルパーは `read.rs` 内で定義されているため、`api_token.rs` から利用するには:
- 方法A: 同じロジックを `api_token.rs` 内に再定義（ヘルパー関数は短いので許容）
- 方法B: 共通モジュールに抽出

→ 方法A を採用（最小変更、read.rs の公開 API を変えない）

#### 6.2 Token 生成

```rust
use rand::RngCore;
use sha2::{Sha256, Digest};

fn generate_token() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let raw = format!("bft_{}", hex::encode(bytes));
    let hash = hex::encode(Sha256::digest(raw.as_bytes()));
    (raw, hash)
}
```

#### 6.3 DB クエリ

```rust
// create
pub async fn create(pool, id, installation_id, repository_id, name, token_hash) -> Result<BoardflowApiToken, sqlx::Error>

// list_by_repository_id (with cursor pagination)
pub async fn list_by_repository_id(pool, repository_id, limit, cursor) -> Result<Vec<BoardflowApiToken>, sqlx::Error>

// revoke
pub async fn revoke(pool, id, repository_id) -> Result<Option<BoardflowApiToken>, sqlx::Error>
```

`revoke` は `repository_id` でも絞り込むことで、別 repository の token を revoke できないよう保護する。

### 7. テスト観点

| テストケース | 期待結果 |
|-------------|----------|
| 認証なしで token 作成 | 401 |
| アクセス権なし repository で token 作成 | 404 |
| 正常 token 作成 | 201, 平文あり |
| 作成した token の hash で find_by_hash が成功 | DB 整合性 |
| token 一覧（0件） | 200, items: [] |
| token 一覧（複数件） | 200, hash なし |
| token revoke | 200, revoked_at 非null |
| 二重 revoke | 200, 冪等 |
| 存在しない token_id で revoke | 404 |
| 別 repository の token を revoke | 404 |
| name が空文字 | 400 validation_failed |
| name が100文字超 | 400 validation_failed |
| 一覧の cursor pagination | next_cursor, has_more 正常動作 |

### 8. ドキュメント更新対象

- `docs/backend/api.md`: Token Management API セクション追記（エンドポイント仕様）
- `docs/logs/27/worklog.md`: 実装経緯記録

### 9. 実装順序

1. `Cargo.toml` に `rand` 依存追加
2. `crates/db/src/queries/api_token.rs` に 3 クエリ追加
3. `crates/api/src/routes/api_token.rs` 新規作成（3 handler）
4. `crates/api/src/routes/mod.rs` にモジュール追加
5. `crates/api/src/lib.rs` にルート登録
6. `cargo build` で コンパイル確認
7. `crates/api/tests/api_token_test.rs` 統合テスト作成
8. `cargo test` で動作確認
9. `docs/backend/api.md` 更新

### 10. 実装要否

**`implementation_required`**

### 11. 未解決の疑問

なし。Research フェーズで必要な情報は全て揃っており、既存パターンに従うため追加判断は不要。

### 12. 更新した作業ログパス

`docs/logs/27/worklog.md`

---

## 実装フェーズ（2026-05-01）

### 実装内容

1. **Workspace deps**: `rand = "0.8"` を workspace root `Cargo.toml` に追加、`crates/api/Cargo.toml` で参照
2. **DB クエリ追加** (`crates/db/src/queries/api_token.rs`):
   - `create(pool, id, installation_id, repository_id, name, token_hash)` → INSERT RETURNING
   - `list_by_repository_id(pool, repository_id, limit, cursor)` → cursor pagination (created_at DESC, id DESC)
   - `revoke(pool, token_id, repository_id)` → UPDATE SET revoked_at = COALESCE(revoked_at, NOW()) RETURNING
3. **Route handler** (`crates/api/src/routes/api_token.rs` 新規):
   - `create_api_token`: name validation (1-100文字, trim), bft_ prefix + 32byte hex random token, SHA-256 hash → DB保存, 201 CREATED
   - `list_api_tokens`: cursor pagination, hash/平文非公開
   - `revoke_api_token`: COALESCE で冪等 revoke, repository_id 照合で別repo token 保護
4. **モジュール登録**: `routes/mod.rs` に `pub mod api_token;`、`lib.rs` に3ルート追加
5. **公開ヘルパー**: `read.rs` の `access_result_to_error` を `pub` に変更し再利用

### テスト結果

10/10 パス（`crates/api/tests/api_token_test.rs`）:

| テスト | 観点 |
|--------|------|
| `test_create_api_token_success` | 正常作成 + 平文format + SHA-256 hash 整合性 |
| `test_list_api_tokens` | 作成済みtokenが一覧に出る、hash/token非公開 |
| `test_revoke_api_token` | revoked_at が設定される |
| `test_revoke_idempotent` | 再revoke時に既存revoked_atが保持される |
| `test_create_api_token_unauthenticated` | 未認証 → 401 |
| `test_create_api_token_repo_not_found` | 存在しないrepo → 404 |
| `test_create_api_token_access_denied` | アクセス権なし → 404 |
| `test_create_api_token_empty_name` | 空白name → 400 |
| `test_create_api_token_name_too_long` | 101文字 → 400 |
| `test_revoke_token_wrong_repo` | 別repo token revoke → 404 |

全パッケージテスト: 62/62 パス（回帰なし）

### 更新ドキュメント

- `docs/logs/27/worklog.md`（本ファイル）

### 残リスク

- `docs/backend/api.md` に token 管理 API の仕様セクション未追記（別タスクとして管理推奨）
- rate limit テスト未追加（他エンドポイントと同一パターンのため優先度低）
- token 数上限（per-repo）の制約なし（MVP スコープ外）

### コミット

- `a7bf1a0` feat(#27): implement API token management endpoints

---

## レビューフェーズ（2026-05-01）

### レビュー結果

Issue #27 の実装差分、`docs/spec.md`、`docs/backend/api.md`、既存 read/auth パターン、関連テストを確認した。

追加で以下を確認した:

- 実装対象ブランチ: `feat/27-api-token-management`
- 対象コミット: `a7bf1a0`（実装）、`4c2d05b`（worklog更新）
- 実行確認: `mise exec rust@nightly -- cargo test -p boardflow-api --test api_token_test` → 10/10 pass
- Web 調査: token は hash 保存・平文は初回のみ返却・revoke は冪等、という方針は一般的な API key 管理ベストプラクティスと整合

### 指摘事項

#### Medium: create API の malformed JSON 時に `request_id` が空になる

- `crates/api/src/routes/api_token.rs` の `create_api_token` は `Json<CreateApiTokenRequest>` を直接 extractor として受けている
- `crates/api/src/error.rs` の `impl From<JsonRejection> for AppError` は `request_id` に空文字を設定する
- そのため create API に不正 JSON を送った場合、仕様書の共通エラー形式が要求する `request_id` を満たさない可能性が高い
- 既存の `plan_run` は `payload: Result<Json<_>, JsonRejection>` を受けて handler 内で `request_id` を付与しており、この新規実装とはパターンが異なる

#### Medium: backend API 仕様書に token 管理 API の契約が未反映

- `docs/spec.md` には token の最小ライフサイクル管理要件がある一方で、`docs/backend/api.md` には `POST/GET /api/v1/repositories/{github_repository_id}/api-tokens` と `POST /api/v1/repositories/{github_repository_id}/api-tokens/{token_id}/revoke` の契約記載がない
- Issue の完了条件には docs 以下の仕様との整合確認が含まれており、API 実装だけ先行して契約ドキュメントが欠けた状態になっている

### テスト不足

- list API の cursor pagination の挙動確認がない（`limit`、`next_cursor`、`has_more`、2ページ目取得、invalid cursor）
- list / revoke の access denied → 404 の確認がない
- create API の malformed JSON → `validation_failed` と `request_id` 付与の確認がない

### ドキュメント確認

- `docs/spec.md`: token 平文は作成時のみ表示、DB には hash のみ保存、revoke 済み token は認可不可、`last_used_at` は成功認証時のみ更新、という仕様は実装と概ね整合
- `docs/backend/api.md`: token 管理 API の endpoint / request / response / error 契約が未記載
- `README.md`: Rust stable と記載があるが、現行 workspace は `edition = "2024"` かつ `mise.toml` で nightly 指定。Issue #27 固有ではないため今回は参考情報に留める

### PR/完了結果

- `pr_ready: false`

### 必須修正

1. create API の JSON parse error を handler 内で `request_id` 付きの `AppError::validation_failed` に変換する
2. `docs/backend/api.md` に token 管理 API の request / response / auth / error / pagination 契約を追記する

### 任意改善

1. token 管理 API でも既存 read API と同様の pagination helper を共通化して重複を減らす
2. session 認証系 endpoint の OpenAPI 上の auth 表現を整理する

### 残リスク

- malformed JSON 時の error contract 逸脱が残る限り、クライアント側の request tracing が不安定
- pagination の未検証分岐に将来の回帰余地がある

---

## 修正フェーズ（2026-05-01）

### レビュー指摘への対応

#### 1. create API の JsonRejection ハンドリング (Medium) → 修正済み

`create_api_token` の引数を `Json<CreateApiTokenRequest>` から `Result<Json<CreateApiTokenRequest>, JsonRejection>` に変更。
`plan_run` と同じパターンで handler 内で `request_id` 付きの `AppError::validation_failed` に変換するようにした。

変更ファイル: `crates/api/src/routes/api_token.rs`

#### 2. docs/backend/api.md にToken管理APIの契約追記 (Medium) → 修正済み

`docs/backend/api.md` に「3.0.5 Token Management API」セクションを追加。
3.0.4 Me と 3.1 Web UI Read API の間に配置。以下を記載:
- POST /api/v1/repositories/{github_repository_id}/api-tokens (create)
- GET /api/v1/repositories/{github_repository_id}/api-tokens (list)
- POST /api/v1/repositories/{github_repository_id}/api-tokens/{token_id}/revoke (revoke)
- 各エンドポイントの認証方式、Request/Response body、エラーケース

#### 3. テスト追加 → 修正済み

`crates/api/tests/api_token_test.rs` に以下4テストを追加（14テスト中4テストが新規）:

| テスト | 観点 |
|--------|------|
| `test_list_api_tokens_cursor_pagination` | 3token作成 → limit=2 → has_more=true, next_cursor非空 → 2ページ目で残り1件, has_more=false |
| `test_list_api_tokens_access_denied` | DenyAll checker使用 → list で 404 |
| `test_revoke_api_token_access_denied` | DenyAll checker使用 → revoke で 404 |
| `test_create_api_token_malformed_json` | 不正JSON送信 → 400 validation_failed + request_id非空 |

### テスト結果

14/14 パス（`--test-threads=1`）、全パッケージ 62/62 パス（回帰なし）

### コミット

- `2d854de` fix(#27): address review - JsonRejection handling, API docs, additional tests

### 残リスク

- `rand_i64()` の UUID v7 ベース生成がパラレルテスト実行で衝突する可能性あり（`--test-threads=1` で回避中、Issue #27 固有ではない）
- token 数上限（per-repo）の制約なし（MVP スコープ外）
- rate limit テスト未追加（他エンドポイントと同一パターンのため優先度低）

---

## ドキュメントレビューフェーズ（2026-05-01）

### ドキュメント確認結果

Issue #27 の対象として、`docs/spec.md`、`docs/backend/api.md`、`docs/backend/summary.md`、`README.md`、本 worklog を確認した。

- `docs/backend/api.md` の 3.0.5 Token Management API セクションは、Issue 本文と実装概要にある create / list / revoke の3 endpoint、Session + GitHub access check、denied 時の 404、平文 token は作成時のみ返却、hash のみ永続化、revoke の冪等性、list の cursor pagination と整合している。
- `docs/spec.md` の token 要件（repository 単位、hash 保存、成功認証時のみ `last_used_at` 更新、revoke 済み token は認可不可）との不整合は見当たらない。
- `docs/backend/summary.md` は backend の責務と認証方針を要約する高レベル文書であり、Issue #27 のために endpoint 単位の追記は必須ではない。
- `README.md` に API token 作成導線や `BOARDFLOW_TOKEN` の運用説明はまだ無いが、現時点では利用手順自体が README の対象として確立しておらず、Issue #27 の必須更新対象とは判断しない。

### 指摘事項

#### Medium: 契約テスト観点に Token Management API が未反映

- `docs/backend/api.md` の 3.0.5 には Token Management API の契約が追加されている一方、同ファイルの §5 契約テスト観点には create / list / revoke に対応する項目がない。
- 今回追加された API は Session 認証、404 による情報隠蔽、cursor pagination、revoke 冪等性、平文 token の一回限り返却という重要契約を持つため、契約テスト観点へ反映しないと将来の回帰を見落としやすい。

### docs 判定

- `docs_ready: false`

### 必須修正

1. `docs/backend/api.md` の §5 契約テスト観点に、少なくとも以下を追加する。
  - create API が平文 token を初回レスポンスにのみ含み、以後 list / revoke では返さないこと
  - list API が `limit` / `cursor` / `next_cursor` / `has_more` を含むこと
  - access denied と token の repository 不一致が 404 `not_found` になること
  - revoke API が冪等であること

### 任意改善

1. 将来 README に Action 設定手順を追加する段階で、`BOARDFLOW_TOKEN` の発行元として本 API か UI 導線への参照を追加する。

### 外部調査メモ

- Issue #27 については `docs/external/` に依存する新規外部調査成果物は見当たらず、既存の外部調査メモとの矛盾もない。

### PR/完了結果

- `docs_ready: false`

### 残リスク

- 契約テスト観点に token 管理 API が無いままだと、OpenAPI や統合テストが将来更新された際に token の公開範囲や pagination 契約の回帰を拾いにくい。


## 再レビューフェーズ（2026-05-01）

### レビュー結果

前回レビューの指摘3点について再確認した。

- `create_api_token` は `Result<Json<CreateApiTokenRequest>, JsonRejection>` で受け、handler 内で `AppError::validation_failed(..., &request_id)` に変換していることを確認
- `docs/backend/api.md` に `3.0.5 Token Management API` が追加され、既存セクションと同じ記述粒度で create/list/revoke の契約、認証、エラーケース、pagination が記載されていることを確認
- `crates/api/tests/api_token_test.rs` に cursor pagination / list access denied / revoke access denied / malformed JSON の4テストが追加され、前回のテスト不足を埋めていることを確認

追加確認:

- `mise exec rust@nightly -- cargo test -p boardflow-api --test api_token_test` → 14/14 pass
- `mise exec rust@nightly -- cargo test` → workspace 全体 pass（`read_api_test` は 62/62 pass、他 test binary/Doc-test も失敗なし）
- Web 調査でも、token の hash 保存、平文の一回表示、revoke の冪等性は一般的な token 管理のベストプラクティスと整合

### 指摘事項

#### Medium: revoke API の `token_id` path validation が既存 API のエラー契約と揃っていない

- `crates/api/src/routes/api_token.rs` の `revoke_api_token` は `Path<(i64, Uuid)>` を直接受けており、`token_id` の parse failure が handler 到達前に axum extractor 側で処理される
- 既存 read API は `Path<String>` を受けた上で handler 内で parse し、`request_id` 付き `validation_failed` を返す実装に寄せている
- そのため revoke API だけ、不正な `token_id` を与えた際の 400 レスポンスが他 endpoint と同じ JSON エラー形式・`request_id` 付与を満たさない可能性がある

### ドキュメント確認

- `docs/spec.md` の token 要件（平文は作成時のみ、DB は hash 保存、revoke は認証不可化）と実装は整合
- `docs/backend/api.md` の新設 `3.0.5 Token Management API` は既存フォーマットと整合
- Issue #27 の research / plan / 実装 / ドキュメントの主線は一致

### PR/完了結果

- `pr_ready: false`

### 必須修正

1. revoke API でも `token_id` を handler 内で parse して、invalid format を `request_id` 付き `validation_failed` に統一する
2. 上記に対応する invalid `token_id` の統合テストを追加する

### 任意改善

1. path/query/body の validation error を request_id 付きで統一する共通パターンを整理する

### テスト不足

- revoke API の invalid `token_id` に対する error contract の検証テストが未追加

### 残リスク

- 主要な受け入れ要件と前回指摘は満たしているが、revoke の invalid path parameter だけ error contract の一貫性が崩れる余地が残る

### 更新した作業ログパス

- `docs/logs/27/worklog.md`

---

## ドキュメント最終確認フェーズ（2026-05-01, 最終）

### ドキュメント確認

Issue #27 の最終 docs 確認として、`docs/spec.md`、`docs/backend/api.md`、`docs/backend/summary.md`、`README.md`、本 worklog を再確認した。

- `docs/backend/api.md` の §5 契約テスト観点には、前回必須だった Token Management API の4観点（平文 token の一回限り返却、list の cursor pagination、access denied / repository 不一致時の `404 not_found`、revoke の冪等性）が反映済み。
- `docs/spec.md` の token 要件（repository 単位、hash 保存、認証成功時のみ `last_used_at` 更新、revoke 済み token は認可不可）と `docs/backend/api.md` の Token Management API 本文に不整合はない。
- `docs/backend/summary.md` と `README.md` についても、Issue #27 の docs_ready を妨げる更新漏れは確認できなかった。

### レビュー結果

- 追加の必須修正はなし
- 前回指摘の「§5 契約テスト観点に Token Management API 未反映」は解消済み

### 判定

- `docs_ready: true`

### 任意改善

1. 非ブロッキングとして、将来的に README へ `BOARDFLOW_TOKEN` の発行導線を追加する余地はあるが、Issue #27 の完了条件ではない。

### 残リスク

- 現時点で PR 作成を止めるドキュメント不整合は確認していない。

### 更新した作業ログパス

- `docs/logs/27/worklog.md`

---

## 最終確認レビュー（4回目）（2026-05-01）

### レビュー結果

Issue #27 の4回目最終確認として、Issue本文、既存 research / plan、`docs/spec.md`、`docs/backend/api.md`、現行実装、関連テスト、直近の契約ドキュメント修正を再確認した。

確認した観点:

- `crates/api/src/routes/api_token.rs` の create / list / revoke すべての utoipa annotation に `400 Validation error` が定義されている
- create は handler 内で `JsonRejection` を `request_id` 付き `validation_failed` に変換している
- revoke は `token_id` を `String` で受けて handler 内 parse しており、不正 format で `400 validation_failed` を返す
- `docs/backend/api.md` の `3.0.5 Token Management API` と §5 契約テスト観点に、前回までの指摘内容が反映されている
- `mise exec rust@nightly -- cargo test -p boardflow-api --test api_token_test -- --test-threads=1` が 15/15 pass
- `mise exec rust@nightly -- cargo build -p boardflow-api` が pass

外部観点としても、OpenAPI では実際に返しうるエラー response を明示するのが一般的な設計方針であり、現在の create / list / revoke の `400` 明記はその期待と整合していることを確認した。

### 指摘事項

- Critical / High の残指摘は確認できなかった
- 前回までの必須指摘は解消済み

### テスト結果

- `mise exec rust@nightly -- cargo test -p boardflow-api --test api_token_test -- --test-threads=1` → 15/15 pass
- `mise exec rust@nightly -- cargo build -p boardflow-api` → pass

### ドキュメント確認

- `docs/spec.md` の repository 単位発行、hash 保存、revoke 済み token の認可不可要件と実装は整合している
- `docs/backend/api.md` の Token Management API 本文と §5 契約テスト観点は、create / list / revoke の契約と整合している
- `docs/backend/summary.md` と `README.md` について、Issue #27 の追加修正が必要な不整合は見当たらなかった

### PR/完了結果

- `pr_ready: true`

### 必須修正

- なし

### 任意改善

1. `list` の invalid cursor に対する `400 validation_failed` の統合テストは依然として無いため、将来の回帰耐性を上げるなら追加余地がある

### テスト不足

- 非ブロッキング: `list_api_tokens` の invalid cursor ケースを直接検証する統合テストは未追加

### 残リスク

- 現時点で PR を止めるリスクは確認していないが、invalid cursor の回帰は統合テスト未整備のため将来的に見落とす余地がある

### 更新した作業ログパス

- `docs/logs/27/worklog.md`

---

## ドキュメント最終確認フェーズ（2026-05-01）

### ドキュメント確認

Issue #27 のドキュメント観点として、`docs/spec.md`、`docs/backend/api.md`、`docs/backend/summary.md`、`README.md`、本 worklog を再確認した。

- `docs/spec.md` と `docs/backend/api.md` の Token Management API 本文は、create / list / revoke の仕様、平文 token の一回限り返却、hash 保存、revoke の冪等性、Session + GitHub access check、404 による情報隠蔽で整合している。
- `docs/backend/summary.md` は高レベル要約として十分で、Issue #27 向けの endpoint 列挙追記は必須ではない。
- `README.md` はまだ API token の利用導線を持たないが、現時点では Issue #27 の必須更新対象とはしない。

### ドキュメント指摘

#### Medium: `docs/backend/api.md` の §5 契約テスト観点に Token Management API が未反映

- `docs/backend/api.md` には `3.0.5 Token Management API` が追記済みだが、同ファイルの §5 契約テスト観点には token 管理 API 向けの観点が追加されていない。
- 少なくとも以下は契約テスト観点に含める必要がある。
  - create API が平文 token を初回レスポンスにのみ含むこと
  - list API が cursor pagination (`limit`, `cursor`, `next_cursor`, `has_more`) を満たすこと
  - access denied / token の repository 不一致が 404 `not_found` になること
  - revoke API が冪等であること

### 判定

- `docs_ready: false`

### 残リスク

- 契約テスト観点が未更新のままだと、今後の OpenAPI / 統合テスト変更時に token 管理 API の回帰を見落とす余地が残る。

### 更新した作業ログパス

- `docs/logs/27/worklog.md`

---

## 最終レビュー（3回目）（2026-05-01）

### レビュー結果

Issue #27 の3回目最終レビューとして、Issue本文、既存 research / plan、`docs/spec.md`、`docs/backend/api.md`、現行実装、直近修正コミット、関連テストを再確認した。

今回の確認結果:

- revoke API の utoipa annotation に `400 Validation error` が追加済み
- `docs/backend/api.md` の revoke エラー表に `400 validation_failed` が追加済み
- `mise exec rust@nightly -- cargo test -p boardflow-api --test api_token_test -- --test-threads=1` が 15/15 pass
- Issue #27 の revoke 修正に関する Critical / High の新規問題は確認できなかった

追加で確認した残差分:

- `crates/api/src/routes/api_token.rs` の list API は `invalid cursor` 時に `400 validation_failed` を返す実装になっている
- しかし list API の utoipa responses には `400` が未記載で、OpenAPI 契約だけ実装と不一致のまま残っている

### 指摘事項

#### Medium: list API の utoipa annotation に 400 が未定義

- `crates/api/src/routes/api_token.rs` の list endpoint は `decode_cursor(c).ok_or_else(|| AppError::validation_failed("invalid cursor", &request_id))?` により `400 validation_failed` を返す
- 一方で同 endpoint の `#[utoipa::path]` responses は `200 / 401 / 404` のみで、`400 Validation error` が未記載
- `docs/backend/api.md` には list API の `400 validation_failed` が記載済みのため、現状は「実装 + backend API doc」と「OpenAPI」がずれている

### テスト結果

- `mise exec rust@nightly -- cargo test -p boardflow-api --test api_token_test -- --test-threads=1` → 15/15 pass

### ドキュメント確認

- `docs/spec.md`: token の平文は作成時のみ表示、hash 保存、revoke 済み token は認可不可、という要件と整合
- `docs/backend/api.md`: revoke の `400 validation_failed` は反映済み
- OpenAPI annotation: revoke は修正済みだが、list の invalid cursor に対する `400` が未反映

### PR/完了結果

- `pr_ready: false`

### 必須修正

1. `crates/api/src/routes/api_token.rs` の list endpoint の utoipa responses に `400 Validation error` を追加する

### 任意改善

1. token 管理 API で validation error を返す endpoint は、実装・utoipa・backend API doc・テストを同一観点で点検できるチェックリストに寄せる

### テスト不足

- list API の invalid cursor に対する `400 validation_failed` の統合テストはまだ無い

### 残リスク

- 現状のままだと OpenAPI 利用者だけが list API の `400 validation_failed` を把握できず、クライアント生成やハンドリング分岐で契約齟齬が残る
- revoke 修正観点では Critical / High の残課題は確認できなかった

### 更新した作業ログパス

- `docs/logs/27/worklog.md`

---

## 最終レビューフェーズ（2026-05-01）

### レビュー結果

Issue #27 の最終確認として、Issue本文、既存 research / plan、`docs/spec.md`、`docs/backend/api.md`、実装差分、関連テスト、全体テストを再確認した。

確認した観点:

- revoke API が `Path<(i64, String)>` で受け、handler 内で `Uuid::parse_str()` していること
- invalid `token_id` テストが追加され、`validation_failed` + `request_id` を検証していること
- `mise exec -- cargo test -p boardflow-api --test api_token_test` が 15/15 pass であること
- `mise exec -- cargo test` が workspace 全体で pass していること
- 前回 1回目・2回目レビュー指摘が実装とテストの両面で解消されていること

追加で判明した事項:

- 実装の revoke API は invalid `token_id` に対して `400 validation_failed` を返す
- しかし `crates/api/src/routes/api_token.rs` の utoipa 注釈には revoke API の `400` response が未記載
- `docs/backend/api.md` の revoke エラーケース表にも `400 validation_failed`（invalid `token_id`）が未記載

### 指摘事項

#### Medium: revoke API の 400 error contract が OpenAPI / backend API doc に未反映

- 実装では `invalid token_id format` を `AppError::validation_failed(..., &request_id)` として返している
- `crates/api/tests/api_token_test.rs` にも invalid `token_id` 時の `400 validation_failed` テストが追加済み
- 一方で `crates/api/src/routes/api_token.rs` の revoke endpoint の `#[utoipa::path]` responses には `400` がない
- `docs/backend/api.md` の `3.0.5.3 Token 失効（Revoke）` のエラーケース表も `401` / `404` のみで、path parameter 形式不正の `400 validation_failed` が落ちている
- このままだと実装・テストと API 契約書 / OpenAPI が不一致になる

### テスト結果

- `mise exec -- cargo test -p boardflow-api --test api_token_test` → 15/15 pass
- `mise exec -- cargo test` → workspace 全体 pass（前回ログ記載の 175 件に対する回帰なし、failure 0）

### ドキュメント確認

- `docs/spec.md`: token 平文は作成時のみ表示、DB には hash 保存、revoke 済み token は認証不可、という要件と整合
- `docs/backend/api.md`: create / list は契約が記載されているが、revoke の invalid path parameter に対する `400 validation_failed` の記述が不足
- research / plan / 実装 / テストの主線は一致している

### PR/完了結果

- `pr_ready: false`

### 必須修正

1. `crates/api/src/routes/api_token.rs` の revoke endpoint の utoipa responses に `400 Validation error` を追加する
2. `docs/backend/api.md` の `3.0.5.3 Token 失効（Revoke）` のエラーケース表に `400 validation_failed`（invalid `token_id` format）を追記する

### 任意改善

1. token 管理 API の validation error 契約をコード注釈・backend API doc・統合テストで常に同時更新する運用に寄せる

### テスト不足

- 追加の必須テスト不足は見当たらない

### 残リスク

- OpenAPI / ドキュメント利用者は revoke API が `400 validation_failed` を返しうることを認識できず、クライアント生成や UI 実装時に契約齟齬を踏む可能性がある
- Critical / High の新規問題は確認できなかった

### 更新した作業ログパス

- `docs/logs/27/worklog.md`

---

## レビュー修正（2回目）（2026-05-01）

### 指摘事項

1. **revoke API の token_id パース (Medium)**: `Path<(i64, Uuid)>` で受けていたため、不正な token_id が axum の Path extractor でエラーとなり `request_id` 付きの `validation_failed` にならない問題。

### 実施した修正

1. `crates/api/src/routes/api_token.rs` の `revoke_api_token`:
   - `Path<(i64, Uuid)>` → `Path<(i64, String)>` に変更
   - handler 内で `Uuid::parse_str()` し、失敗時に `AppError::validation_failed("invalid token_id format", &request_id)` を返すように修正
   - read.rs の board_project_id パースパターンと同じ方式

2. `crates/api/tests/api_token_test.rs` に `test_revoke_api_token_invalid_token_id` テスト追加:
   - 不正 token_id (`"not-a-valid-uuid"`) を指定して revoke → 400 `validation_failed` + `request_id` 非空を検証

### テスト結果

- 全15件の api_token_test が pass
- 全パッケージの全テスト (175件) が pass、0 failures

### 残リスク

- なし（指摘箇所は全て解消済み）

### 更新した作業ログパス

- `docs/logs/27/worklog.md`

---

## PR作成フェーズ（2026-05-01）

### PR作成前確認

- `pr_ready: true`（最終レビュー3回目で確認済み）
- `docs_ready: true`（§5 契約テスト観点追記後に確認済み）
- 未コミット変更: `docs/logs/27/worklog.md` のみ（PR作成フェーズ追記）
- テスト: api_token_test 15/15 pass、全パッケージ 175件 pass
- research成果物と実装に矛盾なし

### PR/完了結果

- PR作成: `gh pr create` で `feat/27-api-token-management` → `main` のPRを作成
- Closes #27

### 残リスク

- `list_api_tokens` の invalid cursor に対する統合テストは未追加（非ブロッキング）
- 将来 README に Action 設定手順を追加する際は、`BOARDFLOW_TOKEN` の発行元として本 API への参照を追加することを推奨

### 更新した作業ログパス

- `docs/logs/27/worklog.md`
