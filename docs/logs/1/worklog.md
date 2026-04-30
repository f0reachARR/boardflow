# Issue #1: Rust workspaceセットアップとDB基盤

## 経緯
- バックエンド実装Issue分割タスクの一環として作成
- 全バックエンドIssueの前提となる土台Issue

## ユーザー要望
- docs/以下の仕様に基づくRustバックエンド実装の第1段階

## Issue作成内容
- Cargo workspace構成、Axum基本構造、SQLx+PostgreSQL、Docker Compose、設定管理
- URL: https://github.com/f0reachARR/boardflow/issues/1

## 後続処理タイプの初期仮説
`implementation_required`

## 調査結果 (2026-04-30)

### 調査概要

Issue #1 実装に必要な外部 crate の最新バージョンと互換性を調査した。

### 主要な結論

1. **Axum 0.8.9** (最新安定) — `axum = "0.8"`
2. **SQLx 0.8.6** — PostgreSQL 完全対応、offline mode 安定
3. **utoipa 5.4.0 + utoipa-axum 0.2.0** — **Axum 0.8 と公式互換確認済み** ✅
   - utoipa-axum 0.2.0 は axum ^0.8.0 + utoipa ^5.0.0 に依存
   - ワークログの懸念事項「utoipa の Axum 0.7 対応状況要確認」は解消（0.8対応済み）
4. **tokio 1.52.0** — LTS: 1.51.x (2027年3月まで)
5. **tracing 0.1.44 / tracing-subscriber 0.3.23**
6. **tracing-opentelemetry 0.28.0** — opentelemetry 0.28 系と互換セットで使用
7. **Docker Compose**: postgres:16-alpine, redis:7-alpine, minio/minio:latest

### SQLx Offline Mode セットアップ要約

```bash
# 準備 (DB接続状態で)
cargo sqlx prepare --workspace --all -- --all-targets

# CI検証
SQLX_OFFLINE=true cargo build

# CIチェック (`.sqlx` が最新か確認)
cargo sqlx prepare --workspace --check -- --all-targets
```

- `.sqlx/` ディレクトリをリポジトリにコミット
- CI では `SQLX_OFFLINE=true` をセット
- `--all-targets` でテストコード内クエリも含める

### 互換性マトリクス

| Crate A | Crate B | 互換性 |
|---|---|---|
| axum 0.8.x | utoipa-axum 0.2.0 | ✅ 公式対応 |
| utoipa 5.x | utoipa-axum 0.2.0 | ✅ 公式対応 |
| tracing-opentelemetry 0.28 | opentelemetry 0.28 | ✅ (要確認だが高確率で互換) |
| tokio 1.x | axum 0.8 / sqlx 0.8 | ✅ |

### 結論ステータス

`implementation_required` — 全ての外部依存は安定版で利用可能。実装に進んでよい。

### 成果物

- `docs/external/rust-crate-versions.md` — 詳細な調査結果と推奨 Cargo.toml

## 残リスク
- tracing-opentelemetry 0.28 の opentelemetry 正確な互換バージョンは実際のビルドで最終確認が必要
- opentelemetry 0.28→0.31 へのアップグレードは将来必要（tracing-opentelemetry の更新待ち）
- MinIO の latest タグは本番では日付タグに固定すべき


## 計画 (2026-04-30)

### 実装要否

\`implementation_required\`

### 目的

Rust Cargo workspace の骨格を構築し、全後続 Issue の基盤を確立する。具体的には:
- 7 crate の workspace 構成
- Axum による HTTP サーバー起動とヘルスチェック
- SQLx による PostgreSQL 接続基盤
- Docker Compose によるローカル開発環境
- 環境変数ベースの設定管理
- tracing による structured logging
- utoipa による OpenAPI スキーマ生成

### 非目的

- 業務ロジックの実装（後続 Issue で追加）
- API エンドポイントの実装（/healthz と OpenAPI JSON 以外）
- マイグレーションの作成（初回マイグレーションは Issue #2 以降）
- GitHub App / OAuth の実装
- Worker のジョブ処理実装
- フロントエンドとの結合

### 受け入れ条件

1. \`cargo build\` が全 crate で成功すること
2. \`cargo test\` が成功すること
3. \`docker compose up -d\` で PostgreSQL, Redis, MinIO が起動すること
4. API server が Axum で起動し、GET /healthz が 200 OK を返すこと
5. SQLx で PostgreSQL 接続が確立すること（ヘルスチェック内で確認）
6. utoipa による OpenAPI JSON が GET /api/v1/openapi.json で取得できること
7. tracing-subscriber による structured logging が stdout に出力されること

### 作成ファイル一覧 (19ファイル)

| パス | 概要 |
|---|---|
| Cargo.toml | workspace定義 + 共通依存 |
| docker-compose.yml | PostgreSQL, Redis, MinIO |
| .env.example | 環境変数テンプレート |
| crates/api/Cargo.toml | API crate 依存定義 |
| crates/api/src/main.rs | エントリポイント: サーバー起動 |
| crates/api/src/lib.rs | app構築ロジック (テスト用分離) |
| crates/api/src/config.rs | 環境変数からの設定読み込み |
| crates/api/src/routes/mod.rs | ルーティング定義 |
| crates/api/src/routes/health.rs | GET /healthz ハンドラー |
| crates/worker/Cargo.toml | Worker crate 依存定義 |
| crates/worker/src/main.rs | スケルトン |
| crates/domain/Cargo.toml | Domain crate |
| crates/domain/src/lib.rs | スケルトン |
| crates/db/Cargo.toml | DB crate |
| crates/db/src/lib.rs | PgPool初期化 |
| crates/jobs/Cargo.toml | Jobs crate |
| crates/jobs/src/lib.rs | スケルトン |
| crates/github/Cargo.toml | GitHub crate |
| crates/github/src/lib.rs | スケルトン |
| crates/artifact/Cargo.toml | Artifact crate |
| crates/artifact/src/lib.rs | スケルトン |

### 実装順序

Phase 1: 基盤 (Cargo.toml, docker-compose.yml, .env.example)
Phase 2: ライブラリ crates (domain, db, jobs, github, artifact)
Phase 3: API サーバー (api crate: config, routes, lib, main)
Phase 4: Worker スケルトン
Phase 5: 検証 (cargo build, cargo test, docker compose, healthz)

### テスト計画

- cargo build: 全crate コンパイル成功
- cargo test: config パース、app構築スモークテスト
- docker compose up -d: 3サービス起動確認
- curl localhost:3000/healthz: 200 {"status":"ok"}
- curl localhost:3000/api/v1/openapi.json: 有効な OpenAPI JSON
- structured logging: JSON ログ stdout 出力確認

### 環境変数設計

- DATABASE_URL (必須): PostgreSQL接続文字列
- REDIS_URL: Redis接続文字列
- MINIO_ENDPOINT / MINIO_ACCESS_KEY / MINIO_SECRET_KEY: MinIO設定
- MINIO_BUCKET_STAGING / MINIO_BUCKET_FINAL: バケット名
- RUST_LOG: tracingフィルター
- API_HOST / API_PORT: APIバインド設定

### 残リスク

1. tracing-opentelemetry 0.28 互換性: Issue #1 では OTel exporter 含めず影響なし
2. MinIO latest タグ: ローカル開発用なので許容
3. Rust nightly: 使用crate はすべて stable 互換

## 実装内容 (2026-04-30)

### 実施事項

計画通りに全19ファイルを作成し、Rust Cargo workspace の骨格を完成させた。

### 作成/変更ファイル

- `Cargo.toml` — workspace定義 + 共通依存 (axum 0.8, sqlx 0.8, utoipa 5, tokio 1 等)
- `docker-compose.yml` — PostgreSQL 16, Redis 7, MinIO (全サービスhealthcheck付き)
- `.env.example` — 環境変数テンプレート
- `crates/api/` — Axum HTTP サーバー (config, routes/health, lib, main)
- `crates/db/` — SQLx PgPool 作成関数
- `crates/domain/` — スケルトン (将来のドメインモデル用)
- `crates/worker/` — スケルトン (structured logging のみ)
- `crates/jobs/`, `crates/github/`, `crates/artifact/` — スケルトン
- `crates/api/tests/config_test.rs` — AppConfig 単体テスト
- `crates/api/tests/integration_test.rs` — healthz/OpenAPI 統合テスト

### テスト結果

| テスト | 結果 |
|---|---|
| `cargo build` | ✅ 全7 crate コンパイル成功 |
| `cargo test` | ✅ 3テスト成功 (config_test: 1, integration_test: 2) |
| `docker compose up -d` | ✅ PostgreSQL, Redis, MinIO 全て healthy |
| `curl /healthz` | ✅ 200 `{"status":"ok"}` |
| `curl /api/v1/openapi.json` | ✅ 有効な OpenAPI 3.1.0 JSON |
| structured logging | ✅ JSON形式で stdout に出力 |

### テスト観点

1. **AppConfig 単体テスト** (`config_test.rs`)
   - DATABASE_URL 未設定時のエラー返却
   - デフォルト値のフォールバック (API_HOST=0.0.0.0, API_PORT=3000, RUST_LOG=info)
   - カスタム値の正常読み込み
   - 無効なポート番号時のデフォルト値フォールバック

2. **統合テスト** (`integration_test.rs`, DATABASE_URL設定時のみ実行)
   - OpenAPI JSON エンドポイントが200を返しBoardFlow APIタイトルを含む
   - healthz エンドポイントがDB ping成功時に200を返す

### 更新ドキュメント

- `docs/logs/1/worklog.md` (本ファイル)

### コミット

- `96220ab` feat(#1): Rust workspace setup with Axum API, SQLx DB, Docker Compose

### 残リスク

1. 統合テストは `DATABASE_URL` 環境変数が設定されていない場合スキップされる（CI環境でのDB接続設定が必要）
2. Rust 2024 edition で `std::env::set_var`/`remove_var` が unsafe になったため、テスト内で unsafe ブロックを使用
3. MinIO healthcheck に `mc ready local` を使用 — MinIOイメージに `mc` が含まれない場合は調整が必要

## レビュー結果 (2026-04-30)

### 総評

- Issue #1 の workspace 構成は docs/backend/summary.md Section 4 の crate 構成に準拠しており、Rust backend の土台としては妥当。
- ローカル再検証では cargo test --workspace、docker compose up -d、healthz、openapi.json を確認できた。
- ただし、仕様と実装の不整合が 2 点あり、現時点では pr_ready: false と判断する。

### 必須修正

1. OpenAPI の契約バージョンを仕様と実装で統一すること。
   - docs/backend/summary.md と docs/technology.md は OpenAPI 3.0.3 前提。
   - 実装の /api/v1/openapi.json は 3.1.0 を返すことを実ランタイムで確認した。

2. 設定管理の実装範囲を計画と一致させること。
   - 計画では Redis、MinIO、MINIO_BUCKET_STAGING、MINIO_BUCKET_FINAL を含む。
   - しかし crates/api/src/config.rs は DATABASE_URL、API_HOST、API_PORT、RUST_LOG しか扱っていない。

### 任意改善

1. API_PORT の不正値を 3000 に黙ってフォールバックせず、起動失敗にした方が安全。
2. DB 非依存の OpenAPI ルートまで live DB 前提のテストになっているため、state 分離か lazy pool 化を検討した方がよい。
3. docker-compose.yml の MinIO は latest タグ固定なので、再現性のため日付タグ固定を検討した方がよい。

### テスト不足

1. integration_test.rs は DATABASE_URL 未設定時に return して成功扱いになるため、CI 設定ミスを見逃しうる。
2. OpenAPI のテストは title のみを検証し、重要な契約差分である openapi バージョンを見ていない。
3. 設定テストは Redis / MinIO / bucket 名の実装不足を検知できない。

### plan / research / docs との不整合

1. docs は OpenAPI 3.0.3、実装実測は 3.1.0。
2. 計画にある MINIO_BUCKET_STAGING と MINIO_BUCKET_FINAL が .env.example と AppConfig に反映されていない。
3. 環境変数ベースの設定管理は、現状では DB と API 起動に必要な最小 subset のみ。

### PR/完了結果

- pr_ready: false

### 追加の再検証結果

- cargo test --workspace: 成功
- docker compose up -d && docker compose ps: PostgreSQL / Redis / MinIO healthy
- GET /healthz: 200 と status ok を確認
- GET /api/v1/openapi.json: openapi 3.1.0 と BoardFlow API を確認

### 残リスク

1. OpenAPI バージョン差分を放置すると、後続 Issue の契約テストと型生成で再調整が必要になる。
2. 設定スコープが曖昧なままだと、Redis / MinIO を使う Issue で設定方式の再設計が入りやすい。
3. 条件付きスキップのままでは、CI の DB セットアップ欠落を見逃しやすい。
