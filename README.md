# Boardflow

## ファイル構成

```text
- crates/: Rust バックエンド workspace
- boardflow/: Next.js フロントエンド
```

## KiCadバージョン

対象とするKiCadバージョンは最新の10.x系(Dockerイメージ: `kicad/kicad:10.0.1`)とする。

## 使用技術

- Rust stable (mise経由で使用)
- pnpm 10.33.2 (mise経由で使用)

## Worker 環境変数

| 変数名 | 必須 | 説明 |
|--------|------|------|
| `DATABASE_URL` | Yes | PostgreSQL接続文字列 |
| `MINIO_BUCKET_STAGING` | No | staging bucket名 (default: `boardflow-staging`) |
| `MINIO_BUCKET_FINAL` | No | final bucket名 (default: `boardflow-artifacts`) |
| `MINIO_ENDPOINT` | No | S3互換エンドポイント |
| `MINIO_ACCESS_KEY` | No | S3アクセスキー |
| `MINIO_SECRET_KEY` | No | S3シークレットキー |
| `POLL_INTERVAL_SECS` | No | ポーリング間隔秒 (default: `2`) |
| `TIMEOUT_SWEEP_INTERVAL_SECS` | No | タイムアウトスイープ間隔秒 (default: `60`) |
| `GITHUB_APP_ID` | No | GitHub App ID。未設定時はGitHub APIジョブをスキップ |
| `GITHUB_PRIVATE_KEY_PEM` | No | GitHub App RSA秘密鍵(PEM)。未設定時はGitHub APIジョブをスキップ |
| `APP_BASE_URL` | No | SaaSベースURL (default: `https://boardflow.example.com`) |

## Frontend ローカル開発

```bash
cd boardflow
cp .env.local.example .env.local   # API_BASE_URL を確認
pnpm install
pnpm dev                            # http://localhost:3000
```

バックエンドが `http://localhost:3001` で起動している前提で、`/api/v1/*` へのリクエストは Next.js rewrites でプロキシされます。

### 主要コマンド

| コマンド | 説明 |
|----------|------|
| `pnpm dev` | 開発サーバー起動 |
| `pnpm build` | プロダクションビルド |
| `pnpm typecheck` | TypeScript 型チェック |
| `pnpm lint` | ESLint 実行 |
| `pnpm generate:api` | Backend OpenAPI spec から型定義を再生成 |
