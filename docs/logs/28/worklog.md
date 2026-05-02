# Issue #28 作業ログ: GitHub App Webhook受信エンドポイント実装

## Issue概要

GitHub App からの Webhook を受信するエンドポイントを実装する。
installation 関連のイベント（installation created/deleted、repositories added/removed）を処理し、repository 情報を DB 同期する。webhook 署名の検証も行う。

## Issueまでの経緯

- Issue #19（GitHub App クライアント）がマージ済み。`crates/github/` に octocrab ベースのクライアントが実装されている。
- `docs/spec.md` および `docs/backend/summary.md` で GitHub App webhook は MVP から含めると明記。
- `docs/backend/summary.md` のテスト方針に「GitHub webhook signature verification test」が含まれている。
- api crate には既に `hmac`, `sha2`, `hex` が依存に含まれている。

## ユーザー要望

docs 以下の仕様に基づいてアプリケーションを一通り実装する。

## 調査結果

### 2026-05-02: 外部調査完了 (research agent)

以下の外部トピックを調査し、`docs/external/github-webhook-signature.md` に記録した。

#### 1. GitHub App Webhook 署名検証
- HMAC-SHA256 アルゴリズムで `X-Hub-Signature-256` ヘッダを検証
- Rust では `hmac` + `sha2` + `hex` crate（全て api crate に既存）で実装可能
- `hmac` crate の `verify_slice()` で定数時間比較が組み込み済み
- 追加 crate 不要

#### 2. Webhook Payload 構造
- `installation` イベント: action = created / deleted
  - `installation.id`, `repositories[].id`, `repositories[].full_name` 等を含む
- `installation_repositories` イベント: action = added / removed
  - `repositories_added[]`, `repositories_removed[]` で差分を受信
- `ping` イベント: webhook 設定確認用、200 OK 返却のみ

#### 3. ベストプラクティス
- 10秒以内に応答する
- `X-GitHub-Delivery` で冪等性を確保可能（MVPでは upsert で十分）
- raw body に対して署名検証を行う（JSON パース後のデータではなく）

#### 4. Axum での実装パターン
- `HeaderMap` + `Bytes` を handler 引数に並べる
- `Bytes` は body を消費するため最後に配置
- 署名検証後に同じ `Bytes` から `serde_json::from_slice` でパース

### 参照URL
- https://docs.github.com/en/webhooks/using-webhooks/securing-your-webhooks
- https://docs.github.com/en/webhooks/webhook-events-and-payloads#installation
- https://docs.github.com/en/webhooks/webhook-events-and-payloads#installation_repositories
- https://docs.github.com/en/webhooks/using-webhooks/best-practices-for-using-webhooks

## 結論ステータス

**`implementation_required`**

調査は完了し、実装に必要な情報はすべて揃っている。以下が後続実装で必要:

1. `AppConfig` に `github_webhook_secret` を追加
2. `POST /api/v1/github/webhook` エンドポイントを api crate に追加
3. 署名検証関数の実装（`hmac` + `sha2` + `hex`）
4. Webhook payload の serde 型定義
5. installation/repository の DB 同期ロジック（upsert）
6. 署名検証のユニットテスト
7. webhook handler の統合テスト

## 残リスク

- なし（MVPに必要な外部情報は十分）

---

## 計画フェーズ (2026-05-02)

### 実装要否: `implementation_required`

### 目的

GitHub App からの Webhook を受信・検証し、installation/repository の状態を DB に同期するエンドポイントを実装する。これにより、GitHub App がインストール/アンインストールされたときやリポジトリが追加/削除されたときに、BoardFlow の repositories テーブルが自動的に更新される。

### 非目的

- push イベントの処理（Webhookではなく Action API 経由で処理）
- Webhook delivery ID による冪等性チェック（upsert で十分）
- Redis / Queue への enqueue（同期処理で十分な軽量 upsert のみ）
- installation テーブルの作成（MVP では repositories テーブルの installation_id で管理）
- OpenAPI ドキュメント登録（webhook は外部からの呼び出しであり、utoipa ルート登録不要）

### 受け入れ条件

1. `POST /api/v1/github/webhook` でリクエストを受信できる
2. `X-Hub-Signature-256` ヘッダによる HMAC-SHA256 署名検証が行われる
3. 署名不正時は 401 を返す
4. `ping` イベントで 200 を返す
5. `installation` created で repositories テーブルに upsert される
6. `installation` deleted で該当 installation_id のリポジトリの installation_id が 0 にクリアされる
7. `installation_repositories` added で repositories テーブルに upsert される
8. `installation_repositories` removed で該当リポジトリの installation_id が 0 にクリアされる
9. 未対応イベントは 200 を返して無視する
10. `GITHUB_WEBHOOK_SECRET` 未設定時は webhook エンドポイントが 500 を返す（署名検証不可のため）
11. 署名検証のユニットテストがある
12. webhook handler の統合テスト（DB あり）がある

### 詳細要件

#### 1. AppConfig 変更

`AppConfig` に `github_webhook_secret: Option<String>` を追加。環境変数 `GITHUB_WEBHOOK_SECRET` から読み込む。

#### 2. 署名検証関数

```rust
// crates/api/src/routes/webhook.rs 内
fn verify_signature(secret: &[u8], body: &[u8], signature_header: &str) -> bool
```

- `sha256=` プレフィックスを除去
- `hex::decode` で期待値をバイト列に変換
- `Hmac<Sha256>` で HMAC を計算
- `mac.verify_slice()` で定数時間比較

#### 3. Webhook Payload serde 型

```rust
// crates/api/src/routes/webhook.rs 内（ファイルローカル）

#[derive(Deserialize)]
struct WebhookRepository {
    id: i64,
    name: String,
    full_name: String,
}

#[derive(Deserialize)]
struct WebhookInstallation {
    id: i64,
}

#[derive(Deserialize)]
struct InstallationEvent {
    action: String,
    installation: WebhookInstallation,
    #[serde(default)]
    repositories: Vec<WebhookRepository>,
}

#[derive(Deserialize)]
struct InstallationRepositoriesEvent {
    action: String,
    installation: WebhookInstallation,
    #[serde(default)]
    repositories_added: Vec<WebhookRepository>,
    #[serde(default)]
    repositories_removed: Vec<WebhookRepository>,
}
```

#### 4. Handler 関数

```rust
// crates/api/src/routes/webhook.rs

pub async fn github_webhook(
    State(pool): State<PgPool>,
    Extension(webhook_secret): Extension<WebhookSecret>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, AppError>
```

処理フロー:
1. `webhook_secret.0` が None なら 500 Internal Error
2. `X-Hub-Signature-256` ヘッダを取得。なければ 401
3. `verify_signature()` で検証。失敗なら 401
4. `X-GitHub-Event` ヘッダでイベント種別を判定
5. イベントに応じた処理:
   - `ping` → 200 OK
   - `installation` → `handle_installation_event()`
   - `installation_repositories` → `handle_installation_repositories_event()`
   - その他 → 200 OK（ログ出力のみ）

#### 5. イベントハンドラ

**handle_installation_event:**
- `created`: `repositories` 配列の各リポジトリを `full_name` から owner/name を分割して upsert
- `deleted`: 該当 `installation_id` のリポジトリの `installation_id` を 0 にクリア

**handle_installation_repositories_event:**
- `added`: `repositories_added` の各リポジトリを upsert
- `removed`: `repositories_removed` の各リポジトリの `installation_id` を 0 にクリア

#### 6. DB クエリ追加

`crates/db/src/queries/repository.rs` に以下を追加:

```rust
pub async fn clear_installation(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    installation_id: i64,
) -> Result<Vec<Repository>, sqlx::Error>
// UPDATE repositories SET installation_id = 0, updated_at = NOW()
// WHERE installation_id = $1 RETURNING *

pub async fn clear_installation_for_repo(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    github_repository_id: i64,
) -> Result<Option<Repository>, sqlx::Error>
// UPDATE repositories SET installation_id = 0, updated_at = NOW()
// WHERE github_repository_id = $1 RETURNING *
```

#### 7. Extension 型

```rust
// crates/api/src/lib.rs に追加
#[derive(Clone)]
pub struct WebhookSecret(pub Option<String>);
```

#### 8. ルーター登録

`create_app_with_config` に:
- 引数 `webhook_secret: Option<String>` を追加
- `WebhookSecret` Extension を layer に追加
- `.route("/api/v1/github/webhook", post(routes::webhook::github_webhook))` を追加
  - OpenAPI ルーターではなく、通常の `.route()` で登録（utoipa 不要）

### 影響範囲

| ファイル | 変更内容 |
|---|---|
| `crates/api/src/config.rs` | `github_webhook_secret` フィールド追加 |
| `crates/api/src/lib.rs` | `WebhookSecret` 型追加、`create_app_with_config` 引数追加、route 登録、Extension layer 追加 |
| `crates/api/src/routes/mod.rs` | `pub mod webhook;` 追加 |
| `crates/api/src/routes/webhook.rs` | **新規作成** — handler、署名検証、payload 型 |
| `crates/api/src/main.rs` | `AppConfig` から `webhook_secret` を `create_app_with_config` に渡す |
| `crates/db/src/queries/repository.rs` | `clear_installation`、`clear_installation_for_repo` 追加 |
| `crates/api/tests/webhook_test.rs` | **新規作成** — 統合テスト |

### 設計方針

1. **署名検証は raw body に対して行う**: `Bytes` extractor で取得した生バイト列に対して HMAC 検証し、その後 `serde_json::from_slice` でパースする
2. **OpenAPI 登録しない**: Webhook は GitHub からの着信であり、BoardFlow の公開 API ドキュメントに載せる必要がない。通常の `Router::route()` で登録する
3. **同期処理**: DB upsert は軽量なので、queue 経由にせず handler 内で同期的に完了する
4. **installation 解除は論理削除**: `installation_id = 0` でクリアし、repository レコード自体は残す（BoardProject や BoardRun の参照整合性を保つ）
5. **create_app_with_config の引数追加**: 既存パターンに合わせて Option<String> で webhook_secret を渡す。テストではテスト用 secret を指定する

### 実装順序（TDD前提）

#### Step 1: 署名検証ユニットテスト & 実装
1. `crates/api/src/routes/webhook.rs` を作成
2. `verify_signature()` 関数を実装
3. GitHub 公式テスト値でユニットテスト（`#[cfg(test)]` モジュール内）:
   - secret: `"It's a Secret to Everybody"`, payload: `"Hello, World!"`
   - 正しい署名 → true
   - 不正な署名 → false
   - プレフィックスなし → false
   - 不正な hex → false

#### Step 2: Payload serde 型
1. `InstallationEvent`, `InstallationRepositoriesEvent` 等の型を定義
2. 調査結果の JSON サンプルでデシリアライズのユニットテスト

#### Step 3: DB クエリ追加
1. `clear_installation` を `crates/db/src/queries/repository.rs` に追加
2. `clear_installation_for_repo` を同ファイルに追加

#### Step 4: AppConfig & Extension 変更
1. `AppConfig.github_webhook_secret` 追加
2. `WebhookSecret` 型を `lib.rs` に追加
3. `create_app_with_config` の引数と layer 追加
4. `main.rs` で config から渡す

#### Step 5: Handler 実装
1. `github_webhook` handler を実装
2. `handle_installation_event`, `handle_installation_repositories_event` を実装
3. route 登録

#### Step 6: 統合テスト
1. `crates/api/tests/webhook_test.rs` を作成
2. テストケース:
   - **test_webhook_ping**: ping イベントで 200 が返る
   - **test_webhook_invalid_signature**: 不正な署名で 401 が返る
   - **test_webhook_missing_signature**: 署名ヘッダなしで 401 が返る
   - **test_webhook_installation_created**: installation created で repositories が upsert される
   - **test_webhook_installation_deleted**: installation deleted で installation_id が 0 にクリアされる
   - **test_webhook_repos_added**: installation_repositories added で upsert される
   - **test_webhook_repos_removed**: installation_repositories removed で installation_id が 0 にクリアされる
   - **test_webhook_unknown_event**: 未知イベントで 200 が返る

統合テストの構成:
- `#[serial]` 付き
- `setup_pool()` で DB 接続・マイグレーション
- `create_app_with_config` でテスト用 webhook_secret を設定
- HMAC-SHA256 でテスト用署名を計算
- `tower::ServiceExt::oneshot` でリクエスト送信
- DB 状態を `sqlx::query_as` で検証

### テスト観点

| テスト | 種別 | ファイル |
|---|---|---|
| verify_signature 正常系 | ユニット | `webhook.rs` 内 `#[cfg(test)]` |
| verify_signature 異常系（不正署名、prefix なし、不正 hex） | ユニット | 同上 |
| payload デシリアライズ | ユニット | 同上 |
| ping イベント | 統合 | `webhook_test.rs` |
| 署名検証失敗 | 統合 | 同上 |
| installation created → DB upsert | 統合 | 同上 |
| installation deleted → installation_id クリア | 統合 | 同上 |
| installation_repositories added → DB upsert | 統合 | 同上 |
| installation_repositories removed → installation_id クリア | 統合 | 同上 |
| 未知イベント → 200 | 統合 | 同上 |

### ドキュメント更新対象

- `docs/logs/28/worklog.md` — 本作業ログ（実装完了時にも追記）
- `docs/backend/api.md` への追記は不要（webhook は外部からの着信であり、BoardFlow の公開 API 仕様ではない）

### 未解決の疑問

なし。仕様・外部調査とも十分な情報がある。

### 更新した作業ログパス

`docs/logs/28/worklog.md`
