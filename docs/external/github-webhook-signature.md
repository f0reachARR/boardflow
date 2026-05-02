# GitHub App Webhook 署名検証・Payload構造・Axum実装パターン調査

## 要約

GitHub App Webhook の受信エンドポイント実装（Issue #28）に必要な外部知識を調査した。
署名検証は HMAC-SHA256 で `X-Hub-Signature-256` ヘッダを使う。BoardFlow の api crate には既に `hmac`, `sha2`, `hex` が依存に含まれており、新規 crate 追加なしで実装可能。
Axum では `HeaderMap` と `Bytes` を handler 引数に並べることで raw body と headers を同時に取得できる。

## 確認した情報

### 1. 署名検証アルゴリズム

**GitHub公式要件（https://docs.github.com/en/webhooks/using-webhooks/securing-your-webhooks）:**

1. GitHub App 設定で webhook secret を登録する
2. GitHub は各 delivery で `X-Hub-Signature-256` ヘッダを付与する
3. 値の形式: `sha256=<HMAC-SHA256 hex digest>`
4. HMAC key = webhook secret, message = raw request body (UTF-8)
5. 比較は定数時間比較（timing-safe）で行う必要がある
6. `X-Hub-Signature` (SHA-1) はレガシーであり、`X-Hub-Signature-256` を推奨

**検証テスト値（GitHub公式）:**

```
secret:    "It's a Secret to Everybody"
payload:   "Hello, World!"
signature: sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17
```

**Rustでの実装コード例:**

```rust
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

fn verify_signature(secret: &[u8], body: &[u8], signature_header: &str) -> bool {
    // "sha256=..." からプレフィックスを除去
    let hex_signature = match signature_header.strip_prefix("sha256=") {
        Some(hex) => hex,
        None => return false,
    };

    // 期待されるsignatureをデコード
    let expected = match hex::decode(hex_signature) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };

    // HMAC-SHA256を計算
    let mut mac = HmacSha256::new_from_slice(secret)
        .expect("HMAC accepts any key length");
    mac.update(body);
    let computed = mac.finalize().into_bytes();

    // 定数時間比較（timing attack防止）
    computed.as_slice().ct_eq(&expected).into()
}
```

**定数時間比較について:**

- `subtle` crate の `ConstantTimeEq` を使う方法が Rust では一般的
- `hmac` crate の `mac.verify_slice()` も内部で定数時間比較を行う
- BoardFlow では `hmac` crate が既に依存にあるので `mac.verify_slice()` を使うのがシンプル

```rust
// hmac crate の verify_slice を使う場合（推奨）
fn verify_signature(secret: &[u8], body: &[u8], signature_header: &str) -> bool {
    let hex_signature = match signature_header.strip_prefix("sha256=") {
        Some(hex) => hex,
        None => return false,
    };

    let expected = match hex::decode(hex_signature) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };

    let mut mac = HmacSha256::new_from_slice(secret)
        .expect("HMAC accepts any key length");
    mac.update(body);

    mac.verify_slice(&expected).is_ok()
}
```

### 2. Webhook Delivery ヘッダ

GitHub は各 webhook delivery に以下のヘッダを含める:

| ヘッダ | 説明 | BoardFlow での用途 |
|---|---|---|
| `X-GitHub-Event` | イベント名 (`installation`, `installation_repositories`, `ping` など) | イベントのルーティング |
| `X-GitHub-Delivery` | GUID (delivery ごとに一意) | 冪等性チェック、ログ追跡 |
| `X-Hub-Signature-256` | `sha256=<HMAC hex>` | 署名検証 |
| `X-GitHub-Hook-ID` | Webhook ID | ログ |
| `X-GitHub-Hook-Installation-Target-Type` | `integration` (GitHub App の場合) | 識別補助 |
| `X-GitHub-Hook-Installation-Target-ID` | App ID | 識別補助 |
| `User-Agent` | `GitHub-Hookshot/...` | ログ |

### 3. Webhook Event Payload 構造

#### 3.1 `installation` イベント

GitHub App がインストール/アンインストールされたときに発生。全 GitHub App が自動受信。

**action: `created`** (App がインストールされた)

```json
{
  "action": "created",
  "installation": {
    "id": 12345678,
    "account": {
      "login": "ForteFibre",
      "id": 987654,
      "type": "Organization"
    },
    "app_id": 123456,
    "target_type": "Organization",
    "permissions": {
      "issues": "write",
      "contents": "read",
      "metadata": "read"
    },
    "events": ["installation", "installation_repositories", "push"],
    "created_at": "2026-05-01T00:00:00Z",
    "updated_at": "2026-05-01T00:00:00Z"
  },
  "repositories": [
    {
      "id": 111222333,
      "node_id": "R_abc123",
      "name": "hardware",
      "full_name": "ForteFibre/hardware",
      "private": false
    }
  ],
  "sender": {
    "login": "user",
    "id": 1
  }
}
```

**action: `deleted`** (App がアンインストールされた)

```json
{
  "action": "deleted",
  "installation": {
    "id": 12345678,
    "account": { "login": "ForteFibre", "id": 987654 }
  },
  "repositories": [
    {
      "id": 111222333,
      "node_id": "R_abc123",
      "name": "hardware",
      "full_name": "ForteFibre/hardware",
      "private": false
    }
  ],
  "sender": { "login": "user", "id": 1 }
}
```

**BoardFlow での処理:**
- `created`: `repositories` テーブルに `installation_id` を紐づけて upsert
- `deleted`: 該当 `installation_id` に関連する repository の連携状態を解除（削除ではない）

#### 3.2 `installation_repositories` イベント

GitHub App がアクセスできるリポジトリが追加/削除されたときに発生。

**action: `added`**

```json
{
  "action": "added",
  "installation": {
    "id": 12345678,
    "account": { "login": "ForteFibre", "id": 987654 }
  },
  "repository_selection": "selected",
  "repositories_added": [
    {
      "id": 444555666,
      "node_id": "R_def456",
      "name": "new-board",
      "full_name": "ForteFibre/new-board",
      "private": true
    }
  ],
  "repositories_removed": [],
  "sender": { "login": "user", "id": 1 }
}
```

**action: `removed`**

```json
{
  "action": "removed",
  "installation": {
    "id": 12345678,
    "account": { "login": "ForteFibre", "id": 987654 }
  },
  "repository_selection": "selected",
  "repositories_added": [],
  "repositories_removed": [
    {
      "id": 444555666,
      "node_id": "R_def456",
      "name": "new-board",
      "full_name": "ForteFibre/new-board",
      "private": true
    }
  ],
  "sender": { "login": "user", "id": 1 }
}
```

**BoardFlow での処理:**
- `added`: `repositories` テーブルに `installation_id` 付きで upsert
- `removed`: 該当リポジトリの連携状態を解除（BoardProject や BoardRun は残す）

#### 3.3 `ping` イベント

Webhook 設定時に GitHub が送信する確認イベント。

```json
{
  "zen": "Responsive is better than fast.",
  "hook_id": 292430182,
  "hook": { ... },
  "sender": { "login": "user", "id": 1 }
}
```

**BoardFlow での処理:** 200 OK を返すだけでよい。

### 4. Axum での Raw Body + Headers 同時取得パターン

Axum では extractor を handler の引数に複数並べることで、headers と raw body を同時に取得できる。
**重要: `Bytes` は body を消費するため、extractors の最後に置く必要がある。**

```rust
use axum::{
    body::Bytes,
    http::{HeaderMap, StatusCode},
    routing::post,
    Router,
};

async fn webhook_handler(
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    // 1. X-Hub-Signature-256 を取得
    let signature = match headers.get("x-hub-signature-256") {
        Some(sig) => match sig.to_str() {
            Ok(s) => s,
            Err(_) => return StatusCode::BAD_REQUEST,
        },
        None => return StatusCode::UNAUTHORIZED,
    };

    // 2. 署名検証
    let webhook_secret = b"your-webhook-secret"; // 環境変数から取得
    if !verify_signature(webhook_secret, &body, signature) {
        return StatusCode::UNAUTHORIZED;
    }

    // 3. X-GitHub-Event でルーティング
    let event = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    match event {
        "installation" => handle_installation(&body).await,
        "installation_repositories" => handle_installation_repositories(&body).await,
        "ping" => StatusCode::OK,
        _ => {
            tracing::debug!(event, "unhandled webhook event");
            StatusCode::OK
        }
    }
}

// ルーター登録
fn webhook_router() -> Router {
    Router::new().route("/api/v1/github/webhook", post(webhook_handler))
}
```

**Axum extractor の順序規則:**
- `HeaderMap` は body を消費しないので先に置ける
- `Bytes` は body を消費するため最後
- `Json<T>` も body を消費するため `Bytes` とは併用不可（raw body を先に取って手動パース）

### 5. 必要な Rust crate

| crate | 用途 | BoardFlow での状態 |
|---|---|---|
| `hmac` | HMAC 計算 | **既に api crate 依存に含まれている** |
| `sha2` | SHA-256 | **既に api crate 依存に含まれている** |
| `hex` | hex encode/decode | **既に api crate 依存に含まれている** |
| `axum` | HTTP framework | **既に api crate 依存に含まれている** |
| `serde` / `serde_json` | JSON パース | **既に api crate 依存に含まれている** |

追加 crate は不要。`subtle` crate は `hmac` の `verify_slice` を使えば不要。

### 6. レスポンス方針

GitHub は webhook delivery のレスポンスとして:
- **10秒以内に応答** することを推奨
- ステータスコード `2xx` を成功とみなす
- `4xx` / `5xx` はリトライ対象

BoardFlow では:
- 署名検証失敗: `401 Unauthorized`
- 処理成功: `200 OK`（レスポンスボディは空か `{"received": true}` でよい）
- DB 処理は同期的に行う（installation 同期は軽量な upsert のみ）
- 重い処理が必要な場合は job enqueue して即座に 200 を返す

### 7. 冪等性

- `X-GitHub-Delivery` ヘッダの GUID で delivery を一意に識別できる
- MVP では DB に delivery ID を保存する冪等性チェックは必須ではない
- installation/repository の upsert 自体が冪等な操作

### 8. セキュリティ考慮事項

- webhook secret は環境変数で管理し、コードにハードコードしない
- 署名検証は **必ず raw body に対して行う**（JSON パース後のシリアライズではなく、受信したバイト列そのもの）
- 定数時間比較を使い、timing attack を防ぐ
- `X-Hub-Signature-256` が存在しない場合は署名検証をスキップするのではなく、リクエストを拒否する

## BoardFlow への示唆

1. **エンドポイント**: `POST /api/v1/github/webhook` を api crate に追加
2. **設定**: `AppConfig` に `github_webhook_secret: Option<String>` を追加
3. **署名検証**: `hmac` + `sha2` + `hex` で実装（新規 crate 不要）
4. **イベント処理**:
   - `installation` created → repositories テーブルに installation_id 付きで upsert
   - `installation` deleted → 該当 installation の repository 連携状態を解除
   - `installation_repositories` added → repositories テーブルに upsert
   - `installation_repositories` removed → 該当 repository の連携状態を解除
   - `ping` → 200 OK
5. **Axum パターン**: `HeaderMap` + `Bytes` で raw body と headers を同時取得
6. **Payload 型**: serde の `#[serde(tag = "action")]` で action ごとにデシリアライズ可能

## 採用/不採用判断

- **HMAC-SHA256署名検証**: 採用（GitHub公式要件、既存crate活用）
- **`Bytes` + `HeaderMap` パターン**: 採用（Axum標準機能、追加依存なし）
- **Payload型の自前定義**: 採用（octocrab の webhook 型は使わず、必要なフィールドのみ定義）
- **delivery ID による冪等性テーブル**: MVP不採用（upsert で十分）

## 制約と pitfall

1. **body は一度しか読めない**: Axum で `Bytes` を使うと body を消費するため、署名検証と JSON パースで同じ `Bytes` を使い回す必要がある
2. **proxy/load balancer による body 変更**: リバースプロキシがボディを変更すると署名が不一致になる。nginx等でボディを変更しない設定が必要
3. **GitHub は 10秒以内の応答を期待**: DB upsert 程度なら問題ないが、重い処理は非同期化する
4. **`X-Hub-Signature` (SHA-1) は無視**: SHA-256 のみ使用する
5. **webhook secret のローテーション**: GitHub App 設定画面から secret を変更すると、古い secret での署名検証が失敗する。MVP では単一 secret で運用

## 未解決の疑問

- なし（MVP に必要な情報は十分に揃っている）

## 参照URL

- https://docs.github.com/en/webhooks/using-webhooks/securing-your-webhooks
- https://docs.github.com/en/webhooks/webhook-events-and-payloads#installation
- https://docs.github.com/en/webhooks/webhook-events-and-payloads#installation_repositories
- https://docs.github.com/en/webhooks/using-webhooks/best-practices-for-using-webhooks
- https://docs.github.com/en/apps/creating-github-apps/about-creating-github-apps/about-creating-github-apps
