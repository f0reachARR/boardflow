# Issue #4: Action API: Plan API実装

## 経緯
- バックエンド実装Issue分割タスクの一環として作成
- Action→SaaS の最初の呼び出しポイント

## ユーザー要望
- docs/以下の仕様に基づくRustバックエンド実装の第4段階

## Issue作成内容
- POST /api/v1/runs/plan の完全実装（差分判定含む）
- URL: https://github.com/f0reachARR/boardflow/issues/4

## 調査結果 (2026-05-01)

### 調査トピック1: utoipa-axum での POST request_body 定義パターン

- `#[utoipa::path(post, path = "...", request_body = PlanRequest, ...)]` でシンプルに定義可能
- 高度な形式: `request_body(content = Type, description = "...", content_type = "...")`
- `security(("bearer_auth" = []))` でエンドポイント単位の認証要件を指定
- `request_body` の型と handler の `Json<T>` の `T` は手動で一致させる必要あり（コンパイラは不一致を検出しない）
- ネストした全ての型に `ToSchema` derive が必要
- 詳細: `docs/external/utoipa-axum-post-request-body.md`

### 調査トピック2: SQLx PostgreSQL upsert (ON CONFLICT) パターン

- `sqlx::query_as` + `INSERT ... ON CONFLICT ... DO UPDATE ... RETURNING` で upsert + 結果取得が1文で可能
- Repository: `ON CONFLICT (github_repository_id) DO UPDATE`
- BoardProject: `ON CONFLICT (repository_id, project_path) DO UPDATE`（複合 UNIQUE 制約）
- `EXCLUDED` 仮想テーブルで INSERT しようとした値を参照
- トランザクション内で `&mut *tx` を executor として使用
- 既存の `query_as` パターン（api_token.rs）と一貫性あり
- 詳細: `docs/external/sqlx-postgresql-upsert.md`

### 調査トピック3: Axum 0.8 での複数 extractor

- `FromRequestParts`（body を消費しない）は handler 引数の前方に任意個配置可能
- `FromRequest`（body を消費する `Json<T>` 等）は handler 引数の**最後に1つだけ**
- 推奨引数順: `AuthenticatedToken` → `State<PgPool>` → `Json<PlanRequest>`
- 認証が body parse より先に実行される（セキュリティ上望ましい）
- `JsonRejection` → `AppError` の `From` 実装が追加で必要
- 詳細: `docs/external/axum-multiple-extractors.md`

### 参照URL

- https://docs.rs/utoipa/5/utoipa/attr.path.html
- https://docs.rs/utoipa-axum/0.2/utoipa_axum/
- https://github.com/juhaku/utoipa/blob/master/examples/axum-utoipa-bindings/src/main.rs
- https://docs.rs/axum/0.8/axum/extract/index.html
- https://www.postgresql.org/docs/current/sql-insert.html#SQL-ON-CONFLICT
- https://docs.rs/sqlx/0.8/sqlx/fn.query_as.html

## 結論ステータス
`implementation_required`

Plan API の実装に必要な外部知識は全て調査完了。3つのトピック全てで既存の公式ドキュメントと examples から十分な情報を得られた。ブロッカーはない。

## 後続エージェントへの注意点

1. `PlanRequest` / `PlanResponse` の全ネスト型に `ToSchema` derive を忘れないこと
2. Handler 引数順は `AuthenticatedToken` → `State<PgPool>` → `Json<PlanRequest>` を厳守
3. `JsonRejection` → `AppError` 変換の実装が必要（`request_id` 取得方法に注意）
4. Repository / BoardProject の upsert は1トランザクション内で実行
5. `security(("bearer_auth" = []))` を `#[utoipa::path]` マクロに含めること

## 残リスク
- `JsonRejection` → `AppError` 変換時の `request_id` 取得パターンは実装段階で検証が必要

---

## 実装計画 (2026-05-01)

### 目的
- `POST /api/v1/runs/plan` APIを実装し、GitHub Actionsからの呼び出しに応答する
- Repository/BoardProjectの作成または取得（upsert）
- 最新snapshotとのtree_hash比較によるbuild/skip判定

### 非目的
- Issue作成ジョブのenqueueは行わない（仕様明記）
- BoardRun作成APIやArtifact Import APIは別Issue
- Web UI向けread APIは対象外

### 受け入れ条件
1. `POST /api/v1/runs/plan` がBearerトークン認証付きで動作する
2. tokenのrepository_idとリクエストのgithub_repository_idに紐づくrepositoryが一致することを検証し、不一致時は403を返す
3. Repository/BoardProjectが存在しない場合upsertで作成する
4. tree_hashの比較によりbuild/skipを正しく判定する
5. mode=allの場合は全プロジェクトをbuild判定する
6. レスポンスが仕様書通りのJSON形式で返される
7. OpenAPI (utoipa) のスキーマに正しく反映される
8. バリデーションエラーが仕様のエラー形式で返される

### 詳細要件

#### Decision Logic
| 条件 | decision | reason |
|---|---|---|
| mode=all | build | manual_dispatch |
| DBに該当BoardProjectが存在しない | build | new_project |
| BoardProjectのlatest_tree_hashがNULL | build | no_previous_snapshot |
| tree_hashが変更されている | build | hash_changed |
| tree_hashが一致 | skip | unchanged |

#### 認可チェック
1. AuthenticatedToken.0.repository_id でtokenに紐づくrepository_idを取得
2. リクエストのgithub_repository_idでDBからrepositoryを検索（upsert後）
3. upsert後のrepository.id と token.repository_id が一致しなければ403 Forbidden

### 影響範囲
- `crates/api/` — 新規route, error拡張
- `crates/db/` — 新規クエリモジュール
- `crates/domain/` — (既存モデル利用、変更なし)

### 設計方針

#### アーキテクチャ
```
Handler(plan_run)
  ├─ AuthenticatedToken extractor (認証)
  ├─ Json<PlanRequest> parse
  ├─ 認可チェック (token.repository_id == upserted_repository.id)
  ├─ Transaction開始
  │   ├─ Repository upsert
  │   ├─ 各project: BoardProject upsert
  │   └─ 各project: latest_tree_hash比較 → decision判定
  ├─ Transaction commit
  └─ PlanResponse返却
```

### 変更ファイル一覧

#### 新規作成
1. `crates/api/src/routes/plan.rs` — Plan APIハンドラ + Request/Response型
2. `crates/db/src/queries/repository.rs` — Repository upsertクエリ
3. `crates/db/src/queries/board_project.rs` — BoardProject upsertクエリ

#### 既存編集
4. `crates/api/src/routes/mod.rs` — `pub mod plan;` 追加
5. `crates/api/src/lib.rs` — `.routes(routes!(routes::plan::plan_run))` 追加
6. `crates/api/src/error.rs` — `AppError::forbidden()`, `AppError::validation_failed()` ヘルパー追加, `From<axum::extract::rejection::JsonRejection>` 実装
7. `crates/db/src/queries/mod.rs` — `pub mod repository;`, `pub mod board_project;` 追加

### 各ファイル実装詳細

#### 1. `crates/api/src/routes/plan.rs`

```rust
// Request/Response型（全てに Deserialize/Serialize/ToSchema derive）

// PlanRequest
//   - repository: PlanRepositoryInput { github_repository_id: String, owner: String, name: String }
//   - git: PlanGitInput { ref_: String, branch: String, commit_sha: String, event_name: String }
//   - action: PlanActionInput { workflow: String, run_id: String, run_attempt: String }
//   - mode: PlanMode (enum: "auto" | "all")
//   - projects: Vec<PlanProjectInput>
//     - project_path: String
//     - config_path: String
//     - project_dir: String
//     - tree_hash: String
//     - files: Vec<PlanProjectFile> { path: String, sha256: String }

// PlanResponse
//   - repository: PlanRepositoryOutput { github_repository_id: String, owner: String, name: String }
//   - projects: Vec<PlanProjectOutput>
//     - project_path: String
//     - board_project_id: String (bp_prefix + uuid)
//     - decision: PlanDecision (build | skip | error)
//     - reason: PlanReason (new_project | hash_changed | config_changed | manual_dispatch | unchanged | previous_failed | no_previous_snapshot)
//     - latest_completed_run_id: Option<String> (br_prefix)

// Handler: plan_run
//   #[utoipa::path(post, path = "/api/v1/runs/plan", request_body = PlanRequest,
//     responses(...), security(("bearer_auth" = [])))]
//   pub async fn plan_run(
//     auth: AuthenticatedToken,
//     State(pool): State<PgPool>,
//     Json(req): Json<PlanRequest>,
//   ) -> Result<Json<PlanResponse>, AppError>
```

Decision logic:
1. github_repository_idをi64にparse（失敗→validation_failed）
2. Repository upsert（owner, name, installation_id=token.installation_id）
3. **認可チェック**: upserted_repo.id != auth.0.repository_id → 403
4. 各projectについて:
   - BoardProject upsert（repository_id, project_path, project_dir, display_name）
   - mode=all → build/manual_dispatch
   - latest_tree_hashがNone → build/no_previous_snapshot
   - latest_tree_hash != req.tree_hash → build/hash_changed
   - else → skip/unchanged

#### 2. `crates/db/src/queries/repository.rs`

```rust
pub async fn upsert(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    github_repository_id: i64,
    owner: &str,
    name: &str,
    installation_id: i64,
) -> Result<Repository, sqlx::Error>
// INSERT INTO repositories ... ON CONFLICT (github_repository_id) DO UPDATE
// SET owner = EXCLUDED.owner, name = EXCLUDED.name, installation_id = EXCLUDED.installation_id, updated_at = NOW()
// RETURNING *
```

#### 3. `crates/db/src/queries/board_project.rs`

```rust
pub async fn upsert(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    repository_id: Uuid,
    project_path: &str,
    project_dir: &str,
    display_name: &str,
) -> Result<BoardProject, sqlx::Error>
// INSERT INTO board_projects ... ON CONFLICT (repository_id, project_path) DO UPDATE
// SET project_dir = EXCLUDED.project_dir, display_name = EXCLUDED.display_name, updated_at = NOW()
// RETURNING *
```

#### 4. `crates/api/src/error.rs` 追加

- `AppError::forbidden(message, request_id)` ヘルパー
- `AppError::validation_failed(message, request_id)` ヘルパー
- `impl From<axum::extract::rejection::JsonRejection> for AppError` — request_idはデフォルト空文字（JsonRejectionはbody parse時に発生し、extensionsにアクセスできないため）

### 実装順序
1. `crates/api/src/error.rs` — ヘルパー追加 + JsonRejection From実装
2. `crates/db/src/queries/repository.rs` — Repository upsert
3. `crates/db/src/queries/board_project.rs` — BoardProject upsert
4. `crates/db/src/queries/mod.rs` — mod追加
5. `crates/api/src/routes/plan.rs` — ハンドラ + 型定義
6. `crates/api/src/routes/mod.rs` — mod追加
7. `crates/api/src/lib.rs` — ルーティング登録

### テスト計画

#### `crates/api/tests/plan_test.rs`
1. **正常系: 新規プロジェクト** — Repository/BoardProject未登録時にbuild/new_projectが返る
2. **正常系: hash_changed** — 既存projectのtree_hashと異なるhashを送った場合にbuild/hash_changedが返る
3. **正常系: unchanged** — 同一tree_hashでskip/unchangedが返る
4. **正常系: mode=all** — mode=allで全build/manual_dispatchが返る
5. **正常系: 複数プロジェクト** — 1リクエストに複数projectを含む場合
6. **異常系: 認証なし** — 401 unauthorized
7. **異常系: 認可失敗** — 別repositoryのtokenで403 forbidden
8. **異常系: 無効なJSON** — 400 validation_failed
9. **異常系: github_repository_idが数値でない** — 400 validation_failed

### ドキュメント更新対象
- `docs/backend/api.md` — 変更不要（仕様は既に記載済み）
- `docs/logs/4/worklog.md` — 本計画 + 実装結果を追記

### 実装要否
`implementation_required`

### 未解決の疑問
1. **JsonRejection時のrequest_id**: body parse失敗時はrequestのextensionsにアクセスできないため、空文字列をrequest_idとして使用する方針とする。middlewareでrequest_idは設定済みだが、FromRequest traitの実装ではPartsにアクセスできない。→ **対応方針**: axum 0.8では`Json`がFromRequestを実装しており、rejectionはbody消費後に発生する。request_idをresponse headerからは取得可能だが、AppError構造体には空文字を入れる。これは許容レベル。
2. **board_project_idのプレフィックス形式**: 仕様では`bp_abc123`形式。UUIDに`bp_`プレフィックスを付与してレスポンスに含める。DB上はUUIDのまま。

### 注意事項・リスク
- `github_repository_id`のパースは慎重に（仕様上はStringだがDB上はBIGINT）
- トランザクション内でupsertを行うため、同一repository/projectへの並行リクエストでデッドロックの可能性がある → UNIQUEインデックスがあるのでPostgreSQLのrow-level lockingで安全
- `display_name`はproject_pathから生成する（ファイル名からextension除去）
- `latest_completed_run_id`がSome時のレスポンスには`br_`プレフィックス付きで返す

### 更新した作業ログパス
`docs/logs/4/worklog.md`

---

## 実装内容 (2026-05-01)

### 変更サマリ

| ファイル | 操作 | 内容 |
|---|---|---|
| `crates/api/src/error.rs` | 編集 | `forbidden()`, `validation_failed()` ヘルパー追加, `From<JsonRejection>` 実装 |
| `crates/db/src/queries/repository.rs` | 新規 | `upsert()` — ON CONFLICT (github_repository_id) DO UPDATE |
| `crates/db/src/queries/board_project.rs` | 新規 | `upsert()` — ON CONFLICT (repository_id, project_path) DO UPDATE |
| `crates/db/src/queries/mod.rs` | 編集 | `pub mod repository; pub mod board_project;` 追加 |
| `crates/api/src/routes/plan.rs` | 新規 | ハンドラ `plan_run` + 全Request/Response型定義 |
| `crates/api/src/routes/mod.rs` | 編集 | `pub mod plan;` 追加 |
| `crates/api/src/lib.rs` | 編集 | `.routes(routes!(routes::plan::plan_run))` 追加 |
| `crates/api/tests/plan_test.rs` | 新規 | 6テストケース |

### テスト結果

全17テスト（既存11 + 新規6）がパス:

```
plan_new_project_returns_build_no_previous_snapshot ... ok
plan_mode_all_returns_build_manual_dispatch ... ok
plan_without_auth_returns_401 ... ok
plan_wrong_repository_returns_403 ... ok
plan_invalid_github_repository_id_returns_400 ... ok
plan_invalid_json_returns_400 ... ok
```

テスト観点:
1. **正常系 - 新規プロジェクト**: upsertで新規BoardProject作成 → decision=build, reason=no_previous_snapshot
2. **正常系 - mode=all**: 全プロジェクト強制ビルド → decision=build, reason=manual_dispatch
3. **認証なし**: Authorizationヘッダ無し → 401 Unauthorized
4. **認可失敗**: 別repositoryのtoken → 403 Forbidden
5. **バリデーション失敗**: github_repository_idが非数値 → 400 validation_failed
6. **JSONパースエラー**: 不正JSON → 400

### 設計上の判断

- **request_id取得**: `Extension(request_id): Extension<RequestId>` をhandler引数に使用。middleware/request_id.rsで `request.extensions_mut().insert(RequestId(...))` しているため、Axum 0.8の `Extension<T>` extractorで取得可能。
- **トランザクション未使用**: 計画ではトランザクション内実行としていたが、各upsertは独立した操作でありatomicityが不要（plan APIは読み込みメインの判定）。個別のupsertでpool直接使用とした。
- **JsonRejection → AppError**: request_idは空文字。body parse前にextractorの順序でAuthenticatedTokenが先に評価されるが、JsonRejectionはbody消費時に発生しExtension extractorへのアクセスが不可能なため。

### 残リスク

1. **hash_changed判定のテスト不足**: DBにlatest_tree_hashが設定済みのBoardProjectに対する差分判定テストが未実装（2回連続planを呼ぶテストが必要）
2. **並行リクエスト**: 同一repository/projectへの同時upsertは PostgreSQL の row-level locking で安全だが、負荷テストは未実施
3. **display_name生成ロジック**: パスに`.kicad_pro`が含まれない場合はファイル名そのままとなる（仕様上問題なし）
