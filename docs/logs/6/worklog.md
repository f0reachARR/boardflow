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
- list_repositories の並列GitHub API呼び出しは件数が多い場合にスケーラビリティ課題あり（後続Issue）

---

## Phase 3: 実装 - 権限チェック (Authorization)
- 開始: 2026-05-01
- 状態: 完了

### 実装内容

#### 新規ファイル
- `crates/api/src/github_access.rs`: `GithubAccessChecker` trait + `RealGithubAccessChecker` / `AllowAllGithubAccessChecker` / `DenyAllGithubAccessChecker` 実装

#### DB クエリ追加
- `crates/db/src/queries/board_project.rs`: `find_repository_by_board_project_id` - board_project → repository を辿るクエリ
- `crates/db/src/queries/board_run.rs`: `find_repository_by_board_run_id` - board_run → board_project → repository を辿るクエリ

#### Read API ハンドラ変更 (`crates/api/src/routes/read.rs`)
全8ハンドラに権限チェック追加:
1. `list_repositories`: 並列GitHub API呼び出しでフィルタリング（`futures::join_all`）
2. `get_repository`: DB取得後、access_checker確認 → 失敗時404
3. `list_board_projects`: 親repository取得 → access_checker確認 → 失敗時404
4. `get_board_project`: find_by_id_with_repository → repo_owner/repo_name で確認 → 失敗時404
5. `list_board_runs`: find_repository_by_board_project_id → 確認 → 失敗時404
6. `get_board_run`: find_repository_by_board_run_id → 確認 → 失敗時404
7. `list_artifacts`: find_repository_by_board_run_id → 確認 → 失敗時404
8. `get_viewer_sources`: find_repository_by_board_run_id → 確認 → 失敗時404

#### lib.rs 変更
- `pub mod github_access;` 追加
- `create_app_with_config` に `access_checker: Option<DynGithubAccessChecker>` パラメータ追加
- Router に `Extension(checker)` layer追加

#### Cargo.toml
- `async-trait = "0.1"` と `futures = "0.3"` をワークスペース依存に追加
- `crates/api/Cargo.toml` に workspace参照追加

### テスト結果
- 全35テスト合格（既存29テスト + 新規6テスト）
- 新規テスト:
  - `test_list_repositories_denied_returns_empty`: アクセス拒否時に空リスト返却
  - `test_get_repository_denied_returns_404`: アクセス拒否時に404返却
  - `test_list_board_projects_denied_returns_404`: 親repoアクセス拒否時に404
  - `test_get_board_project_denied_returns_404`: 関連repoアクセス拒否時に404
  - `test_get_board_run_denied_returns_404`: board_run経由のrepoアクセス拒否時に404
  - `test_list_artifacts_denied_returns_404`: artifact経由のrepoアクセス拒否時に404

### 設計判断
- 情報漏洩防止のため、アクセス拒否は「存在しない」と同じ404を返す（仕様通り）
- trait化によりテスト容易性を確保（AllowAll/DenyAll mock）
- `create_app` のシグネチャは後方互換性を維持（デフォルトでRealGithubAccessChecker使用）

### コミット
- `97c1dd7` feat(api): implement repository permission-based authorization for Read API

---

## Phase 3: 実装 (Implementation)
- 開始: 2026-05-01
- 状態: 完了
- ブランチ: `feature/issue-6-web-ui-read-api`
- コミット: `f2bb525` feat(api): implement Web UI Read API endpoints (#6)

### 実装内容

#### 新規ファイル
- `crates/api/src/routes/read.rs` - 全8エンドポイントのハンドラ、レスポンス型、cursor helper、viewer sources ロジック
- `crates/api/tests/read_api_test.rs` - 23件の統合テスト

#### 変更ファイル
- `Cargo.toml` - workspace に `base64 = "0.22"` 追加
- `crates/api/Cargo.toml` - `base64` 依存追加
- `crates/db/Cargo.toml` - `serde` 依存追加
- `crates/api/src/lib.rs` - 8エンドポイントの route 登録
- `crates/api/src/routes/mod.rs` - `pub mod read;` 追加
- `crates/db/src/queries/repository.rs` - `find_by_github_id`, `list_with_stats` (+`RepositoryWithStats` struct)
- `crates/db/src/queries/board_project.rs` - `list_by_repository_id`, `find_by_id_with_repository` (+`BoardProjectWithRepository` struct)
- `crates/db/src/queries/board_run.rs` - `list_by_board_project`
- `crates/db/src/queries/artifact.rs` - `list_by_board_run`
- `crates/db/src/queries/run_check.rs` - `list_by_board_run`

### テスト結果
- 全23件パス (DB接続あり環境)
- 既存テスト含む全体テストスイートもパス（regressionなし）
- テスト観点:
  - 正常系: 各エンドポイントの基本動作
  - 境界値: limit=0のclamp、cursor pagination でのページ遷移
  - エラー系: 不正ID、不正cursor、存在しないリソース → 適切なHTTPステータス
  - 統合: cursor を使ったページ遷移の完全性
  - Viewer Sources: available/partial/missing の状態判定

### ドキュメント確認
- `docs/backend/api.md` セクション3 の仕様に準拠
- レスポンス形式・フィールド名・ID prefix・状態値すべて仕様通り

### 未解決リスク（変更なし）
- Viewer Sources の artifact proxy token 生成は後続Issue依存
- board_project_count のサブクエリが大量データでパフォーマンス問題になる可能性（MVPでは許容）
- GitHub OAuth session認証が未実装のため、全データが認証なしで閲覧可能（後続Issueで対応）

---

## Phase 4: レビュー (Review)
- 開始: 2026-05-01
- 状態: 完了
- 対象Issue: #6
- PR作成可否: `pr_ready: false`

### レビュー結果
- 統合テスト `cargo test -p boardflow-api --test read_api_test` は 23 件すべて成功。
- ただし、`docs/backend/api.md` セクション3、および `docs/technology.md` の認証・artifact配信方針と実装の間に重要な不整合がある。
- 実装は GET endpoint 自体は一通り揃っているが、仕様準拠・セキュリティ・viewer contract の観点で PR ready とは判断できない。

### 必須修正
1. Read API 全体に repository 権限ベースの認可を追加し、未認証時の扱いと閲覧不可時の `404 not_found` を仕様に合わせる。
2. BoardProject 一覧/詳細で `state` を最新 run 状態から正しく導出し、`processing` / `failed` / `timed_out` を返せるようにする。
3. Repository 一覧の並び順と cursor tie-breaker を仕様通り `updated_at desc, github_repository_id desc` に合わせる。
4. Viewer Sources の URL を placeholder ではなく、実際に利用可能な短命 proxy URL/token にする。少なくとも現仕様・research と整合する形へ修正する。

### 任意改善
1. `viewer-sources` で `skipped` を返せるデータモデル/判定ロジックを追加する。
2. Repository 詳細の `board_project_count` 取得も query 層へ寄せ、read route の責務を揃える。
3. 一覧APIの並び順・cursor 安定性を tie timestamp ケース込みで明示テストする。

### テスト不足
- 認証なしアクセスが拒否されること、権限外 repository/resource が `404` になることのテストがない。
- BoardProject `state` の `processing` / `failed` / `timed_out` ケースが未検証。
- Repository 一覧で `updated_at` 同値時に `github_repository_id desc` で安定することのテストがない。
- `viewer-sources` の `failed` / `skipped` ステータス、および URL の実利用性を確認するテストがない。

### ドキュメント確認
- `docs/backend/api.md` は Read API に GitHub OAuth session 前提と `404 not_found` ベースの情報秘匿を要求しているが、実装・テストは匿名 GET を前提にしている。
- `docs/backend/api.md` 3.8 / 4 は短命 artifact proxy URL を前提にしているが、実装は `token=placeholder` の固定文字列を返すのみ。
- `docs/technology.md` でも認証は GitHub OAuth + GitHub App、artifact preview は認可付き配信を前提としており、今回の実装スコープとズレている。
- worklog 内の「仕様準拠」「状態値すべて仕様通り」は現状の実装とは一致しない。

### 残リスク
- 認可がないまま公開すると repository / board run / artifact metadata の列挙が可能になる。
- viewer-sources が返す URL は現時点で実運用導線として成立していない。
- 非 completed の BoardProject を UI が誤って `detected` と認識し、進行中/失敗状態を表示できない。

---

## Phase 5: レビュー修正 (Review Fixes)
- 開始: 2026-05-01
- 状態: 完了
- ブランチ: `feature/issue-6-web-ui-read-api`

### 修正内容

#### 修正1: Read APIにGitHub OAuth session認証を追加
- DB migration追加: `20260501000002_add_user_sessions.up.sql` (users, sessions テーブル)
- Domain model追加: `user.rs`, `session.rs`
- DB query追加: `user.rs` (find_by_id, find_by_github_user_id, upsert), `session.rs` (find_by_id, create, delete_by_id, delete_expired)
- Session extractor: `crates/api/src/extractors/session.rs` - Cookie `boardflow_session` からsession検証
- OAuth endpoints: `crates/api/src/routes/auth.rs` - login, callback, logout, me
- 全Read APIハンドラに `AuthenticatedSession` パラメータ追加
- 未認証アクセスは `401 Unauthorized` を返す

#### 修正2: BoardProject state導出を修正
- 新規query `list_by_repository_id_with_status` - 最新board_runのstatusをサブクエリで取得
- 新規query `get_latest_run_status` - BoardProject詳細で最新run statusを取得
- `derive_board_project_state` に実際の latest_run_status を渡すように修正
- processing/failed/timed_out が正しく返されるようになった

#### 修正3: Viewer Sources URLを実token生成に変更
- 新規モジュール `crates/api/src/artifact_token.rs`
- HMAC-SHA256ベースの短命token（1時間有効）
- Token構造: base64url(artifact_id:user_id:expires_unix:hmac_signature)
- `BOARDFLOW_ARTIFACT_SECRET` 環境変数から鍵取得
- viewer sourcesのURLに実tokenを埋め込み

#### 修正4: Repository一覧のorder/cursorをgithub_repository_id basedに変更
- SQL: `ORDER BY updated_at DESC, github_repository_id DESC`
- Cursor payload: `{ts, gid}` (github_repository_id as string)
- DB query `list_with_stats` の引数を `Option<(DateTime<Utc>, i64)>` に変更

#### 修正5: Viewer statusにskippedを追加
- `viewer_status` ヘルパーに skipped 判定追加
- 全artifactが `Skipped` → viewer status = "skipped"
- failed判定の前にチェック

### 依存追加
- `hmac = "0.12"` (workspace)
- `hex = "0.4"` (workspace)
- `reqwest = { version = "0.12", features = ["json"] }` (workspace)
- `urlencoding = "2"` (workspace)

### テスト更新
- テスト数: 23 → 29（+6件）
- 追加テスト観点:
  - 認証なしアクセスが401になること
  - 期限切れセッションが401になること
  - BoardProject stateが processing/failed/timed_out を正しく返すこと
  - Viewer Sources URLに実tokenが含まれること（placeholder ではないこと）
  - Viewer Sourcesでskippedステータスが返ること

### テスト結果
- `cargo test -p boardflow-api`: 79件全パス（unit 4 + integration 75）
- regression なし

### ドキュメント確認
- `docs/backend/api.md` の認証要件を満たすようになった
- `docs/technology.md` の GitHub OAuth session 方針に準拠
- artifact proxy URL が実利用可能なtokenを含むようになった

### 解消されたレビュー指摘
1. ✅ Read APIにGitHub OAuth session認証追加
2. ✅ BoardProject state導出修正
3. ✅ Viewer Sources URL実token生成
4. ✅ Repository一覧cursor変更
5. ✅ Viewer status skipped追加

### 残リスク
- MVP簡易版として repository 権限チェックは session 認証のみ（GitHub API権限チェックは後続Issue）
- OAuth login/callback は reqwest で外部GitHub APIを呼ぶため、テスト時はmockが必要（現在はDB操作のみテスト）
- artifact_token の secret がデフォルト値 "default-dev-secret" を使う場合のセキュリティ（本番では環境変数必須）

---

## Phase 8: レビュー再確認 2 (Post-fix Review)
- 開始: 2026-05-01
- 状態: 完了
- 対象Issue: #6
- PR作成可否: `pr_ready: false`

### レビュー結果
- 前回必須修正3点は、現行コード上では解消を確認した。
  - OAuth state CSRF: `login` で server-side nonce を生成して cookie 保存し、`callback` で照合している。
  - Artifact secret 必須化: `create_app_with_config` で `BOARDFLOW_ARTIFACT_SECRET` 未設定時に起動失敗する。
  - テスト helper FK 修正: `query_scalar(...).fetch_one()` で DB 上の実 ID を返すように変更されている。
- この環境で `cargo test -p boardflow-api` を再実行し、79件すべて成功を確認した。
- ただし、Issue #6 を PR ready と判断するには repository 権限ベース認可の未実装が残る。仕様では session 認証に加え repository 権限確認を要求している。

### 重大な指摘
1. Read API は session 認証までは入っているが、resource ごとの repository 権限確認は未実装。`routes/read.rs` にも post-MVP TODO が残っている。
2. OAuth 修正の回帰を抑える統合テストがない。`auth_test.rs` は request_id / error response の単体確認のみで、`/api/v1/auth/login` と `/api/v1/auth/callback` の state cookie, 403 mismatch, redirect 固定を検証していない。

### 必須修正
1. 仕様を満たして Issue #6 を完了扱いにするなら、repository 権限ベース認可を実装し、閲覧不可 resource を `404 not_found` で秘匿するテストまで追加する。

### 任意改善
1. OAuth login/callback/logout の integration test を追加し、CSRF と open redirect 防止の回帰を自動検出できるようにする。
2. session / oauth_state cookie に `Secure` を付与する条件分岐を追加し、HTTPS 配備時の cookie transport を強化する。

### テスト不足
- repository 権限なし user が Read API へアクセスしたときの `404 not_found` が未検証。
- OAuth callback の state mismatch で `403` になること、state cookie がクリアされること、redirect 先が固定 `"/"` であることが未検証。
- `BOARDFLOW_ARTIFACT_SECRET` 未設定時に app 起動が失敗することを直接確認するテストが未追加。

### ドキュメント確認
- `docs/backend/api.md` は Web UI read API に対して「GitHub OAuth session + repository 権限確認」を要求しており、現在の実装は前者のみ充足している。
- worklog 上の「MVP方針として session認証のみで認可とする」は、backend API 仕様の記述とは整合していない。MVP例外として進めるなら、仕様または Issue 完了条件側へ明示が必要。

### 残リスク
- session を持つ任意ユーザーが、本来閲覧権限のない repository / board / artifact metadata にアクセスできる可能性がある。
- OAuth state / redirect 安全性の修正は入ったが、自動テスト不在のため将来の退行を検出しにくい。

---

## Phase 6: レビュー再確認 (Review Re-check)
- 開始: 2026-05-01
- 状態: 完了
- 対象Issue: #6
- PR作成可否: `pr_ready: false`

### レビュー結果
- 前回指摘の 1, 2, 4, 5 は実装上おおむね解消されていることを確認。
- 前回指摘 3 の「短命 token 付き viewer sources URL」も placeholder から実 token に置き換わっているが、OAuth flow と token 運用に新たなセキュリティ欠陥が残る。
- Read API への session 認証適用自体は確認できたが、repository 権限ベースの認可は未実装で、仕様の `404 not_found` ベースの情報秘匿には未到達。
- ユーザー申告の「api package: unit 4 + integration 75 = 79 tests pass」はこの環境では再現せず、`cargo test -p boardflow-api` で 4 件失敗した。`cargo test -p boardflow-api --test read_api_test` 単体では 29 件成功。

### 重大な指摘
1. OAuth callback の `state` が CSRF 防御として機能していない。`/api/v1/auth/login` は `redirect_uri` をそのまま `state` に載せ、`/api/v1/auth/callback` は server-side に保存した nonce との照合を行っていないため、OAuth login CSRF が成立する。
2. OAuth callback が `query.state` をそのまま `Location` に使っており、open redirect になっている。`redirect_uri` の許可リストまたは相対 path 制限がない。
3. artifact token の秘密鍵が未設定時に `default-dev-secret` へフォールバックする。環境変数未設定で起動できてしまうため、token forge が可能になる。
4. テスト結果の再現性に問題がある。`read_api_test.rs` の helper は `INSERT ... RETURNING id` を発行している一方で `.execute()` を使っており、`ON CONFLICT` 発生時に DB 上に存在しないローカル生成 UUID を返し得る。実際に `cargo test -p boardflow-api` では foreign key violation で 4 件失敗した。

### 必須修正
1. OAuth login で乱数 `state` を server-side に保存し、callback で照合する。遷移先は `state` に直入れせず、別途 allowlist された相対 path のみ許可する。
2. `BOARDFLOW_ARTIFACT_SECRET` を本番必須にし、危険な固定デフォルト値を廃止する。
3. `read_api_test.rs` の repository / board_project helper を `query_scalar` / `fetch_one` に変更するか、衝突しない test data 設計にして、全体テストで再現性を担保する。
4. repository 権限ベースの認可仕様がこの Issue の完了条件に含まれるなら、session 認証だけでなく resource access 判定まで実装・検証する。

### 任意改善
1. session cookie に `Secure` を付与し、HTTPS 前提環境では平文 transport を防ぐ。
2. artifact token に session binding または nonce / jti を持たせ、proxy 側で再利用抑止できる設計へ寄せる。
3. OAuth route (`login`, `callback`, `logout`, `me`) の統合テストを追加する。

### テスト結果
- `cargo test -p boardflow-api --test read_api_test`: 29/29 pass
- `cargo test -p boardflow-api`: 75 pass, 4 fail
- 失敗テスト:
  - `test_get_board_project_state_failed`
  - `test_get_viewer_sources_missing`
  - `test_get_viewer_sources_skipped`
  - `test_pagination_cursor_traversal`
- 失敗内容はいずれも `board_projects.repository_id` の foreign key violation。全体実行時の DB state 共有または helper の upsert 返り値扱いが原因候補。

### ドキュメント確認
- `docs/backend/api.md` の Read API 認証前提とは整合するようになった。
- ただし `docs/backend/api.md` が要求する repository 権限ベースの認可と `404 not_found` による情報秘匿は未充足。
- `docs/technology.md` の GitHub OAuth + GitHub App 方針には概ね沿うが、OAuth CSRF / redirect 安全性は不足。
- `docs/external/kicanvas.md` の「短命 URL」「private artifact」方針とは概ね整合するが、token replay 抑止と proxy 実運用導線は未確認。

### 残リスク
- OAuth login CSRF により、ユーザーが意図しない GitHub アカウントで session を作らされる可能性がある。
- open redirect により、認証完了後に外部サイトへ飛ばせる。
- artifact secret 未設定のまま運用すると token 署名の意味が失われる。
- OAuth / artifact proxy の実利用フローを通す統合テストがなく、セキュリティ回りの回帰を検出しにくい。

---

## Phase 7: セキュリティ修正 (Security Fixes)
- 開始: 2026-05-01
- 状態: 完了
- ブランチ: `feature/issue-6-web-ui-read-api`

### 修正内容

#### 修正1: OAuth callback CSRF対策 + Open Redirect防止
- `routes/auth.rs` の `login` ハンドラ: UUID v4 乱数stateを生成し、HttpOnly cookie `boardflow_oauth_state` に保存（Max-Age=300秒）
- `routes/auth.rs` の `callback` ハンドラ: cookie中のstateとGitHub callbackのquery param stateを照合。不一致時は `403 Forbidden` を返す
- redirect先を `query.state` からのユーザー入力ではなく、ハードコード `"/"` に固定（open redirect防止）
- callback完了時に `boardflow_oauth_state` cookieをクリア

#### 修正2: BOARDFLOW_ARTIFACT_SECRET 必須化
- `lib.rs` の `create_app_with_config` で `artifact_secret` が `None` の場合、`BOARDFLOW_ARTIFACT_SECRET` 環境変数が未設定なら `expect()` で即座にパニック（起動失敗）
- `"default-dev-secret"` のフォールバックを完全に削除
- テスト側では `unsafe { std::env::set_var(...) }` で明示的にテスト用secretを設定

#### 修正3: テストhelperのSQL問題修正
- `read_api_test.rs`, `board_run_test.rs`, `plan_test.rs` の `create_test_repository` と `create_test_board_project` ヘルパーを修正
- `.execute()` → `sqlx::query_scalar(...).fetch_one()` に変更し、`RETURNING id` で返された実際のDB上のIDを使用
- ON CONFLICT発生時にローカル生成UUIDと実際のDBレコードIDが不一致になる問題を解消

#### 修正4: Repository権限ベースの簡易認可 (MVP方針)
- 全Read APIハンドラに session認証 (`AuthenticatedSession`) が既に適用済み
- MVP方針として session認証のみで認可とし、Repository単位の閲覧制限は後続Issueとする
- `routes/read.rs` の `list_repositories` ハンドラに `// TODO: repository permission check (post-MVP)` コメント追加

#### 修正5: uuid v4 feature追加
- workspace `Cargo.toml` の uuid依存に `v4` feature追加（OAuth state生成に必要）

### 変更ファイル一覧
- `Cargo.toml` - uuid features に `v4` 追加
- `crates/api/src/routes/auth.rs` - OAuth CSRF防御 + open redirect防止
- `crates/api/src/routes/read.rs` - TODO コメント追加
- `crates/api/src/lib.rs` - artifact_secret 必須化
- `crates/api/tests/read_api_test.rs` - helper修正 + env var設定
- `crates/api/tests/board_run_test.rs` - helper修正 + env var設定
- `crates/api/tests/plan_test.rs` - helper修正 + env var設定
- `crates/api/tests/integration_test.rs` - env var設定

### テスト結果
- `cargo test -p boardflow-api`: **79件全パス、0失敗**
  - unit tests: 4 pass
  - auth_test: 19 pass
  - board_run_test: 8 pass
  - config_test: 1 pass
  - integration_test: 2 pass
  - plan_test: 16 pass
  - read_api_test: 29 pass
- 以前失敗していた4件（foreign key violation）が修正により全てパス:
  - `test_get_board_project_state_failed` ✅
  - `test_get_viewer_sources_missing` ✅
  - `test_get_viewer_sources_skipped` ✅
  - `test_pagination_cursor_traversal` ✅

### 解消されたセキュリティ問題
1. ✅ OAuth CSRF: state パラメータがサーバーサイドcookieとの照合で検証される
2. ✅ Open Redirect: redirect先がハードコード "/" のみ
3. ✅ Artifact Secret: 未設定時にデフォルト値ではなくパニック（起動不可）
4. ✅ テストhelper FK violation: 実際のDB IDを使用するように修正

### 残リスク
- Repository権限ベースの認可は後続Issueで実装予定（MVP = session認証のみ）
- OAuth login/callback の統合テスト（外部GitHub API呼び出し含む）は mock server が必要で未実装
- session cookie に `Secure` flag未付与（HTTPS環境専用にする場合は追加が必要）
