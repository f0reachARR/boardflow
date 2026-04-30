# Rust Crate バージョン調査メモ

対象Issue: #1 (Rust workspaceセットアップとDB基盤)

## 1. 要約

Issue #1 の実装に必要な主要 Rust crate の最新安定バージョンと互換性を調査した。Axum 0.8 + utoipa 5 + utoipa-axum 0.2 の組み合わせは公式に互換性あり。SQLx 0.8.6 は PostgreSQL 対応・offline mode ともに安定。OpenTelemetry 連携は tracing-opentelemetry のバージョンが opentelemetry 本体より遅れるため、互換セットに注意が必要。

## 2. 確認した情報

### 2.1 Axum

| 項目 | 値 |
|---|---|
| 最新安定バージョン | **0.8.9** (2026-04-14) |
| 推奨 Cargo.toml 指定 | `axum = "0.8"` |
| MSRV | rustc 1.65+ |
| 備考 | 0.8.0 は 2025-01-01 リリース。0.7→0.8 で Router API に breaking changes あり |

基本パターン (Axum 0.8):
```rust
use axum::{routing::get, Router};

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(|| async { "Hello, World!" }));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

### 2.2 SQLx

| 項目 | 値 |
|---|---|
| 最新安定バージョン | **0.8.6** |
| 推奨 Cargo.toml 指定 | `sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "uuid", "chrono", "migrate"] }` |
| CLI | `cargo install sqlx-cli --no-default-features --features rustls,postgres` |
| PostgreSQL サポート | 完全対応 (compile-time checked queries) |
| マイグレーション | `sqlx migrate run` / `sqlx migrate add <name>` |

#### SQLx Offline Mode セットアップ

1. **準備コマンド** (DBに接続した状態で実行):
   ```bash
   cargo sqlx prepare --workspace --all -- --all-targets
   ```
   - `--all-targets` を付けることでテストコード内のクエリも含めて `.sqlx/` に記録される

2. **CI での利用**:
   - `.sqlx/` ディレクトリをリポジトリにコミットする
   - CI で `SQLX_OFFLINE=true` 環境変数をセット
   - DB接続なしでコンパイル時クエリチェックが通る

3. **チェックコマンド** (CIで `.sqlx` が最新か検証):
   ```bash
   cargo sqlx prepare --workspace --check -- --all-targets
   ```

4. **注意点**:
   - `.env` に `SQLX_OFFLINE=true` を設定したまま `cargo sqlx prepare` を実行するとエラーになる
   - 準備コマンド実行時は `SQLX_OFFLINE` を unset する必要がある

### 2.3 utoipa + utoipa-axum

| 項目 | 値 |
|---|---|
| utoipa 最新バージョン | **5.4.0** |
| utoipa-axum 最新バージョン | **0.2.0** |
| 推奨 Cargo.toml 指定 | `utoipa = { version = "5", features = ["axum_extras"] }` |
| 推奨 Cargo.toml 指定 | `utoipa-axum = "0.2"` |

#### Axum 0.8 との互換性 ✅ 確認済み

`utoipa-axum` 0.2.0 の依存関係:
- `axum ^0.8.0` — **Axum 0.8 に公式対応**
- `utoipa ^5.0.0` — utoipa 5.x 系と互換

使用パターン:
```rust
use utoipa_axum::{routes, router::OpenApiRouter};

#[utoipa::path(get, path = "/user", responses((status = OK, body = User)))]
async fn get_user() -> impl IntoResponse { /* ... */ }

let (router, api) = OpenApiRouter::new()
    .routes(routes!(get_user))
    .split_for_parts();
```

### 2.4 Tokio

| 項目 | 値 |
|---|---|
| 最新安定バージョン | **1.52.0** |
| 推奨 Cargo.toml 指定 | `tokio = { version = "1", features = ["full"] }` |
| LTS リリース | 1.47.x (2026年9月まで), 1.51.x (2027年3月まで) |
| 備考 | semver 1.x 系なのでバージョン指定 `"1"` で十分 |

### 2.5 tracing + tracing-subscriber

| 項目 | 値 |
|---|---|
| tracing 最新バージョン | **0.1.44** |
| tracing-subscriber 最新バージョン | **0.3.23** |
| 推奨 Cargo.toml 指定 | `tracing = "0.1"` |
| 推奨 Cargo.toml 指定 | `tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }` |
| MSRV | rustc 1.65+ |

### 2.6 OpenTelemetry 連携

| 項目 | 値 |
|---|---|
| opentelemetry (API) 最新 | 0.31.0 |
| opentelemetry_sdk 最新 | 0.31.0 |
| opentelemetry-otlp 最新 | 0.31.1 |
| tracing-opentelemetry 最新 | **0.28.0** |

#### 互換セットの注意

`tracing-opentelemetry` は tokio-rs/tracing リポジトリで管理されており、opentelemetry 本体とはバージョン番号が異なる。`tracing-opentelemetry` 0.28.0 は `opentelemetry` 0.27.x または 0.28.x と互換（歴史的に1つズレるパターンがあったが、現在は揃った可能性が高い）。

**推奨する互換セット（安全な組み合わせ）:**
```toml
opentelemetry = "0.28"
opentelemetry_sdk = { version = "0.28", features = ["rt-tokio"] }
opentelemetry-otlp = { version = "0.28", features = ["tonic"] }
tracing-opentelemetry = "0.28"
```

> ⚠️ opentelemetry 本体は 0.31 まで進んでいるが、tracing-opentelemetry が追随していないため、0.28 系で揃えるのが最も安全。将来 tracing-opentelemetry が更新されたらアップグレード可能。

### 2.7 Docker Compose サービス

| サービス | イメージ | ポート |
|---|---|---|
| PostgreSQL | `postgres:16-alpine` | 5432 |
| Redis | `redis:7-alpine` | 6379 |
| MinIO | `minio/minio:latest` | 9000 (API), 9001 (Console) |

推奨 Docker Compose 構成:
```yaml
services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: boardflow
      POSTGRES_USER: boardflow
      POSTGRES_PASSWORD: boardflow
    ports:
      - "5432:5432"
    volumes:
      - postgres-data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U $$POSTGRES_USER -d $$POSTGRES_DB"]
      interval: 5s
      timeout: 5s
      retries: 10

  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 5s
      timeout: 3s
      retries: 10

  minio:
    image: minio/minio:latest
    command: server /data --console-address ":9001"
    environment:
      MINIO_ROOT_USER: minioadmin
      MINIO_ROOT_PASSWORD: minioadmin
    ports:
      - "9000:9000"
      - "9001:9001"
    volumes:
      - minio-data:/data

volumes:
  postgres-data:
  minio-data:
```

## 3. BoardFlow への示唆

- Axum 0.8 + utoipa 5 + utoipa-axum 0.2 の組み合わせで OpenAPI 生成がそのまま動く
- SQLx 0.8 の offline mode で CI 時の DB 不要ビルドが可能
- OpenTelemetry は tracing-opentelemetry のバージョンに合わせて 0.28 系で統一する
- Docker Compose では healthcheck を設定し、サービス起動順を制御する

## 4. 採用/不採用判断

| Crate | 判断 | 理由 |
|---|---|---|
| axum 0.8 | ✅ 採用 | 最新安定、tokio 公式、活発なメンテナンス |
| sqlx 0.8 | ✅ 採用 | compile-time checked、PostgreSQL 完全対応、offline mode |
| utoipa 5 + utoipa-axum 0.2 | ✅ 採用 | Axum 0.8 公式対応確認済み |
| tokio 1 | ✅ 採用 | デファクトスタンダード、LTS あり |
| tracing 0.1 + tracing-subscriber 0.3 | ✅ 採用 | tokio エコシステム標準 |
| tracing-opentelemetry 0.28 | ✅ 採用 | 互換セットに注意するが利用可能 |

## 5. 制約と pitfall

- **utoipa-axum 0.2 は axum 0.8 専用**: axum 0.7 からアップグレードする場合は Router API の breaking changes に注意
- **SQLx offline mode**: `SQLX_OFFLINE=true` 設定中に `cargo sqlx prepare` を実行するとエラー
- **OpenTelemetry バージョン不一致**: tracing-opentelemetry と opentelemetry 本体のバージョンが揃わない場合コンパイルエラーになる。必ず互換セットで指定する
- **MinIO latest タグ**: 本番では日付付きタグ (`RELEASE.2025-09-07T16-13-09Z`) を推奨。開発用は `latest` で可

## 6. 未解決の疑問

- `tracing-opentelemetry` 0.28 が依存する `opentelemetry` の正確なバージョン要件（`^0.27` or `^0.28`）は Cargo.toml を見て実際にビルドで確認する必要がある
- opentelemetry 0.28→0.31 の breaking changes の量（将来のアップグレードコスト）

## 7. Cargo.toml 推奨バージョン一覧

```toml
[workspace.dependencies]
# Web framework
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace"] }

# Database
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "uuid", "chrono", "migrate"] }

# OpenAPI
utoipa = { version = "5", features = ["axum_extras", "uuid", "chrono"] }
utoipa-axum = "0.2"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Observability
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
opentelemetry = "0.28"
opentelemetry_sdk = { version = "0.28", features = ["rt-tokio"] }
opentelemetry-otlp = { version = "0.28", features = ["tonic"] }
tracing-opentelemetry = "0.28"

# Config
dotenvy = "0.15"
```

## 8. 参照URL

- Axum: https://docs.rs/axum/0.8.9 / https://crates.io/crates/axum
- Axum 0.8.0 announcement: https://tokio.rs/blog/2025-01-01-announcing-axum-0-8-0
- SQLx: https://crates.io/crates/sqlx (v0.8.6)
- utoipa: https://crates.io/crates/utoipa (v5.4.0)
- utoipa-axum: https://crates.io/crates/utoipa-axum (v0.2.0)
- utoipa-axum dependencies: https://crates.io/crates/utoipa-axum/dependencies
- Tokio: https://crates.io/crates/tokio (v1.52.0)
- tracing: https://docs.rs/tracing/0.1.44
- tracing-subscriber: https://docs.rs/tracing-subscriber/0.3.23
- tracing-opentelemetry: https://crates.io/crates/tracing-opentelemetry (v0.28.0)
- opentelemetry-rust releases: https://github.com/open-telemetry/opentelemetry-rust/releases
- SQLx offline mode: https://docs.rs/sqlx/latest/sqlx/attr.test.html
