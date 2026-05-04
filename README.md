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
| `MINIO_BUCKET_FINAL` | No | final bucket名 (default: `boardflow-final`) |
| `MINIO_ENDPOINT` | No | S3互換エンドポイント |
| `MINIO_ACCESS_KEY` | No | S3アクセスキー |
| `MINIO_SECRET_KEY` | No | S3シークレットキー |
| `POLL_INTERVAL_SECS` | No | ポーリング間隔秒 (default: `2`) |
| `TIMEOUT_SWEEP_INTERVAL_SECS` | No | タイムアウトスイープ間隔秒 (default: `60`) |
| `GITHUB_APP_ID` | No | GitHub App ID。未設定時はGitHub APIジョブをスキップ |
| `GITHUB_PRIVATE_KEY_PEM` | No | GitHub App RSA秘密鍵(PEM)。未設定時はGitHub APIジョブをスキップ |
| `BOARDFLOW_APP_DOMAIN` | No | フロントエンドのベースURL (default: `http://localhost:3000`)。OAuth callback と CORS で使用。後方互換として `APP_BASE_URL` も使用可 |

> **Note**: `GITHUB_APP_ID` は API サーバーでも使用されます。設定すると、Webhook 不着時にユーザーの GitHub App user access token（OAuth ログイン時に発行）を使い、Installation Repositories API (`GET /user/installations/{id}/repositories`) 経由でリポジトリ一覧を DB に best-effort 同期するフォールバックが有効になります。この機能を利用するには、GitHub App のユーザー認可フロー（OAuth）が正しく設定されている必要があります。未設定の場合、フォールバック同期は無効ですが他の機能には影響しません。

## GitHub OAuth App 設定

OAuth 認証は Next.js rewrites 経由でフロントエンドドメインを callback 先に使用します。GitHub OAuth App の **Authorization callback URL** は `BOARDFLOW_APP_DOMAIN` と一致させてください。

| 環境 | Callback URL 例 | `BOARDFLOW_APP_DOMAIN` |
|------|----------------|------------------------|
| 開発環境 | `http://localhost:3001/api/v1/auth/callback` | `http://localhost:3001` |
| 本番環境 | `https://app.boardflow.example.com/api/v1/auth/callback` | `https://app.boardflow.example.com` |

- Callback URL のホスト・ポートは `BOARDFLOW_APP_DOMAIN` と完全一致が必要です
- この仕組みは Next.js の `rewrites()` で `/api/v1/*` をバックエンド API にプロキシしていることが前提です
- `BOARDFLOW_APP_DOMAIN` が `https://` の場合、Cookie に `Secure` フラグが自動付与されます
- ローカル開発では API (port 3000) とフロントエンド (port 3001) が別ポートのため、`.env` に `BOARDFLOW_APP_DOMAIN=http://localhost:3001` を設定してください

## Frontend ローカル開発

バックエンドAPI起動には `DATABASE_URL` 等の環境変数が必要です。`docker-compose.yml` で依存サービスを起動し、ルートディレクトリに `.env` を用意してください。

```bash
cd boardflow
cp .env.local.example .env.local   # API_BASE_URL を確認
pnpm install

# バックエンドAPI (port 3000) を先に起動しておく（別ターミナル、要 .env）
mise exec -- cargo run -p boardflow-api

# フロントエンド開発サーバー起動（別ターミナル）
pnpm dev --port 3001               # http://localhost:3001
```

バックエンドが `http://localhost:3000` で起動している前提で、`/api/v1/*` へのリクエストは Next.js rewrites でプロキシされます。Next.js はデフォルトでポート 3000 を使用しますが、バックエンドと衝突するため `--port 3001` で別ポートを指定してください。

### 主要コマンド

| コマンド | 説明 |
|----------|------|
| `pnpm dev` | 開発サーバー起動 |
| `pnpm build` | プロダクションビルド |
| `pnpm typecheck` | TypeScript 型チェック |
| `pnpm lint` | ESLint 実行 |
| `pnpm generate:api` | Backend OpenAPI spec から型定義を再生成 |

### API型定義の再生成

バックエンドのOpenAPI定義が変更された場合、以下の手順で `schema.d.ts` を再生成してください。APIサーバ起動には `DATABASE_URL` 等の環境変数と依存サービス（PostgreSQL、MinIO）が必要です。

```bash
# 1. APIサーバを起動（別ターミナル、要 .env + docker-compose up）
mise exec -- cargo run -p boardflow-api

# 2. APIサーバが起動したことを確認
curl -s http://localhost:3000/api/v1/openapi.json | head -c 100

# 3. 型定義を再生成
cd boardflow
pnpm generate:api

# 4. 型チェック
pnpm typecheck
```

`schema.d.ts` は自動生成ファイルのため、手動編集しないでください。
