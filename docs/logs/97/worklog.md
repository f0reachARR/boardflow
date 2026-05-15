# Issue #97 — APIのrepository access認可処理を共通サービス化する

## 調査対象

- `crates/api/src/routes/read/` 以下の全handler
- `crates/api/src/services/` 以下の全service
- `crates/api/src/github_access/` モジュール（types, error_conversion, test_doubles）
- `crates/api/src/error.rs` の `AppError` 定義
- `crates/api/tests/` のテスト構造

---

## 調査結果

### 1. 認可チェックが行われている全ファイル・関数

#### `routes/read/` (handler層)

| ファイル | 関数 | パターン | not_found_msg |
|---------|------|---------|---------------|
| `repositories.rs` | `list_repositories` | `list_accessible_repo_ids` + `access_error_to_app_error` | ― |
| `repositories.rs` | `get_repository` | repo by github_id → `check_access` → `access_result_to_error` | `"repository not found"` |
| `board_projects.rs` | `list_board_projects` | repo by github_id → `check_access` → `access_result_to_error` | `"repository not found"` |
| `board_projects.rs` | `get_board_project` | `find_by_id_with_repository` → inline `check_access` → `access_result_to_error` | `"board project not found"` |
| `board_runs.rs` | `list_board_runs` | repo by board_project_id → `check_access` → `access_result_to_error` | `"board project not found"` |
| `board_runs.rs` | `get_board_run` | repo by board_run_id → `check_access` → `access_result_to_error` | `"board run not found"` |
| `diff.rs` | `get_board_run_diff` | repo by board_run_id → `check_access` → `access_result_to_error` | `"board run not found"` |
| `viewer_sources.rs` | `get_viewer_sources` | repo by board_run_id → `check_access` → `access_result_to_error` | `"board run not found"` |
| `findings.rs` | `list_findings` | repo by board_run_id → `check_access` → `access_result_to_error` | `"board run not found"` |
| `artifacts.rs` | `list_artifacts` | repo by board_run_id → `check_access` → `access_result_to_error` | `"board run not found"` |

#### `services/` (service層)

| ファイル | 関数 | パターン | not_found_msg |
|---------|------|---------|---------------|
| `api_token.rs` | `execute_create_api_token` | repo by github_id → `check_access` → `access_result_to_error` | `"repository not found"` |
| `api_token.rs` | `execute_list_api_tokens` | repo by github_id → `check_access` → `access_result_to_error` | `"repository not found"` |
| `api_token.rs` | `execute_revoke_api_token` | repo by github_id → `check_access` → `access_result_to_error` | `"repository not found"` |

**合計: 13箇所** で認可チェックが行われている。

---

### 2. 重複している認可パターン（3パターン + 1特殊パターン）

#### パターンA: github_repository_id → repo lookup → check_access
**使用箇所**: `list_board_projects`, `get_repository`, `execute_create_api_token`, `execute_list_api_tokens`, `execute_revoke_api_token` (5箇所)

```rust
let repo = boardflow_db::queries::repository::find_by_github_id(&pool, github_repository_id)
    .await
    .map_err(|e| {
        tracing::error!("... repo lookup failed: {e}");
        AppError::internal_error("database error", &request_id)
    })?
    .ok_or_else(|| AppError::not_found("repository not found", &request_id))?;

let result = access_checker
    .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
    .await;
if let Some(err) = access_result_to_error(&result, "repository not found", &request_id) {
    return Err(err);
}
```

#### パターンB: board_run_id → repo lookup → check_access
**使用箇所**: `get_board_run`, `get_board_run_diff`, `get_viewer_sources`, `list_findings`, `list_artifacts` (5箇所)

```rust
let repo = boardflow_db::queries::board_run::find_repository_by_board_run_id(&pool, id)
    .await
    .map_err(|e| {
        tracing::error!("... repo lookup failed: {e}");
        AppError::internal_error("database error", &request_id)
    })?
    .ok_or_else(|| AppError::not_found("board run not found", &request_id))?;

let result = access_checker
    .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
    .await;
if let Some(err) = access_result_to_error(&result, "board run not found", &request_id) {
    return Err(err);
}
```

#### パターンC: board_project_id → repo lookup → check_access
**使用箇所**: `list_board_runs` (1箇所)

```rust
let repo = boardflow_db::queries::board_project::find_repository_by_board_project_id(&pool, bp_id)
    .await
    .map_err(|e| {
        tracing::error!("list_board_runs repo lookup failed: {e}");
        AppError::internal_error("database error", &request_id)
    })?
    .ok_or_else(|| AppError::not_found("board project not found", &request_id))?;

let result = access_checker
    .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
    .await;
if let Some(err) = access_result_to_error(&result, "board project not found", &request_id) {
    return Err(err);
}
```

#### 特殊パターン: list_repositories (pre-filter)
**使用箇所**: `list_repositories` (1箇所)

```rust
let accessible_ids = access_checker
    .list_accessible_repo_ids(token)
    .await
    .map_err(|e| access_error_to_app_error(&e, &request_id))?;
```

`check_access` ではなく `list_accessible_repo_ids` を使用。パターンが異なるため共通化の対象外。

#### get_board_project の変形
`get_board_project` はパターンAの変形で、`find_by_id_with_repository` クエリの結果から `repo_owner` / `repo_name` を直接取得しているため、別途 repo lookup が不要。

---

### 3. `access_result_to_error` の現在の実装

**場所**: `crates/api/src/github_access/error_conversion.rs`

```rust
pub fn access_result_to_error(
    result: &AccessResult,
    not_found_msg: &str,
    request_id: &str,
) -> Option<AppError> {
    match result {
        AccessResult::Allowed => None,
        AccessResult::Denied => Some(AppError::not_found(not_found_msg, request_id)),
        AccessResult::Error(AccessError::TokenExpired) => Some(AppError::unauthorized(
            "github session expired, please re-login", request_id,
        )),
        AccessResult::Error(AccessError::RateLimited) => Some(AppError::new(
            crate::error::ErrorCode::RateLimited, "rate limited", request_id,
        )),
        AccessResult::Error(AccessError::Upstream(detail)) => {
            tracing::error!("GitHub API error: {detail}");
            Some(AppError::internal_error("upstream error", request_id))
        }
    }
}

pub(crate) fn access_error_to_app_error(err: &AccessError, request_id: &str) -> AppError {
    match err {
        AccessError::TokenExpired => AppError::unauthorized("github session expired, please re-login", request_id),
        AccessError::RateLimited => AppError::new(ErrorCode::RateLimited, "rate limited", request_id),
        AccessError::Upstream(detail) => {
            tracing::error!("GitHub API error: {detail}");
            AppError::internal_error("upstream error", request_id)
        }
    }
}
```

**重要**: `Denied` は `not_found` に変換（forbidden を外部に漏らさない）。この挙動はセキュリティ要件。

---

### 4. `services/` の既存パターン

#### `services/mod.rs`
```rust
pub mod api_token;
pub mod board_run;
pub mod plan;
```

#### 構造特徴
- `plan.rs` / `board_run.rs`: API tokenベースの認可（`token_repository_id` と DB上のrepository_idの一致チェック）。GitHub access checkerは使わない。
- `api_token.rs`: GitHub session-based認可。`DynGithubAccessChecker` + `access_result_to_error` を使用。handler層と同じパターン。

→ 新しい `services/authz.rs` を追加する場合、`mod.rs` に `pub mod authz;` を追加する形になる。

---

### 5. `AppError` の定義と種別

**場所**: `crates/api/src/error.rs`

```rust
pub enum ErrorCode {
    Unauthorized,     // 401
    Forbidden,        // 403
    ValidationFailed, // 400
    NotFound,         // 404
    Conflict,         // 409
    Gone,             // 410
    RateLimited,      // 429
    InternalError,    // 500
}

pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub request_id: String,
}
```

便利コンストラクタ: `unauthorized()`, `forbidden()`, `validation_failed()`, `not_found()`, `conflict()`, `gone()`, `internal_error()`, `new()`

`IntoResponse` トレイトでaxum ResponseへHTTP status codeとJSON bodyに変換される。

---

### 6. テストファイルの構造

| ファイル | 内容 |
|---------|------|
| `read_api_test.rs` | read系APIの統合テスト。`AllowAll`, `DenyAll`, `RateLimited`, `UpstreamError` の各checker使用。認可テスト含む |
| `api_token_test.rs` | API tokenのCRUDテスト。`AllowAll`, `DenyAll` 使用 |
| `proxy_test.rs` | artifact proxy テスト。`AllowAll` 使用 |
| `github_cache_test.rs` | `CachedGithubAccessChecker` の単体テスト |
| `auth_test.rs` | ErrorCode, セッション認証テスト |
| `common.rs` | テストヘルパー（unique ID生成等） |

テストdoubles (`test_doubles.rs`): `AllowAllGithubAccessChecker`, `DenyAllGithubAccessChecker`, `RateLimitedGithubAccessChecker`, `TokenExpiredGithubAccessChecker`, `UpstreamErrorGithubAccessChecker`

---

### 7. 共通化の設計方針

Issueで提案されている `ensure_*` ヘルパーに対応するパターンマッピング:

| 提案ヘルパー | 対応パターン | 使用箇所数 |
|-------------|------------|----------|
| `ensure_repository_access(pool, checker, token, github_repo_id, request_id)` | パターンA | 5 |
| `ensure_board_run_access(pool, checker, token, board_run_id, request_id)` | パターンB | 5 |
| `ensure_board_project_access(pool, checker, token, board_project_id, request_id)` | パターンC | 1 |

#### 返り値の設計
- パターンA: `Result<RepositoryRow, AppError>` — 後続処理で `repo.id`, `repo.installation_id` 等を使う
- パターンB: `Result<RepositoryRow, AppError>` — 後続で `repo` を使う箇所あり（ないものもある）
- パターンC: `Result<RepositoryRow, AppError>` — 同上

#### 注意点
- `get_board_project` は `find_by_id_with_repository` で repo 情報を一緒に取得している。repo_owner/repo_nameだけで access check する変形パターン。このhandlerには ensure_board_project_access がそのまま適用できない可能性あり（別クエリで repo を再取得するとN+1になる）。
- `list_repositories` は `list_accessible_repo_ids` を使う特殊パターン。共通化対象外。

---

## 結論ステータス

**`implementation_required`**

### 根拠
- 13箇所で認可チェックが重複しており、パターンA/B/Cへの共通化は明確
- `access_result_to_error` は既にPR #127で抽出済みだが、「repo lookup + access check」の組み合わせがまだ散在
- 外部ライブラリの調査は不要（純粋な内部リファクタリング）

### 残リスク
- `get_board_project` の変形パターン：repo情報がすでにJOINで取得済みのため、ensure_board_project_accessを適用すると追加クエリが発生する。handlerの特性に応じて、owner/name を引数に取る低レベル版ヘルパーも検討すべき
- テスト: `read_api_test.rs` で DenyAll/RateLimited/UpstreamError の統合テストが存在し、リファクタ後も同じ挙動であることを確認可能

### 後続エージェントへの注意点
1. `services/authz.rs` を新規作成し、`ensure_repository_access`, `ensure_board_run_access`, `ensure_board_project_access` を実装
2. 各ヘルパーは `Result<RepositoryRow, AppError>` を返し、repo情報を後続処理で使えるようにする
3. `get_board_project` は独自のクエリ結果（`repo_owner`, `repo_name`）を使うため、低レベルヘルパー（owner/nameだけ受け取る版）を検討
4. `list_repositories` の `list_accessible_repo_ids` パターンは共通化対象外
5. 既存の `access_result_to_error` / `access_error_to_app_error` は `error_conversion.rs` に残す（ensure_* から内部で呼ぶ）
6. テスト: `cargo test --workspace` + 特に `read_api_test.rs`, `api_token_test.rs` の認可テストが通ることを確認
7. `Denied` → `not_found` 変換のセキュリティ挙動を絶対に変更しない

### 参照URL
なし（外部調査なし、コードベース内部の調査のみ）

---

---

## 実装計画 (plan フェーズ)

### 目的

handler/service 層に散在する「DB lookup → GitHub access check → AppError 変換」の重複コードを `services/authz.rs` に集約し、認可ロジックの一貫性・保守性を向上させる。

### 非目的

- 認可ロジック自体の挙動変更（Denied → not_found 変換など）
- `list_repositories` の `list_accessible_repo_ids` パターンの変更
- `error_conversion.rs` の関数の変更や移動
- テストの追加（既存テストの通過確認のみ）

### 受け入れ条件

1. `ensure_repository_access` / `ensure_board_run_access` / `ensure_board_project_access` が `services/authz.rs` に実装されている
2. パターンA/B/C の全11箇所が ensure_* 呼び出しに置き換わっている
3. `get_board_project` の変形パターンが `check_repo_access` で簡潔に書き直されている
4. 各 handler/service の not_found_msg 文字列が変更前と完全一致している
5. `cargo test --workspace` が全パス
6. `cargo clippy --workspace --all-targets -- -D warnings` が通る

---

### 詳細要件

#### 新規ファイル

**`services/authz.rs`** — 認可ヘルパーモジュール

```rust
use sqlx::PgPool;
use uuid::Uuid;

use boardflow_domain::models::repository::Repository;

use crate::error::AppError;
use crate::github_access::{DynGithubAccessChecker, access_result_to_error};

/// パターンA: github_repository_id → repo lookup → check_access
/// 使用箇所: get_repository, list_board_projects, execute_create/list/revoke_api_token
pub async fn ensure_repository_access(
    pool: &PgPool,
    access_checker: &DynGithubAccessChecker,
    github_access_token: &str,
    github_repository_id: i64,
    request_id: &str,
) -> Result<Repository, AppError> {
    let repo = boardflow_db::queries::repository::find_by_github_id(pool, github_repository_id)
        .await
        .map_err(|e| {
            tracing::error!("repository lookup failed: {e}");
            AppError::internal_error("database error", request_id)
        })?
        .ok_or_else(|| AppError::not_found("repository not found", request_id))?;

    check_repo_access(access_checker, github_access_token, &repo.owner, &repo.name, "repository not found", request_id).await?;

    Ok(repo)
}

/// パターンB: board_run_id → repo lookup → check_access
/// 使用箇所: get_board_run, get_board_run_diff, get_viewer_sources, list_findings, list_artifacts
pub async fn ensure_board_run_access(
    pool: &PgPool,
    access_checker: &DynGithubAccessChecker,
    github_access_token: &str,
    board_run_id: Uuid,
    request_id: &str,
) -> Result<Repository, AppError> {
    let repo = boardflow_db::queries::board_run::find_repository_by_board_run_id(pool, board_run_id)
        .await
        .map_err(|e| {
            tracing::error!("board run repo lookup failed: {e}");
            AppError::internal_error("database error", request_id)
        })?
        .ok_or_else(|| AppError::not_found("board run not found", request_id))?;

    check_repo_access(access_checker, github_access_token, &repo.owner, &repo.name, "board run not found", request_id).await?;

    Ok(repo)
}

/// パターンC: board_project_id → repo lookup → check_access
/// 使用箇所: list_board_runs
pub async fn ensure_board_project_access(
    pool: &PgPool,
    access_checker: &DynGithubAccessChecker,
    github_access_token: &str,
    board_project_id: Uuid,
    request_id: &str,
) -> Result<Repository, AppError> {
    let repo = boardflow_db::queries::board_project::find_repository_by_board_project_id(pool, board_project_id)
        .await
        .map_err(|e| {
            tracing::error!("board project repo lookup failed: {e}");
            AppError::internal_error("database error", request_id)
        })?
        .ok_or_else(|| AppError::not_found("board project not found", request_id))?;

    check_repo_access(access_checker, github_access_token, &repo.owner, &repo.name, "board project not found", request_id).await?;

    Ok(repo)
}

/// 低レベルヘルパー: owner/name を直接受け取って check_access → AppError 変換
/// 使用箇所: get_board_project（find_by_id_with_repository で取得済みの repo_owner/repo_name を使う）
pub async fn check_repo_access(
    access_checker: &DynGithubAccessChecker,
    github_access_token: &str,
    owner: &str,
    name: &str,
    not_found_msg: &str,
    request_id: &str,
) -> Result<(), AppError> {
    let result = access_checker
        .check_access(github_access_token, owner, name)
        .await;
    if let Some(err) = access_result_to_error(&result, not_found_msg, request_id) {
        return Err(err);
    }
    Ok(())
}
```

**設計判断**:
- `ensure_*` は `Result<Repository, AppError>` を返す。後続処理で `repo.id` や `repo.installation_id` を使う箇所があるため。
- `check_repo_access` は `Result<(), AppError>` を返す低レベル版。`get_board_project` のように既に owner/name が手元にある場合に使用する。`ensure_*` 内部からも呼ばれる。
- tracing のエラーメッセージは汎化する（`"repository lookup failed"` 等）。元の個別メッセージ（`"get_repository failed"`, `"list_board_projects repo lookup failed"` 等）は各 handler 固有であり、機能的意味はないため。
- `not_found_msg` は各呼び出し元から渡すが、`ensure_*` 関数内にハードコードする（パターンごとに固定のため）。

---

### 影響範囲

#### 変更対象ファイル一覧

| # | ファイル (crates/api/src/ 相対) | 変更内容 |
|---|------|----------|
| 1 | `services/mod.rs` | `pub mod authz;` 追加 |
| 2 | `services/authz.rs` | **新規作成** |
| 3 | `routes/read/repositories.rs` | `get_repository`: パターンA → `ensure_repository_access` |
| 4 | `routes/read/board_projects.rs` | `list_board_projects`: パターンA → `ensure_repository_access`, `get_board_project`: 変形 → `check_repo_access` |
| 5 | `routes/read/board_runs.rs` | `list_board_runs`: パターンC → `ensure_board_project_access`, `get_board_run`: パターンB → `ensure_board_run_access` |
| 6 | `routes/read/diff.rs` | `get_board_run_diff`: パターンB → `ensure_board_run_access` |
| 7 | `routes/read/viewer_sources.rs` | `get_viewer_sources`: パターンB → `ensure_board_run_access` |
| 8 | `routes/read/findings.rs` | `list_findings`: パターンB → `ensure_board_run_access` |
| 9 | `routes/read/artifacts.rs` | `list_artifacts`: パターンB → `ensure_board_run_access` |
| 10 | `services/api_token.rs` | 3関数: パターンA → `ensure_repository_access` |

#### 変更しないファイル

- `routes/read/repositories.rs` の `list_repositories` — `list_accessible_repo_ids` パターンは対象外
- `github_access/error_conversion.rs` — `access_result_to_error` / `access_error_to_app_error` はそのまま
- `github_access/mod.rs` — 再エクスポートは変更不要（`check_repo_access` は `services/authz.rs` から `access_result_to_error` を使うだけ）

---

### 設計方針

1. **`check_repo_access` を内部ビルディングブロックとする**: `ensure_*` 3関数は内部で `check_repo_access` を呼ぶ。`get_board_project` は直接 `check_repo_access` を使う。
2. **not_found_msg のハードコード**: `ensure_repository_access` は `"repository not found"` 固定、`ensure_board_run_access` は `"board run not found"` 固定、`ensure_board_project_access` は `"board project not found"` 固定。これは現在の全呼び出し箇所と完全一致。
3. **tracing メッセージの汎化**: 元の handler 名入りメッセージ（e.g., `"list_board_projects repo lookup failed"`, `"get_board_run repo lookup failed"`）は `"repository lookup failed"`, `"board run repo lookup failed"`, `"board project repo lookup failed"` に統一。error ログの構造化が目的であり、元の handler 名は `tracing::Span` から取得可能なため問題なし。
4. **import 整理**: 置き換え後に不要になる `use crate::github_access::access_result_to_error` を各ファイルから除去。`repositories.rs` の `list_repositories` は `access_error_to_app_error` を引き続き使うため、そちらの import は残す。

---

### 変更の順序

#### Step 1: `services/authz.rs` 新規作成 + `services/mod.rs` 更新

1. `services/authz.rs` を上記シグネチャで作成
2. `services/mod.rs` に `pub mod authz;` を追加
3. `cargo check -p boardflow-api` で型エラーがないことを確認

#### Step 2: パターンA — `ensure_repository_access` への置き換え (5箇所)

##### 2a. `routes/read/repositories.rs` — `get_repository`

Before:
```rust
use crate::github_access::{access_error_to_app_error, access_result_to_error};
...
    let repo = boardflow_db::queries::repository::find_by_github_id(&pool, github_repository_id)
        .await
        .map_err(|e| {
            tracing::error!("get_repository failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("repository not found", &request_id))?;

    let result = access_checker
        .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
        .await;
    if let Some(err) = access_result_to_error(&result, "repository not found", &request_id) {
        return Err(err);
    }
```

After:
```rust
use crate::services::authz::ensure_repository_access;
// access_error_to_app_error の import は list_repositories で使うため残す
// access_result_to_error の import は不要になるため削除
...
    let repo = ensure_repository_access(
        &pool, &access_checker, &session.user.github_access_token,
        github_repository_id, &request_id,
    ).await?;
```

Import 変更: `use crate::github_access::{access_error_to_app_error, access_result_to_error};` → `use crate::github_access::access_error_to_app_error;` + `use crate::services::authz::ensure_repository_access;`

##### 2b. `routes/read/board_projects.rs` — `list_board_projects`

Before:
```rust
use crate::github_access::access_result_to_error;
...
    let repo = boardflow_db::queries::repository::find_by_github_id(&pool, github_repository_id)
        ...
    let result = access_checker.check_access(...).await;
    if let Some(err) = access_result_to_error(...) { ... }
```

After:
```rust
use crate::services::authz::{ensure_repository_access, check_repo_access};
// access_result_to_error の import は削除
...
    let repo = ensure_repository_access(
        &pool, &access_checker, &session.user.github_access_token,
        github_repository_id, &request_id,
    ).await?;
```

##### 2c. `routes/read/board_projects.rs` — `get_board_project` (変形パターン)

Before:
```rust
    if let Some(err) = access_result_to_error(
        &access_checker
            .check_access(
                &session.user.github_access_token,
                &row.repo_owner,
                &row.repo_name,
            )
            .await,
        "board project not found",
        &request_id,
    ) {
        return Err(err);
    }
```

After:
```rust
    check_repo_access(
        &access_checker,
        &session.user.github_access_token,
        &row.repo_owner,
        &row.repo_name,
        "board project not found",
        &request_id,
    ).await?;
```

##### 2d. `services/api_token.rs` — 3関数

`execute_create_api_token`, `execute_list_api_tokens`, `execute_revoke_api_token` の各関数で:

Before:
```rust
use crate::github_access::{DynGithubAccessChecker, access_result_to_error};
...
    let repo = boardflow_db::queries::repository::find_by_github_id(pool, github_repository_id)
        ...
    let result = access_checker.check_access(...).await;
    if let Some(err) = access_result_to_error(...) { ... }
```

After:
```rust
use crate::github_access::DynGithubAccessChecker;
use crate::services::authz::ensure_repository_access;
...
    let repo = ensure_repository_access(
        pool, access_checker, github_access_token,
        github_repository_id, request_id,
    ).await?;
```

**注意**: `api_token.rs` の3関数は引数が `pool: &PgPool` (参照) であり、`ensure_repository_access` のシグネチャも `pool: &PgPool` なのでそのまま渡せる。

#### Step 3: パターンB — `ensure_board_run_access` への置き換え (5箇所)

##### 3a. `routes/read/board_runs.rs` — `get_board_run`

Before:
```rust
    let repo = boardflow_db::queries::board_run::find_repository_by_board_run_id(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("get_board_run repo lookup failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("board run not found", &request_id))?;

    let result = access_checker
        .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
        .await;
    if let Some(err) = access_result_to_error(&result, "board run not found", &request_id) {
        return Err(err);
    }
```

After:
```rust
    ensure_board_run_access(
        &pool, &access_checker, &session.user.github_access_token,
        id, &request_id,
    ).await?;
```

注意: `get_board_run` では repo の戻り値を使わないため `_` で受けるか、戻り値を破棄する。

##### 3b. `routes/read/diff.rs` — `get_board_run_diff`

同パターン。repo 不使用。

##### 3c. `routes/read/viewer_sources.rs` — `get_viewer_sources`

同パターン。repo 不使用。

##### 3d. `routes/read/findings.rs` — `list_findings`

同パターン。repo 不使用。

##### 3e. `routes/read/artifacts.rs` — `list_artifacts`

同パターン。repo 不使用。

#### Step 4: パターンC — `ensure_board_project_access` への置き換え (1箇所)

##### 4a. `routes/read/board_runs.rs` — `list_board_runs`

Before:
```rust
    let repo =
        boardflow_db::queries::board_project::find_repository_by_board_project_id(&pool, bp_id)
            .await
            .map_err(|e| {
                tracing::error!("list_board_runs repo lookup failed: {e}");
                AppError::internal_error("database error", &request_id)
            })?
            .ok_or_else(|| AppError::not_found("board project not found", &request_id))?;

    let result = access_checker
        .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
        .await;
    if let Some(err) = access_result_to_error(&result, "board project not found", &request_id) {
        return Err(err);
    }
```

After:
```rust
    ensure_board_project_access(
        &pool, &access_checker, &session.user.github_access_token,
        bp_id, &request_id,
    ).await?;
```

注意: repo の戻り値は `list_board_runs` で使用しないため破棄。

#### Step 5: import 整理 + cargo check/clippy/test

1. 各ファイルで不要になった `use crate::github_access::access_result_to_error` を削除
2. `cargo fmt --all`
3. `cargo clippy --workspace --all-targets -- -D warnings`
4. `cargo test --workspace`

---

### テスト観点

| テスト | 検証内容 |
|--------|---------|
| `read_api_test.rs` — `AllowAll` テスト群 | 正常系が変わらないこと |
| `read_api_test.rs` — `DenyAll` テスト群 | Denied → 404 変換が維持されていること |
| `read_api_test.rs` — `RateLimited` テスト | RateLimited → 429 変換が維持されていること |
| `read_api_test.rs` — `UpstreamError` テスト | Upstream → 500 変換が維持されていること |
| `api_token_test.rs` — `DenyAll` テスト | API token 認可が変わらないこと |
| `cargo clippy` | unused import の検出・修正 |

新規テストの追加は不要。`ensure_*` 関数は統合テストで間接的にテストされるため、単体テストは過剰。

---

### ドキュメント更新対象

- `docs/logs/97/worklog.md` (本ファイル) — 計画・実装記録
- `docs/backend/api.md` — 変更不要（外部挙動の変更なし）
- `docs/backend/summary.md` — services/authz.rs の追加を反映（任意）

---

### リスクと注意点

1. **tracing メッセージの差異**: 元の個別メッセージ（`"get_repository failed"`, `"list_board_projects repo lookup failed"` 等）が汎用メッセージ（`"repository lookup failed"` 等）に変わる。error ログ監視で handler 名を grep している場合に影響するが、tracing span で handler は判別可能なため低リスク。
2. **`get_board_project` の `check_repo_access`**: `not_found_msg` が呼び出し元で明示される。ensure_* と異なりハードコードされないため、呼び出し元のミスリスクがわずかにある。ただし1箇所のみなので許容。
3. **`ensure_*` が `Repository` を返すが使わない箇所**: パターンB の5箇所と パターンC の1箇所では repo を後続で使わない。`let _ = ensure_board_run_access(...)` とすると clippy が警告する可能性があるため、`.await?;` だけで十分（`Result` の `Ok` 値を使わなくても clippy は警告しない）。

---

### 実装要否

**`implementation_required`**

### 更新した作業ログパス

`docs/logs/97/worklog.md`

---

## 実装結果 (impl フェーズ)

### 実装内容

#### 新規ファイル
- `crates/api/src/services/authz.rs` — 4つの公開関数 + 5つのユニットテスト
  - `ensure_repository_access`: github_repository_id → repo lookup → check_access (パターンA)
  - `ensure_board_run_access`: board_run_id → repo lookup → check_access (パターンB)
  - `ensure_board_project_access`: board_project_id → repo lookup → check_access (パターンC)
  - `check_repo_access`: owner/name だけで access check (get_board_project の変形パターン用)

#### 変更ファイル (11箇所の置き換え)
- `services/mod.rs` — `pub mod authz;` 追加
- `services/api_token.rs` — 3関数 (`execute_create/list/revoke_api_token`) を `ensure_repository_access` に置き換え
- `routes/read/repositories.rs` — `get_repository` を `ensure_repository_access` に置き換え
- `routes/read/board_projects.rs` — `list_board_projects` を `ensure_repository_access`、`get_board_project` を `check_repo_access` に置き換え
- `routes/read/board_runs.rs` — `list_board_runs` を `ensure_board_project_access`、`get_board_run` を `ensure_board_run_access` に置き換え
- `routes/read/diff.rs` — `get_board_run_diff` を `ensure_board_run_access` に置き換え
- `routes/read/viewer_sources.rs` — `get_viewer_sources` を `ensure_board_run_access` に置き換え
- `routes/read/findings.rs` — `list_findings` を `ensure_board_run_access` に置き換え
- `routes/read/artifacts.rs` — `list_artifacts` を `ensure_board_run_access` に置き換え

#### import 整理
- `access_result_to_error` の直接 import を 8 ファイルから削除 (authz.rs 内部で使用)
- `access_error_to_app_error` は `repositories.rs` の `list_repositories` で引き続き使用 (共通化対象外のパターン)
- `DynGithubAccessChecker` は全 handler の Extension 抽出で引き続き使用

### テスト結果

#### ユニットテスト (5件, services::authz::tests)
- `check_repo_access_allowed` — Allowed → Ok(())
- `check_repo_access_denied_returns_not_found` — Denied → not_found (セキュリティ挙動保証)
- `check_repo_access_token_expired` — TokenExpired → 401
- `check_repo_access_rate_limited` — RateLimited → 429
- `check_repo_access_upstream_error` — UpstreamError → 500

#### 統合テスト (63件, boardflow-api 全テスト)
- 全テスト通過。認可関連テスト (`test_get_board_run_denied_returns_404`, `test_list_repositories_denied_returns_empty` 等) も変更なしでパス。

#### CI チェック
- `cargo fmt --all -- --check` — OK
- `cargo clippy --workspace --all-targets -- -D warnings` — OK (warning なし)

### 更新ドキュメント
- `docs/logs/97/worklog.md` (本ファイル)

### 未変更 (対象外)
- `list_repositories` — `list_accessible_repo_ids` を使う特殊パターン。共通化対象外。
- `error_conversion.rs` — `access_result_to_error` / `access_error_to_app_error` は変更なし。authz.rs から内部利用。

### 残リスク
- `database::tests::from_env_succeeds_when_database_url_set` (boardflow-config) が環境変数未設定で失敗するが、本Issue とは無関係の既存問題。
- tracing ログメッセージが `ensure_*` 関数のものに統一されたため、既存のログ文字列に依存するモニタリングがあれば調整が必要 (ただし内部ログのため外部影響なし)。
