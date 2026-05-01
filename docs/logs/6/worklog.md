# Issue #6: Web UI Read API実装

## 経緯
- バックエンド実装Issue分割タスクの一環として作成
- フロントエンドが利用するRead API群

## ユーザー要望
- docs/以下の仕様に基づくRustバックエンド実装の第6段階

## Issue作成内容
- Repository/BoardProject/BoardRun/Artifact 一覧・詳細 + Viewer Sources API
- URL: https://github.com/f0reachARR/boardflow/issues/6

## 後続処理タイプの初期仮説
`implementation_required`

## 作業経緯

### Phase 1: 調査 (Research)
- 開始: 2026-05-01
- 状態: 完了

### Phase 2: 計画 (Plan)
- 開始: 2026-05-01
- 状態: 完了

---

## 実装計画

### 目的
Web UI フロントエンドが利用する読み取り専用APIエンドポイント群（Repository/BoardProject/BoardRun/Artifact一覧・詳細、Viewer Sources API）を実装する。

### 非目的
- GitHub OAuth session認証ミドルウェア実装（後続Issue）
- Artifact Proxy API実装（別Issue）
- `run_check_findings` の一覧API（別Issue）
- Write操作（既に実装済み）

### 受け入れ条件
1. `docs/backend/api.md` セクション3の全8エンドポイントが動作する
2. Cursor pagination が limit/cursor/next_cursor/has_more で正しく動作する
3. 認証なしでアクセス可能（MVP初期段階）
4. OpenAPI spec に全エンドポイントが登録される
5. 統合テストがパスする
6. Viewer Sources APIが artifact のステータスに基づいたviewer状態を返す

### 詳細要件

#### エンドポイント一覧
| # | Method | Path | 概要 |
|---|--------|------|------|
| 1 | GET | `/api/v1/repositories` | Repository一覧 (cursor pagination) |
| 2 | GET | `/api/v1/repositories/{github_repository_id}` | Repository詳細 |
| 3 | GET | `/api/v1/repositories/{github_repository_id}/board-projects` | BoardProject一覧 (cursor pagination) |
| 4 | GET | `/api/v1/board-projects/{board_project_id}` | BoardProject詳細 |
| 5 | GET | `/api/v1/board-projects/{board_project_id}/board-runs` | BoardRun一覧 (cursor pagination) |
| 6 | GET | `/api/v1/board-runs/{board_run_id}` | BoardRun詳細 |
| 7 | GET | `/api/v1/board-runs/{board_run_id}/artifacts` | Artifact一覧 |
| 8 | GET | `/api/v1/board-runs/{board_run_id}/viewer-sources` | Viewer Sources |

#### Cursor Pagination 仕様
- `limit`: default=50, max=100
- `cursor`: opaque string (base64エンコードされたJSON `{"updated_at": "...", "id": "..."}`)
- 並び順: エンドポイントごとに固定
  - repositories: `updated_at DESC, id DESC`
  - board-projects: `updated_at DESC, id DESC`
  - board-runs: `created_at DESC, id DESC`
- `next_cursor`: 次ページがなければ `null`
- `has_more`: bool

### 影響範囲
- `crates/db/src/queries/` - Read用クエリ関数追加
- `crates/api/src/routes/` - 新規ルートモジュール追加
- `crates/api/src/lib.rs` - ルート登録追加
- `crates/api/tests/` - 統合テスト追加

### 設計方針

#### 1. ファイル構成

| ファイル | 責務 |
|---------|------|
| `crates/api/src/routes/mod.rs` | `pub mod read;` 追加 |
| `crates/api/src/routes/read.rs` | Read API全エンドポイントのハンドラ + レスポンス型 |
| `crates/db/src/queries/repository.rs` | `list_repositories`, `find_by_github_id` 追加 |
| `crates/db/src/queries/board_project.rs` | `list_by_repository`, `find_by_id_with_repository` 追加 |
| `crates/db/src/queries/board_run.rs` | `list_by_board_project`, `find_by_id_with_checks` 追加 |
| `crates/db/src/queries/artifact.rs` | `list_by_board_run` 追加 |
| `crates/db/src/queries/run_check.rs` | `list_by_board_run` 追加 |
| `crates/api/tests/read_api_test.rs` | Read API統合テスト |

#### 2. DBクエリ追加計画

**repository.rs に追加:**
- `find_by_github_id(executor, github_repository_id: i64) -> Option<Repository>`
- `list_repositories(executor, limit: i64, cursor: Option<(DateTime<Utc>, Uuid)>) -> Vec<Repository>`
  - SQL: `SELECT * FROM repositories WHERE (updated_at, id) < ($cursor) ORDER BY updated_at DESC, id DESC LIMIT $limit + 1`
  - limit+1 取得で has_more 判定

**board_project.rs に追加:**
- `list_by_repository_github_id(executor, github_repository_id: i64, limit: i64, cursor: Option<(DateTime<Utc>, Uuid)>) -> Vec<BoardProject>`
  - JOIN repositories で github_repository_id フィルタ
- `find_by_id_with_repository(executor, id: Uuid) -> Option<(BoardProject, Repository)>`
  - JOIN で repository 情報取得（詳細APIレスポンスに必要）

**board_run.rs に追加:**
- `list_by_board_project(executor, board_project_id: Uuid, limit: i64, cursor: Option<(DateTime<Utc>, Uuid)>) -> Vec<BoardRun>`
  - ORDER BY created_at DESC, id DESC

**artifact.rs に追加:**
- `list_by_board_run(executor, board_run_id: Uuid) -> Vec<Artifact>`
  - pagination不要（1 run あたり artifact数は有限）

**run_check.rs に追加:**
- `list_by_board_run(executor, board_run_id: Uuid) -> Vec<RunCheck>`

#### 3. ルートハンドラ実装計画 (`routes/read.rs`)

各ハンドラの構造:
```rust
#[utoipa::path(get, path = "/api/v1/...", params(...), responses(...))]
pub async fn handler(
    State(pool): State<PgPool>,
    Extension(request_id): Extension<RequestId>,
    Path(...): Path<...>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Response>, AppError> { ... }
```

共通:
- `PaginationParams` struct: `limit: Option<u32>`, `cursor: Option<String>`
- cursor decode/encode ヘルパー関数
- ID prefix parse/format ヘルパー（既存 `board_run.rs` のパターン踏襲）

#### 4. レスポンス型定義

```rust
// 共通pagination wrapper
struct PaginatedResponse<T> {
    items: Vec<T>,
    next_cursor: Option<String>,
    has_more: bool,
}

// Repository一覧item
struct RepositoryListItem { github_repository_id, owner, name, installation_id, board_project_count, latest_run_status, updated_at }

// Repository詳細
struct RepositoryDetail { github_repository_id, owner, name, installation_id, html_url, board_project_count, created_at, updated_at }

// BoardProject一覧item
struct BoardProjectListItem { board_project_id, project_path, project_dir, display_name, state, latest_completed_run_id, latest_tree_hash, issue_url, updated_at }

// BoardProject詳細
struct BoardProjectDetail { board_project_id, repository{...}, project_path, project_dir, display_name, state, latest_completed_run_id, latest_tree_hash, issue_number, issue_url, recreate_issue_on_update, created_at, updated_at }

// BoardRun一覧item
struct BoardRunListItem { board_run_id, status, commit_sha, branch, ref, github_run_id, github_run_attempt, tree_hash, erc_status, erc_errors, erc_warnings, drc_status, drc_errors, drc_warnings, created_at, completed_at }

// BoardRun詳細
struct BoardRunDetail { board_run_id, board_project_id, status, commit_sha, branch, ref, github_run_id, github_run_attempt, tree_hash, checks[], artifact_summary{}, created_at, completed_at }

// Artifact一覧item
struct ArtifactListItem { artifact_id, type, status, filename, content_type, sha256, size_bytes, source_path, logical_name, status_reason, created_at }

// Viewer Sources
struct ViewerSourcesResponse { board_run_id, expires_at, viewers{} }
```

#### 5. Cursor Pagination 実装方針

```rust
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Cursor {
    ts: String,  // RFC3339
    id: String,  // UUID hex
}

fn decode_cursor(s: &str) -> Result<Cursor, AppError> {
    let bytes = URL_SAFE_NO_PAD.decode(s).map_err(|_| ...)?;
    serde_json::from_slice(&bytes).map_err(|_| ...)
}

fn encode_cursor(ts: DateTime<Utc>, id: Uuid) -> String {
    let c = Cursor { ts: ts.to_rfc3339(), id: id.to_string() };
    URL_SAFE_NO_PAD.encode(serde_json::to_vec(&c).unwrap())
}
```

SQLは `WHERE (col, id) < ($1, $2)` パターンで keyset pagination。
limit+1行取得し、N+1行目が存在すれば `has_more=true`、N行目で next_cursor生成。

#### 6. Viewer Sources 実装方針
- artifact一覧から type でグルーピング
- viewer ごとに必要な artifact type を定義（kicanvas: kicad_pro/kicad_sch/kicad_pcb, schematic: schematic_pdf, pcb_preview: pcb_top_svg/pcb_bottom_svg, ibom: ibom_html, bom: bom_csv, fabrication: gerber_zip/drill_zip）
- artifact status に基づきviewer status を判定（全available→available, 一部available→partial, 全missing→missing）
- MVP では artifact proxy URL は placeholder (`/proxy/artifacts/{artifact_id}`) とし、token生成は後続Issue

#### 7. board_project_count / latest_run_status の取得
- Repository一覧のレスポンスに含まれる `board_project_count` と `latest_run_status` はサブクエリで取得
- SQL: `SELECT r.*, (SELECT COUNT(*) FROM board_projects WHERE repository_id = r.id) AS board_project_count, ...`
- 型は DB query 専用の拡張struct（`RepositoryWithStats`）を定義

### テスト計画 (`crates/api/tests/read_api_test.rs`)

1. **Repository一覧**: 空一覧、複数件、pagination(limit=1で2ページ取得)
2. **Repository詳細**: 正常取得、存在しないID→404
3. **BoardProject一覧**: 正常取得、pagination、repository不在→404
4. **BoardProject詳細**: 正常取得、存在しないID→404
5. **BoardRun一覧**: 正常取得、pagination
6. **BoardRun詳細**: 正常取得（checks含む）、存在しないID→404
7. **Artifact一覧**: 正常取得（mixed status）
8. **Viewer Sources**: available/partial/missing判定

テストはDBありの統合テスト。既存パターン（`setup_pool`, `create_test_*`）を踏襲。

### 実装順序

1. **DB queries** - Read用関数を各queryモジュールに追加
2. **レスポンス型 + cursor helper** - `routes/read.rs` にstruct定義
3. **ハンドラ実装** - Repository → BoardProject → BoardRun → Artifact → Viewer Sources の順
4. **ルート登録** - `lib.rs` の `create_app` に追加
5. **統合テスト** - `read_api_test.rs` 作成
6. **動作確認** - `cargo build` + `cargo test`

### ドキュメント更新対象
- `docs/logs/6/worklog.md` - 本計画および実装記録
- `docs/backend/api.md` - 変更不要（仕様は既に記載済み）

### 実装要否
`implementation_required`

### 未解決の疑問
- **解消済み**: 認証 → MVP初期は認証スキップ（ユーザー要望で確認済み）
- **解消済み**: viewer-sources の URL生成 → MVP では placeholder URL、後続Issueで実token生成

### ブランチ
`feature/issue-6-web-ui-read-api` (mainから作成)

---

## 残リスク
- Viewer Sources の artifact proxy token 生成は後続Issue依存
- board_project_count のサブクエリが大量データでパフォーマンス問題になる可能性（MVPでは許容）
- GitHub OAuth session認証が未実装のため、全データが認証なしで閲覧可能（後続Issueで対応）
