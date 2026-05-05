# reqwest リトライ (指数バックオフ) 実装パターン

## 要約

`reqwest` を使った HTTP リクエストのリトライ戦略。3回リトライ、指数バックオフ、5xx/timeout に対するリトライを実装する。`reqwest-middleware` + `reqwest-retry` クレートを使ったミドルウェア方式と、手動ループ方式の2パターンを整理。

## 確認した情報

### 方式1: reqwest-middleware + reqwest-retry (推奨)

```toml
[dependencies]
reqwest = { version = "0.12", features = ["json"] }
reqwest-middleware = "0.4"
reqwest-retry = "0.7"
retry-policies = "0.4"
```

```rust
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};

fn build_api_client() -> ClientWithMiddleware {
    let retry_policy = ExponentialBackoff::builder()
        .build_with_max_retries(3);

    ClientBuilder::new(reqwest::Client::new())
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build()
}
```

**利点**:
- 5xx と接続エラーをデフォルトで transient (リトライ対象) として分類
- 4xx は permanent (リトライしない) として分類
- ジッター付き指数バックオフが組み込み
- `ClientWithMiddleware` は `reqwest::Client` とほぼ同じ API

**カスタム戦略**:
```rust
use reqwest_retry::{Retryable, RetryableStrategy};
use reqwest::StatusCode;

struct BoardFlowRetryStrategy;

impl RetryableStrategy for BoardFlowRetryStrategy {
    fn handle(
        &self,
        res: &Result<reqwest::Response, reqwest_middleware::Error>,
    ) -> Option<Retryable> {
        match res {
            Ok(response) => {
                let status = response.status();
                if status.is_server_error() {
                    Some(Retryable::Transient)
                } else if status == StatusCode::TOO_MANY_REQUESTS {
                    Some(Retryable::Transient)
                } else {
                    Some(Retryable::Fatal)
                }
            }
            Err(_) => Some(Retryable::Transient), // ネットワークエラー
        }
    }
}
```

### 方式2: 手動リトライループ (依存少)

```rust
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;

pub struct ApiClient {
    client: Client,
    base_url: String,
    token: String,
    max_retries: u32,
}

impl ApiClient {
    pub fn new(base_url: String, token: String) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            base_url,
            token,
            max_retries: 3,
        }
    }

    pub async fn request(
        &self,
        method: reqwest::Method,
        endpoint: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, ApiError> {
        let url = format!("{}{}", self.base_url, endpoint);
        let mut backoff = Duration::from_secs(1);

        for attempt in 1..=self.max_retries {
            let mut req = self.client.request(method.clone(), &url)
                .header("Authorization", format!("Bearer {}", self.token))
                .header("Content-Type", "application/json");

            if let Some(body) = body {
                req = req.json(body);
            }

            match req.send().await {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        return response.json().await
                            .map_err(|e| ApiError::Parse(e.to_string()));
                    }

                    if status.is_server_error() && attempt < self.max_retries {
                        eprintln!(
                            "Retry {}/{}: HTTP {} for {}",
                            attempt, self.max_retries, status, endpoint
                        );
                        sleep(backoff).await;
                        backoff *= 2;
                        continue;
                    }

                    let body = response.text().await.unwrap_or_default();
                    return Err(ApiError::Http { status, body });
                }
                Err(e) if e.is_timeout() || e.is_connect() => {
                    if attempt < self.max_retries {
                        eprintln!(
                            "Retry {}/{}: {} for {}",
                            attempt, self.max_retries, e, endpoint
                        );
                        sleep(backoff).await;
                        backoff *= 2;
                        continue;
                    }
                    return Err(ApiError::Network(e.to_string()));
                }
                Err(e) => return Err(ApiError::Network(e.to_string())),
            }
        }

        Err(ApiError::MaxRetries)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("HTTP {status}: {body}")]
    Http { status: reqwest::StatusCode, body: String },
    #[error("Network error: {0}")]
    Network(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Max retries exceeded")]
    MaxRetries,
}
```

### bash 実装との対応

| bash (api.sh) | Rust |
|---|---|
| `max_retries=3` | `max_retries: 3` |
| `backoff=1` / `backoff=$((backoff * 2))` | `backoff = Duration::from_secs(1)` / `backoff *= 2` |
| `--connect-timeout 30` | `.connect_timeout(Duration::from_secs(30))` |
| `--max-time 60` | `.timeout(Duration::from_secs(60))` |
| 5xx → リトライ | `status.is_server_error()` → リトライ |
| 4xx → 即エラー | `status.is_client_error()` → 即エラー |
| curl exit != 0 → リトライ | `e.is_timeout() \|\| e.is_connect()` → リトライ |

## BoardFlow への示唆

- **推奨**: 方式2 (手動リトライ) を採用。理由:
  - 依存クレート追加が不要 (workspace に既に `reqwest` あり)
  - bash 実装と完全に同じロジックを再現しやすい
  - action-runner は限定的な API 呼び出しのみ (plan, create, import, fail の4エンドポイント)
  - reqwest-middleware は追加依存が多く、action-runner の軽量性に反する
- バンドルアップロード (`PUT` presigned URL) は別途タイムアウト設定: `--max-time 600` → `.timeout(Duration::from_secs(600))`

## 採用/不採用判断

**採用**: 方式2 (手動リトライループ) — bash 実装を忠実に再現

## 制約とpitfall

1. **タイムアウト設定の分離**: API 呼び出し (60s) とバンドルアップロード (600s) でタイムアウトが異なる
2. **リトライ対象**: 5xx とネットワークエラーのみ。429 (Rate Limit) のリトライも検討すべき
3. **ジッター**: bash 実装にはジッターがない。Rust 実装でもジッターなしで忠実再現が望ましい
4. **レスポンスボディ消費**: `response.text()` や `response.json()` は body を消費するため、エラー時のログ出力とリトライのタイミングに注意
5. **reqwest の TLS**: workspace で `rustls` を使っている。action-runner でも同じ feature flag を使う

## 未解決の疑問

- なし

## 参照URL

- https://docs.rs/reqwest-retry/
- https://docs.rs/reqwest-middleware/
- https://crates.io/crates/reqwest-retry
- https://docs.rs/reqwest/0.12/
