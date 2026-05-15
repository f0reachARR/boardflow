# Issue #96 作業ログ: command系handlerからservice層への切り出し

## Issueまでの経緯

- Issue #96: `plan/board_run系API handlerからユースケース処理をservice層へ切り出す`
- #98 で `pagination.rs` の共通モジュール抽出が完了
- #99 で `read.rs` → `read/` ディレクトリへの分割が完了 (PR #120)
- 上記2件でread系・共通ユーティリティのリファクタリングが終わり、次はcommand系handlerのservice層分離

## ユーザー要望

- 挙動変更は避け、純粋なコード移動・抽出に留める
- handler はHTTP境界に集中させ、ユースケース本体をservice moduleに抽出

---

## 調査フェーズ (2026-05-15)

### 1. `crates/api/src/` ディレクトリツリー

```
crates/api/src/
├── artifact_token.rs
├── config.rs
├── error.rs
├── extractors/
│   ├── auth.rs
│   ├── mod.rs
│   └── session.rs
├── github_access/
│   ├── cached.rs
│   ├── installation_sync.rs
│   ├── mod.rs
│   ├── real.rs
│   ├── test_doubles.rs
│   └── types.rs
├── lib.rs
├── main.rs
├── middleware/
├── pagination.rs          ← #98 で抽出済み
├── routes/
│   ├── api_token.rs       ← command系 (POST create, POST revoke) + read系 (GET list)
│   ├── auth.rs
│   ├── board_run.rs       ← command系 (POST create, POST fail, POST import)
│   ├── health.rs
│   ├── mod.rs
│   ├── plan.rs            ← command系 (POST plan_run)
│   ├── proxy.rs
│   ├── read/              ← #99 で分割済み (10ファイル)
│   │   ├── mod.rs, access.rs, dto.rs, repositories.rs,
│   │   ├── board_projects.rs, board_runs.rs, artifacts.rs,
│   │   ├── viewer_sources.rs, diff.rs, findings.rs
│   └── webhook.rs
```

**service module は存在しない。** `crates/api/src/services/` や類似のディレクトリは未作成。

### 2. command系handlerファイル一覧

| ファイル | 行数 | 関数 |
|---------|------|------|
| `routes/plan.rs` | 209行 | `plan_run` (POST /api/v1/runs/plan) |
| `routes/board_run.rs` | 551行 | `create_board_run`, `fail_board_run`, `import_artifact_bundle` + `generate_upload_info`(private helper), `bundle_status`(private helper) |
| `routes/api_token.rs` | 291行 | `create_api_token`, `list_api_tokens`, `revoke_api_token` + `generate_raw_token`, `hash_token`(private helpers) |

**合計: 1,051行** (3ファイル)

### 3. 各handler内の「HTTP境界」と「ユースケースロジック」の境界分析

#### `plan_run` (plan.rs, 209行)

| 行範囲 | 処理内容 | 分類 |
|--------|---------|------|
| L29-35 | axum extractors (auth, request_id, pool, payload) + JSON parse | HTTP境界 |
| L37-64 | github_repository_id parse, 認証チェック, repository lookup → 403/500 | **ユースケース** (認証含む) |
| L66-80 | repository upsert | **ユースケース** |
| L82-91 | バリデーション (duplicate path検出) | **ユースケース** |
| L93-193 | プロジェクトごとの plan 判定ループ (validation + DB upsert + decision) | **ユースケース** |
| L195-209 | レスポンス構築 → `Json(PlanResponse)` | HTTP境界 |

**ポイント**: ユースケースロジックが handler の大部分 (~170行) を占める。extractorとレスポンス構築が薄い。

#### `create_board_run` (board_run.rs)

| 行範囲 | 処理内容 | 分類 |
|--------|---------|------|
| L89-97 | extractors (auth, request_id, pool, s3_client, staging_bucket, payload) + JSON parse | HTTP境界 |
| L99-189 | board_project lookup, 認証チェック, 数値パース, idempotency check, 既存run返却 | **ユースケース** |
| L191-229 | new BoardRun insert, ArtifactBundle insert, presigned URL生成 | **ユースケース** |
| L231-235 | レスポンス構築 | HTTP境界 |

#### `fail_board_run` (board_run.rs)

| 行範囲 | 処理内容 | 分類 |
|--------|---------|------|
| extractors + parse | HTTP境界 |
| board_run lookup → 認証 → status check → mark_failed | **ユースケース** |
| レスポンス構築 | HTTP境界 |

#### `import_artifact_bundle` (board_run.rs)

| 行範囲 | 処理内容 | 分類 |
|--------|---------|------|
| extractors + parse | HTTP境界 |
| トランザクション開始, FOR UPDATE lock, 認証, status check, idempotency/conflict check, bundle upsert, mark_importing, enqueue job, commit | **ユースケース** (最も複雑) |
| レスポンス構築 | HTTP境界 |

#### `create_api_token` / `revoke_api_token` (api_token.rs)

| 処理内容 | 分類 |
|---------|------|
| extractors + parse | HTTP境界 |
| repository lookup, access check, token生成/revoke | **ユースケース** |
| レスポンス構築 | HTTP境界 |

### 4. 既存のservice moduleの有無

**存在しない。** 現在のアーキテクチャはhandler内に全てのユースケースロジックが埋め込まれている。

関連する分離パターンとしては:
- `pagination.rs`: 共通ユーティリティモジュール (#98で抽出)
- `read/access.rs`: access check のヘルパー関数 (`access_result_to_error`)
- `github_access/`: GitHub アクセスチェックのtrait + impl (既にinterface分離済み)
- `extractors/`: axum extractorの分離 (既にmodule化済み)

### 5. テストファイルの構造

| ファイル | テスト数 | テスト方式 |
|---------|---------|-----------|
| `board_run_test.rs` | 19 | `create_app()` → `tower::ServiceExt::oneshot()` でHTTPリクエスト送信 |
| `plan_test.rs` | (同様) | 同上 |
| `api_token_test.rs` | 15 | 同上 |

全テストが **HTTPレベルの結合テスト** で、handlerを直接呼び出すのではなく `create_app()` でRouter全体を構築し、HTTPリクエストを送信している。

→ **service層を切り出しても既存テストは修正不要** (handler署名・ルーティングが変わらない限り)

### 6. #98 / #99 のリファクタリングパターン

#### #98 (pagination.rs抽出)
- `read.rs` と `api_token.rs` に重複していたcursor encode/decode, `PaginationParams`, `PaginatedResponse` を `crates/api/src/pagination.rs` に抽出
- `pub(crate)` スコープで公開
- 元のファイルから `use crate::pagination::*` でインポート

#### #99 (read.rs分割)
- 1ファイル1679行 → `read/` ディレクトリ (10ファイル) に分割
- `mod.rs` で `pub use` 再エクスポート → 外部からのインポートパスは変更なし
- 機能ごとにファイルを分離 (repositories, board_projects, board_runs, artifacts, etc.)
- DTOは `dto.rs` に、アクセス制御ヘルパーは `access.rs` に分離

### 7. 実装に向けた推奨パターン

#### A. serviceモジュール構造

```
crates/api/src/
├── services/
│   ├── mod.rs
│   ├── plan.rs          ← plan_run のユースケースロジック
│   ├── board_run.rs     ← create/fail/import のユースケースロジック
│   └── api_token.rs     ← create/revoke のユースケースロジック (list はread系なので要検討)
```

#### B. 分離方針

1. **service関数のシグネチャ**: `PgPool` (+ S3 client等) を直接受け取り、domain型を返す。`AppError` を返しても良い（HTTP status codeの決定はhandler側で行うのが理想だが、既存の `AppError` がstatus codeを含んでいるため、段階的に分離する）
2. **handler の責務**: extractorでのパース → service呼び出し → レスポンスDTO構築
3. **private helper** (`generate_upload_info`, `bundle_status`, `generate_raw_token`, `hash_token`) はservice側に移動
4. **トランザクション管理**: `import_artifact_bundle` のトランザクションはservice側に含める (DBの一貫性保証はユースケースの責務)

#### C. 注意点

- `#[utoipa::path]` アトリビュートはhandler関数に残す (OpenAPI生成に必要)
- `openapi_router()` のルーティング登録は変更不要 (handler関数名が変わらないため)
- テストは全てHTTPレベルなので修正不要
- `api_token.rs` の `list_api_tokens` はread系だが、同じファイル内にあるため、service分離の際にcommand系だけ先に切り出すか、ファイルごと移行するか判断が必要

#### D. 優先順位

1. `board_run.rs` (551行, 最も複雑) → 最大の効果
2. `plan.rs` (209行) → 自己完結していて分離しやすい
3. `api_token.rs` (291行) → command/readが混在

### 結論ステータス

**`implementation_required`**

- 外部ライブラリの調査は不要 (純粋な内部リファクタリング)
- axumのhandler/service分離は標準的なパターンで、既存の依存関係で十分対応可能
- 既存テストがHTTPレベルのため、service層への切り出しによるテスト修正は不要
- #98/#99 のパターンに倣い、段階的にモジュールを分離する方針が明確

### 残リスク

- `AppError` がHTTPステータスコードを含むため、service層でも `AppError` を返す形になる（理想的にはdomain errorを返してhandlerで変換すべきだが、このリファクタリングのスコープ外）
- `import_artifact_bundle` はトランザクション管理が絡むため、`PgPool` のライフタイムとトランザクション境界に注意が必要
- `api_token.rs` の `list_api_tokens` をどう扱うか (command系だけ切り出すか、ファイルごと移行するか) の判断が必要

### 参照URL

---

## 計画フェーズ (2026-05-15)

### 概要

`routes/plan.rs`, `routes/board_run.rs`, `routes/api_token.rs` の command 系 handler からユースケースロジック（認証チェック・DB操作・バリデーション・ビジネスロジック）を `crates/api/src/services/` モジュールに抽出する。handler は extractor でのパース + service 呼び出し + レスポンス DTO 構築のみに留め、HTTP 境界に集中させる。`AppError` はそのまま service 層でも使用し、domain error 分離は将来の課題とする。全ステップでコンパイル・テスト通過を維持する。

### 設計方針

#### エラー型: `AppError` をそのまま利用
- 既存の `AppError` は `ErrorCode` (semantic) + `message` + `request_id` で構成されており、HTTP status code は `IntoResponse` 実装時に `ErrorCode::status_code()` で決まる
- service 関数は `request_id: &str` を受け取り、`AppError` を返す
- 理想的には domain error → handler で変換が望ましいが、**純粋なリファクタリング** のスコープでは過度な設計変更を避ける

#### service 関数のインタフェース規約
- service 関数は axum の型（`Extension`, `State`, `Path`, `Json` など）に一切依存しない
- 引数は素の Rust 型（`&PgPool`, `&str`, `i64`, domain struct など）
- 返り値は `Result<DomainType, AppError>` (一部は `Result<(StatusCode, DomainType), AppError>` にしない — StatusCode 決定は handler 側)

#### `list_api_tokens` の扱い
- `list_api_tokens` は GET（read 系）だが、`create_api_token` / `revoke_api_token` と同じファイル内に存在する
- 今回のスコープは command 系切り出しのため、`list_api_tokens` も含めてファイルごと services に移行する (ファイルを分断すると `CreateApiTokenRequest` 等の型の所在が不自然になるため)

### ファイル変更一覧

| パス | 変更種別 | 変更内容 |
|------|---------|---------|
| `crates/api/src/services/mod.rs` | **新規作成** | `pub mod plan; pub mod board_run; pub mod api_token;` |
| `crates/api/src/services/plan.rs` | **新規作成** | `plan_run` のユースケースロジック関数 `execute_plan_run` |
| `crates/api/src/services/board_run.rs` | **新規作成** | `execute_create_board_run`, `execute_fail_board_run`, `execute_import_artifact_bundle` + private helpers (`generate_upload_info`, `bundle_status`) |
| `crates/api/src/services/api_token.rs` | **新規作成** | `execute_create_api_token`, `execute_list_api_tokens`, `execute_revoke_api_token` + private helpers (`generate_raw_token`, `hash_token`) |
| `crates/api/src/lib.rs` | **修正** | `pub mod services;` 追加 |
| `crates/api/src/routes/plan.rs` | **修正** | ユースケースロジックを `services::plan::execute_plan_run` 呼び出しに置換。handler は extractor parse + service 呼び出し + `Json()` wrap のみ |
| `crates/api/src/routes/board_run.rs` | **修正** | 3 handler を service 呼び出しに置換。private helpers 削除 |
| `crates/api/src/routes/api_token.rs` | **修正** | 3 handler を service 呼び出しに置換。private helpers・Request/Response 型は service 側に移動 |

### Service関数シグネチャ設計

#### `services/plan.rs`

```rust
use boardflow_api_types::plan::*;
use crate::error::AppError;
use sqlx::PgPool;

pub async fn execute_plan_run(
    pool: &PgPool,
    repository_id: uuid::Uuid,  // auth.0.repository_id
    req: PlanRequest,
    request_id: &str,
) -> Result<PlanResponse, AppError>
```

- handler から `auth.0.repository_id` と parse 済みの `PlanRequest` を受け取る
- 認証チェック（repository lookup + github_repository_id 照合）、upsert、plan 判定ロジック全てを含む
- `PlanResponse` を返し、handler は `Ok(Json(response))` するだけ

#### `services/board_run.rs`

```rust
use boardflow_api_types::board_run::*;
use crate::error::AppError;
use sqlx::PgPool;
use uuid::Uuid;

/// create_board_run のユースケース
pub async fn execute_create_board_run(
    pool: &PgPool,
    s3_client: &Option<aws_sdk_s3::Client>,
    staging_bucket: &str,
    repository_id: Uuid,           // auth.0.repository_id
    req: CreateBoardRunRequest,
    request_id: &str,
) -> Result<CreateBoardRunResponse, AppError>

/// fail_board_run のユースケース
pub async fn execute_fail_board_run(
    pool: &PgPool,
    repository_id: Uuid,
    board_run_id_str: &str,
    req: FailBoardRunRequest,
    request_id: &str,
) -> Result<FailBoardRunResponse, AppError>

/// import_artifact_bundle のユースケース
pub async fn execute_import_artifact_bundle(
    pool: &PgPool,
    repository_id: Uuid,
    installation_id: i64,          // auth.0.installation_id
    board_run_id_str: &str,
    req: ImportArtifactBundleRequest,
    request_id: &str,
) -> Result<ImportArtifactBundleResponse, AppError>

// private helpers (service内部)
fn bundle_status(status: ArtifactBundleStatus) -> ImportArtifactBundleStatus
async fn generate_upload_info(...) -> Result<ArtifactBundleInfo, AppError>
```

- `import_artifact_bundle` は `installation_id` も必要 (enqueue_import で使用)
- トランザクション管理 (`pool.begin()`) は service 内部

#### `services/api_token.rs`

```rust
use crate::error::AppError;
use crate::github_access::DynGithubAccessChecker;
use crate::pagination::{PaginatedResponse, PaginationParams};
// Request/Response 型はこのファイル内に定義を移動
use sqlx::PgPool;

pub async fn execute_create_api_token(
    pool: &PgPool,
    access_checker: &DynGithubAccessChecker,
    github_access_token: &str,    // session.user.github_access_token
    github_repository_id: i64,
    name: &str,                   // parse 済みの body.name
    request_id: &str,
) -> Result<CreateApiTokenResponse, AppError>

pub async fn execute_list_api_tokens(
    pool: &PgPool,
    access_checker: &DynGithubAccessChecker,
    github_access_token: &str,
    github_repository_id: i64,
    params: &PaginationParams,
    request_id: &str,
) -> Result<PaginatedResponse<ApiTokenListItem>, AppError>

pub async fn execute_revoke_api_token(
    pool: &PgPool,
    access_checker: &DynGithubAccessChecker,
    github_access_token: &str,
    github_repository_id: i64,
    token_id_str: &str,
    request_id: &str,
) -> Result<ApiTokenDetailResponse, AppError>

// private helpers
fn generate_raw_token() -> String
fn hash_token(raw_token: &str) -> String
```

- Request/Response 型 (`CreateApiTokenRequest`, `CreateApiTokenResponse`, `ApiTokenListItem`, `ApiTokenDetailResponse`) は `services/api_token.rs` に移動し、`routes/api_token.rs` から `use crate::services::api_token::*` でインポート
- `access_result_to_error` は既存の `routes::read::access` から `use crate::routes::read::access_result_to_error` として service からも利用可能 (既に `pub` で re-export 済み)

### 実装ステップ

#### Step 1: services モジュール骨格作成 + `plan.rs` 抽出

**何をするか**: 最も単純な `plan_run` から始めて、service モジュールの骨格を確立する

**ファイル変更**:
1. `crates/api/src/services/mod.rs` 新規作成: `pub mod plan;`
2. `crates/api/src/services/plan.rs` 新規作成: `execute_plan_run` 関数 (現在の `plan_run` の L37-209 のロジック部分を移動)
3. `crates/api/src/lib.rs` 修正: `pub mod services;` 追加
4. `crates/api/src/routes/plan.rs` 修正: ユースケースロジックを `crate::services::plan::execute_plan_run(...)` 呼び出しに置換

**確認ポイント**: `cargo fmt`, `cargo clippy`, `cargo test --workspace` 全通過

#### Step 2: `board_run.rs` のservice抽出

**何をするか**: 最も複雑な `board_run.rs` の 3 handler + 2 private helpers を service に抽出

**ファイル変更**:
1. `crates/api/src/services/mod.rs` 修正: `pub mod board_run;` 追加
2. `crates/api/src/services/board_run.rs` 新規作成: `execute_create_board_run`, `execute_fail_board_run`, `execute_import_artifact_bundle` + private helpers
3. `crates/api/src/routes/board_run.rs` 修正: 3 handler をそれぞれ service 呼び出しに置換。private helpers (`generate_upload_info`, `bundle_status`) 削除

**注意点**:
- `generate_upload_info` は `AppError::internal_error("...", "")` と空の request_id で呼んでいる — service 化時に `request_id` を渡すように改善
- `import_artifact_bundle` のトランザクション管理は service 関数内に含める
- `StagingBucket` の Extension は handler で取り出して `&str` として service に渡す

**確認ポイント**: `cargo fmt`, `cargo clippy`, `cargo test --workspace` 全通過。特に `board_run_test.rs` (19件) が全通過すること

#### Step 3: `api_token.rs` のservice抽出

**何をするか**: `api_token.rs` の 3 handler + 2 private helpers + Request/Response 型を service に抽出

**ファイル変更**:
1. `crates/api/src/services/mod.rs` 修正: `pub mod api_token;` 追加
2. `crates/api/src/services/api_token.rs` 新規作成: `execute_create_api_token`, `execute_list_api_tokens`, `execute_revoke_api_token` + private helpers + Request/Response 型定義
3. `crates/api/src/routes/api_token.rs` 修正: handler を service 呼び出しに置換。Request/Response 型を `use crate::services::api_token::*` に変更。private helpers 削除

**注意点**:
- Request/Response 型には `utoipa::ToSchema` derive があり、OpenAPI schema 生成に使われる
- `#[utoipa::path]` の `request_body` / `responses` 参照パスが変わらないことを確認
- `CreateApiTokenRequest` は handler の `#[utoipa::path]` から参照されているため、型のパスが変わると OpenAPI snapshot に影響する可能性がある → snapshot 差分を確認

**確認ポイント**: `cargo fmt`, `cargo clippy`, `cargo test --workspace` 全通過。`api_token_test.rs` (15件) 全通過。`cargo insta test` で OpenAPI snapshot に差分がないことを確認

#### Step 4: 最終確認

**何をするか**: 全体の整合性確認

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. OpenAPI snapshot 確認: `cargo insta test -p boardflow-api` (差分があれば `cargo insta review`)
5. 差分がある場合はレビュー対象として記録

### リスクと注意点

1. **`AppError` の service 層での利用**: service 層が HTTP 概念 (`ErrorCode::NotFound` → 404 等) を知っていることになる。今回は許容するが、将来的に domain error への分離を検討すべき (別 Issue)
2. **OpenAPI snapshot**: `utoipa::ToSchema` を derive した Request/Response 型の移動により、OpenAPI schema の型パス表記が変わる可能性がある。Step 3 後に snapshot 差分を必ず確認
3. **`generate_upload_info` の空 request_id**: 現在 `AppError::internal_error("...", "")` と空 request_id で呼んでおり、service 化で改善可能だが、挙動変更になるため今回は維持
4. **crate 外への公開**: `services` モジュールは `pub mod` だが、個別の service 関数は `pub(crate)` にして crate 外から直接呼べないようにする
5. **`list_api_tokens` の位置**: read 系だが api_token.rs の他の command 系と密結合 (同じ Request/Response 型を共有) のため、今回はまとめて services に移行する

### 実装要否

**`implementation_required`**

### 未解決の疑問

なし — 全て調査・コードリーディングで解消済み

### 更新した作業ログパス

`docs/logs/96/worklog.md`

- (外部ドキュメント参照なし — 内部コードベース調査のみ)

---

## Step 2 実装フェーズ (2026-05-15)

### 実装内容

`crates/api/src/routes/board_run.rs` (551行) からユースケースロジックを `crates/api/src/services/board_run.rs` に抽出。

#### 移動したもの
- `bundle_status()` private helper → `services/board_run.rs` 内部関数
- `generate_upload_info()` async private helper → `services/board_run.rs` 内部関数
- `create_board_run` のユースケースロジック → `pub(crate) async fn execute_create_board_run()`
- `fail_board_run` のユースケースロジック → `pub(crate) async fn execute_fail_board_run()`
- `import_artifact_bundle` のユースケースロジック → `pub(crate) async fn execute_import_artifact_bundle()`

#### handler の簡素化
各handler は extractor でのパース → service 呼び出し → `Ok(Json(response))` のパターンに統一。`#[utoipa::path]` アトリビュートはそのまま handler に残存。

#### 注意点の対応
- `generate_upload_info` の空 request_id (`AppError::internal_error("...", "")`) は挙動変更を避けるためそのまま維持
- `import_artifact_bundle` のトランザクション管理 (`pool.begin()`) は service 内部に含めた
- `StagingBucket` は handler で `Extension` から取り出し `&str` として service に渡す形式
- `auth.0.installation_id` は `execute_import_artifact_bundle` に引数として渡す

#### 追加ファイル
- `crates/api/src/services/api_token.rs` — 空のスタブファイル（`services/mod.rs` が `pub mod api_token;` を宣言済みのため、コンパイル通過に必要）

### ファイル変更
| パス | 変更 |
|------|------|
| `crates/api/src/services/board_run.rs` | 新規作成 (3 service関数 + 2 private helpers) |
| `crates/api/src/services/api_token.rs` | 新規作成 (空スタブ) |
| `crates/api/src/routes/board_run.rs` | 551行 → 131行に簡素化 |

### テスト結果
- `cargo fmt --all -- --check`: 通過
- `cargo clippy --workspace --all-targets -- -D warnings`: 通過
- `cargo test --workspace`: 全通過
  - `board_run_test.rs`: **19件全通過** (test_create_board_run_success, test_fail_board_run_success 等)

### 残リスク
- `services/api_token.rs` は空スタブ — Step 3 で実装予定

---

## Step 3 実装フェーズ (2026-05-15)

### 実装内容

`crates/api/src/routes/api_token.rs` (291行) からユースケースロジックを `crates/api/src/services/api_token.rs` に抽出。

#### 移動したもの
- **Request/Response 型**: `CreateApiTokenRequest`, `CreateApiTokenResponse`, `ApiTokenListItem`, `ApiTokenDetailResponse` (全て `utoipa::ToSchema` derive 付き)
- **Private helpers**: `generate_raw_token()`, `hash_token()` → `services/api_token.rs` 内部関数
- **Service関数**:
  - `pub(crate) async fn execute_create_api_token()` — create handler のユースケースロジック (name validation, repo lookup, access check, token生成, DB insert)
  - `pub(crate) async fn execute_list_api_tokens()` — list handler のユースケースロジック (repo lookup, access check, pagination, DB query, cursor encode)
  - `pub(crate) async fn execute_revoke_api_token()` — revoke handler のユースケースロジック (UUID parse, repo lookup, access check, DB revoke)

#### handler の簡素化
- `routes/api_token.rs`: 291行 → 114行
- 各 handler は extractor parse → service 呼び出し → `Ok(Json(response))` に統一
- `create_api_token` は `StatusCode::CREATED` を handler 側で付与 (service は `CreateApiTokenResponse` のみ返す)
- `#[utoipa::path]` アトリビュートはそのまま handler に残存
- Request/Response 型は `use crate::services::api_token::*` ではなく個別 import で参照

### ファイル変更
| パス | 変更 |
|------|------|
| `crates/api/src/services/api_token.rs` | 空スタブ → 224行 (4 型定義 + 2 private helper + 3 service関数) |
| `crates/api/src/routes/api_token.rs` | 291行 → 114行に簡素化 |

### テスト結果
- `cargo fmt --all -- --check`: 通過
- `cargo clippy --workspace --all-targets -- -D warnings`: 通過
- `cargo insta test -p boardflow-api`: **全73テスト通過** (63 integration + 10 webhook)
  - `api_token_test.rs`: **15件全通過**
- **OpenAPI snapshot 差分: なし** ("no snapshots to review")

### 残リスク
- `AppError` がservice層でHTTP概念を含む点は変わらず (将来的にdomain error分離が望ましいが、このリファクタリングのスコープ外)
