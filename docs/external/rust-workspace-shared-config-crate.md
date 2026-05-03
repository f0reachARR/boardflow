# Rust Workspace 共通 Config Crate のベストプラクティス

## 要約

Cargo workspace内でenv設定を共通化するために、`crates/config/` のような共通crateを追加し、API・Workerの両方から依存させるパターンを調査した。外部ライブラリの新規導入は不要で、`std::env::var` を使った既存パターンの統合で十分。

## 確認した情報

### Cargo workspace への crate 追加方法

1. `crates/config/` ディレクトリを作成し、`Cargo.toml` を配置
2. ルート `Cargo.toml` の `[workspace] members` に `"crates/config"` を追加
3. 依存元 (`crates/api`, `crates/worker`) の `Cargo.toml` に `boardflow-config = { path = "../config" }` を追加

これはプロジェクト既存パターン（`boardflow-db`, `boardflow-domain` 等）と同じ。

### 設計パターン

**共通crateで提供するもの：**
- `DatabaseConfig`: `DATABASE_URL`（必須）
- `S3Config`: endpoint, access_key, secret_key, staging_bucket, final_bucket
- `ConfigError`: 統一されたエラー型（MissingEnvVar, InvalidValue）
- ヘルパー関数: `required_env(name)`, `optional_env(name)`, `optional_env_or(name, default)`, `parse_env_or(name, default)`

**各crateで残すもの：**
- `AppConfig` (api固有): api_host, api_port, rust_log, OAuth関連, session/artifact secret, webhook secret
- `WorkerConfig` (worker固有): poll_interval_secs, timeout_sweep_interval_secs, github_app_id, github_private_key_pem

### エラーハンドリング方針

統一して `Result<T, ConfigError>` を返す。`expect()` / `panic!` は使わない。
- API: 既存が `Result` → そのまま
- Worker: 既存が `expect` → `Result` に移行、`main()` 側で `.expect()` するか `anyhow` でハンドル

## BoardFlow への示唆

### 現在の問題点

1. **デフォルト値の不整合（バグ）**: `MINIO_BUCKET_FINAL` のデフォルト値が異なる
   - `AppConfig`: `"boardflow-final"`
   - `WorkerConfig`: `"boardflow-artifacts"`
   → 環境変数未設定時に API と Worker が異なるバケットを参照する

2. **フィールド名の不統一**: 同じ環境変数を読み取るフィールド名が異なる
   - `minio_bucket_staging` (API) vs `staging_bucket` (Worker)
   - `minio_bucket_final` (API) vs `artifacts_bucket` (Worker)
   - `minio_endpoint` (API) vs `s3_endpoint` (Worker)
   - `minio_access_key` (API) vs `s3_access_key` (Worker)

3. **env読み取りの散在**: `config.rs` 以外に `lib.rs`, `routes/board_run.rs` でも直接 `std::env::var` を呼んでいる

4. **関連するURLの不統一**: `BOARDFLOW_APP_DOMAIN`(API) vs `APP_BASE_URL`(Worker)。目的は近いが環境変数名が異なる。

5. **`.env.example` の不完全性**: `BOARDFLOW_ARTIFACT_SECRET`, `GITHUB_CLIENT_ID/SECRET`, `GITHUB_WEBHOOK_SECRET`, `GITHUB_APP_ID`, `GITHUB_PRIVATE_KEY_PEM`, `APP_BASE_URL` 等が `.env.example` に含まれていない

## 採用/不採用判断

- **共通 config crate 新設**: 採用
  - 名前: `boardflow-config` (`crates/config/`)
  - 既存パターン (`boardflow-db` 等) と一致
  - 共通部分（DB, S3, ヘルパー）を集約
- **外部ライブラリ (config, figment, dotenvy等)**: 不採用
  - 現在の規模では `std::env::var` + ヘルパー関数で十分
  - 既存コードとの一貫性維持

## 制約と pitfall

- `MINIO_BUCKET_FINAL` のデフォルト値統一時、既存のWorkerデプロイが `boardflow-artifacts` に依存している可能性あり。`boardflow-final` に統一する場合は注意
- `BOARDFLOW_APP_DOMAIN` と `APP_BASE_URL` は目的が異なる可能性あり（前者はCORS/CSP用、後者はGitHubコメントURL用）。統合するか別々にするかはユーザー判断
- `lib.rs` 内の `create_app_with_config()` のfallback env読み取りは、テスト用の柔軟性のために意図的に残されている可能性がある

## 未解決の疑問

1. `BOARDFLOW_APP_DOMAIN` と `APP_BASE_URL` は統合すべきか、別目的として分離すべきか？
2. `create_app_with_config()` 内のenv fallbackは config crate に移行すべきか、テスト用に残すべきか？

## 参照URL

- Cargo workspace ドキュメント: https://doc.rust-lang.org/cargo/reference/workspaces.html
- `std::env::var` ドキュメント: https://doc.rust-lang.org/std/env/fn.var.html
