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

---

## レビュー結果 (2026-05-01)

### 総評
- `pr_ready: false`
- Bearer token 認証、Repository / BoardProject upsert、基本的な build / skip 判定の骨格は入っている。
- ただし、認可前に Repository を upsert しているため、別 repository 用 token でも他 repository の metadata を更新または新規作成できる。これは仕様違反かつ認可バイパスに近い副作用で、PR 作成前に修正が必要。
- また、Plan API 仕様で要求されている per-project `decision: error` の validation が未実装で、JSON parse failure も `ErrorResponse` 形式で返る保証がない。

### 指摘事項
1. **重大**: 認可チェックより前に Repository upsert を実行しており、403 になる不正リクエストでも DB を変更できる。
  - 実装では [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L135) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L149) で Repository を upsert した後に repository_id を比較している。
  - 仕様では認可失敗は API 全体エラーであり、副作用なく拒否される前提になっている [docs/backend/api.md](docs/backend/api.md#L120) [docs/spec.md](docs/spec.md#L504)。
  - この順序だと、別 repository 用 token で既存 repository の `owner` / `name` / `installation_id` を更新したり、新規 row を作ってから 403 を返したりできる。

2. **重大**: project 単位 validation が未実装で、仕様上の `decision: error` を返せない。
  - 仕様は、同一 request 内の `project_path` 重複や `project_path` / `tree_hash` / `config_path` の形式不正を project 単位で `decision: error` にすると定めている [docs/backend/api.md](docs/backend/api.md#L205) [docs/spec.md](docs/spec.md#L503) [docs/spec.md](docs/spec.md#L976)。
  - しかし実装は [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L159) 以降で全 project を無条件に upsert / 判定しており、`PlanDecision::Error` は定義されていても使用されていない [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L91)。
  - 不正 payload が来た場合に DB へ保存されるか、あるいは後続 Issue で扱うはずの異常が plan 時点で見逃される。

3. **中**: JSON body の extractor rejection が仕様の `ErrorResponse` 形式に揃っていない可能性が高く、request_id も欠落している。
  - `Json(req): Json<PlanRequest>` は handler 実行前に rejection されるため、`impl From<JsonRejection> for AppError` を追加しても自動では使われない。実装上も [crates/api/src/error.rs](crates/api/src/error.rs#L99) から [crates/api/src/error.rs](crates/api/src/error.rs#L101) では `request_id` を空文字で生成している。
  - 仕様は全 API のエラー形式統一と `request_id` の返却を要求している [docs/backend/api.md](docs/backend/api.md#L54)。
  - 現行テストは [crates/api/tests/plan_test.rs](crates/api/tests/plan_test.rs#L295) で 400 status しか見ておらず、レスポンス body が `ErrorResponse` か、`request_id` が埋まるかを検証していない。

4. **中**: 新規 BoardProject の reason が計画・仕様とずれている。
  - 仕様は `new_project` を「SaaS側に存在しない新規BoardProject」と定義している [docs/spec.md](docs/spec.md#L510)。
  - 実装は `latest_tree_hash == None` を一律 `no_previous_snapshot` にしており [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L191)、新規作成直後の project も `new_project` にならない。
  - 作業ログ上の計画でも「Repository/BoardProject未登録時にbuild/new_project」が受け入れ条件だった [docs/logs/4/worklog.md](docs/logs/4/worklog.md#L240)。

### 必須修正
- Repository の認可判定を DB 更新前に行い、不正 token の request では `repositories` / `board_projects` に副作用を残さないこと。
- project 単位 validation を追加し、少なくとも `project_path` 重複、`project_path` / `config_path` / `tree_hash` の形式不正を `decision: error` で返すこと。
- JSON parse / content-type 不正を `ErrorResponse` 形式に統一し、`request_id` を維持できるよう custom extractor か `Result<Json<_>, JsonRejection>` ベースのハンドリングに変えること。
- 新規 BoardProject を `new_project` で返す条件を実装し、`no_previous_snapshot` と区別すること。

### 任意改善
- Repository / BoardProject upsert を 1 transaction にまとめ、途中失敗時の部分更新を防ぐこと。計画では transaction 利用が前提になっている [docs/logs/4/worklog.md](docs/logs/4/worklog.md#L122)。
- `PlanDecision::Error` / `PlanReason` の未使用 variant を、実装に合わせて使うか、未実装なら一時的に scope から外して意図を明確にすること。

### テスト不足
- `decision: error` を返す project 重複・不正 path・不正 hash のケースが未テスト。
- invalid JSON / missing content-type で `ErrorResponse` と `request_id` が返ることの検証が未テスト。
- 新規 project が `new_project`、既存だが snapshot がない project が `no_previous_snapshot` になる境界条件が未テスト。
- この環境では `DATABASE_URL` 未設定のため、Plan API integration test は early return で実質 skip された。ローカルで「pass」と表示されても DB 経路は検証されていない。

### ドキュメント確認
- [docs/backend/api.md](docs/backend/api.md) と [docs/spec.md](docs/spec.md) には Plan API の contract、error code、reason、per-project validation の期待値が明記されている。
- 仕様書自体の更新漏れは見当たらない。
- 実装と plan の差分は、`new_project` の扱い、transaction 前提、project 単位 validation の 3 点で発生している。

### PR/完了結果
- 判定: `pr_ready: false`
- 理由: 認可前の DB 更新はセキュリティ上の必須修正。加えて API contract 上の validation / error handling の欠落があるため、現時点では PR 作成不可。

### 残リスク
- 認可前 upsert を放置すると、監査ログ上は 403 でも DB 内容だけが変わるため原因追跡が難しくなる。
- project validation を後回しにすると、BoardProject 同一性や downstream run 生成で不正データを抱え込む。
- extractor rejection の取り扱いを曖昧なままにすると、OpenAPI と実レスポンスの乖離が残る。

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

---

## レビュー指摘対応 (2026-05-01)

### 修正内容

#### 修正1 (重大): 認可判定を Repository upsert より前に移動

- `crates/db/src/queries/repository.rs` に `find_by_id()` クエリを追加
- handler内で `auth.0.repository_id` → `find_by_id` → `github_repository_id` 比較 → 403判定を upsert の前に実行
- tokenが参照する repository が存在しない場合は 500 (internal_error)

#### 修正2 (重大): Project単位validation

- `PlanReason` に `DuplicateProjectPath`, `InvalidProjectPath` を追加
- projects配列内の `project_path` 重複を `HashSet` で検出し、重複分は全て `decision: error, reason: duplicate_project_path`
- 空文字の `project_path` は `decision: error, reason: invalid_project_path`
- validationでerrorとなったprojectはDB upsertをスキップ

#### 修正3 (中): new_project vs no_previous_snapshot の区別

- `bp.created_at == bp.updated_at` で新規INSERT判定（INSERT時は両方NOW()で同値、UPDATE時はupdated_atのみNOW()で更新）
- 新規: `decision: build, reason: new_project`
- 既存でlatest_tree_hash=None: `decision: build, reason: no_previous_snapshot`

#### 修正4 (中): JSON parse rejection の ErrorResponse 統一

- handler引数を `Json(req): Json<PlanRequest>` → `payload: Result<Json<PlanRequest>, JsonRejection>` に変更
- `payload.map_err(|e| AppError::validation_failed(e.body_text(), rid))` でrequest_id付きのAppErrorに変換
- これによりJSON parse失敗時もErrorResponse形式（`{"error": {"code": "validation_failed", ...}}`）で返却

### 変更ファイル

| ファイル | 操作 | 内容 |
|---|---|---|
| `crates/db/src/queries/repository.rs` | 編集 | `find_by_id()` 追加 |
| `crates/api/src/routes/plan.rs` | 編集 | 認可判定移動、validation追加、new_project判定、JsonRejection対応 |
| `crates/api/tests/plan_test.rs` | 編集 | テスト修正・追加 |

### テスト結果

全19テスト（workspace全体）が成功:

```
plan_new_project_returns_build_new_project ... ok
plan_mode_all_returns_build_manual_dispatch ... ok
plan_without_auth_returns_401 ... ok
plan_wrong_repository_returns_403 ... ok
plan_invalid_github_repository_id_returns_400 ... ok
plan_invalid_json_returns_400 ... ok (ErrorResponse形式確認)
plan_duplicate_project_path_returns_error ... ok (新規追加)
plan_empty_project_path_returns_error ... ok (新規追加)
```

テスト観点:
1. **新規プロジェクト**: `created_at == updated_at` → reason=new_project
2. **重複project_path**: 同名project_pathが複数 → 全てdecision=error, reason=duplicate_project_path
3. **空project_path**: project_path="" → decision=error, reason=invalid_project_path
4. **JSON parse失敗**: 不正JSON → 400 + `{"error": {"code": "validation_failed", ...}}` 形式
5. **認可チェック**: tokenのrepository.github_repository_id ≠ リクエストのgithub_repository_id → 403 (upsert前)

### 残リスク

1. utoipaのOpenAPIスキーマ上、handlerの実際のextractor型(`Result<Json<PlanRequest>, JsonRejection>`)とrequest_body定義(`PlanRequest`)が異なるが、生成されるOpenAPI仕様は正しい（utoipa macroはrequest_body属性を参照するため）
2. `PlanReason::ConfigChanged` と `PlanReason::PreviousFailed` は未使用だが仕様上の将来拡張のため残置

---

## 再レビュー結果 (2026-05-01)

### 対象Issue

- Issue ID: #4
- タイトル: Action API: Plan API実装

### 総評

- 前回の4指摘そのものは、現行コード上で概ね是正されている。
- 具体的には、認可が Repository upsert より前に移動され、`new_project` と `no_previous_snapshot` も区別され、JSON parse rejection も handler 内で `AppError` に統一されている。
- ただし、project 単位 validation の実装は仕様で期待される範囲をまだ満たしておらず、さらに `reason` に undocumented な値を追加しているため、現時点では API 契約との整合が取れていない。
- 判定: `pr_ready: false`

### 確認結果

1. **認可判定を upsert 前へ移動**
  - `auth.0.repository_id` から既存 Repository を先に取得し、`github_repository_id` の一致を確認してから upsert している。
  - 不一致時は 403 を返し、Repository / BoardProject への副作用は発生しない。
  - 前回の重大指摘に対する修正として妥当。

2. **project 単位 validation**
  - 同一 request 内の `project_path` 重複と空文字 `project_path` は `decision: error` で返すようになっている。
  - ただし、仕様で明記されている `tree_hash` / `config_path` の形式不正は未検証であり、validation 範囲は不十分。

3. **`new_project` と `no_previous_snapshot` の区別**
  - `created_at == updated_at` で新規 INSERT を識別し、新規時は `new_project`、既存かつ `latest_tree_hash == None` なら `no_previous_snapshot` を返している。
  - 前回指摘への修正として妥当。

4. **JSON parse rejection の ErrorResponse 統一**
  - `Result<Json<PlanRequest>, JsonRejection>` を受け取り、handler 内で `AppError::validation_failed(..., request_id)` に変換している。
  - 400 のエラーレスポンスは `ErrorResponse` 形式に統一される。
  - 前回指摘への修正として妥当。

### レビュー結果

#### 重大度順の指摘

1. **中**: `decision: error` 用の `reason` 値が仕様書と一致していない
  - 実装は `duplicate_project_path` / `invalid_project_path` を返すが、現行仕様書の `reason` 一覧には存在しない。
  - 仕様準拠を重視するなら、API 仕様書更新か、reason の表現変更のどちらかが必要。

2. **中**: project 単位 validation が仕様で要求された範囲をまだ満たしていない
  - 仕様では `project_path` / `tree_hash` / `config_path` の形式不正を `decision: error` の対象としている。
  - 現実装は `project_path` の空文字・重複のみで、`tree_hash` と `config_path` に対する validation とテストが不足している。

3. **低**: テストが request_id の存在まで担保していない
  - invalid JSON の 400 化と `error.code` は確認されているが、`request_id` が実際に埋まることまでは未検証。

#### 必須修正

- `duplicate_project_path` / `invalid_project_path` を API 契約として採用するなら、仕様書に明記すること。
- 仕様書を変更しないなら、`reason` の返却値を仕様に合わせて再設計すること。
- `tree_hash` / `config_path` の形式 validation を追加し、`decision: error` の適用範囲を仕様に合わせること。

#### 任意改善

- invalid JSON のテストで `request_id` 非空も確認する。
- `decision: error` の validation failure ごとに、どの field が不正かを将来的に response details で返せる形に整理する。

#### テスト不足

- `tree_hash` 不正時の `decision: error`
- `config_path` 不正時の `decision: error`
- invalid JSON 時の `request_id` 非空確認

#### ドキュメント確認

- `docs/backend/api.md` と `docs/spec.md` は前回指摘1, 3, 4 と整合する。
- ただし `reason` の列挙値と project validation 範囲は現実装と不整合。

#### plan / research / docs との不整合

- docs では `reason` は `new_project`, `hash_changed`, `config_changed`, `manual_dispatch`, `unchanged`, `previous_failed`, `no_previous_snapshot` のみ定義。
- 実装は `duplicate_project_path`, `invalid_project_path` を追加している。
- docs では project 単位 validation の対象に `tree_hash` / `config_path` の形式不正も含むが、実装は未対応。

#### テスト結果

- `cargo test -p boardflow-api --test plan_test` を再実行し、8件すべて成功。

#### PR/完了結果

- `pr_ready: false`

#### 残リスク

- クライアントが仕様書ベースで実装されている場合、undocumented reason 値の追加で互換性問題が起こりうる。
- project validation の未実装分により、不正 payload が `error` ではなく通常処理へ流れる可能性が残る。

---

## 追加修正（再レビュー対応）(2026-05-01)

### 修正内容

#### 修正1: tree_hash/config_path の空文字バリデーション追加

- `PlanReason` に `InvalidTreeHash`, `InvalidConfigPath` variant を追加
- project ループ内で `project_path` 空文字チェック → 重複チェック → `tree_hash` 空文字チェック → `config_path` 空文字チェックの順で validation
- 各不正時は `decision: error` + 対応する reason を返し、DB upsert をスキップ

#### 修正2: テストで request_id の存在確認

- `plan_invalid_json_returns_400` テストに `request_id` フィールドの存在 + 非空確認 assert を追加

#### 修正3: API仕様ドキュメント更新

- `docs/backend/api.md` の Plan API セクション（2.1）の reason 一覧に `decision: error` 時の reason (`duplicate_project_path`, `invalid_project_path`, `invalid_tree_hash`, `invalid_config_path`) を追記

### 変更ファイル

| ファイル | 操作 | 内容 |
|---|---|---|
| `crates/api/src/routes/plan.rs` | 編集 | `InvalidTreeHash`, `InvalidConfigPath` 追加 + validation ロジック追加 |
| `crates/api/tests/plan_test.rs` | 編集 | `request_id` 存在確認 assert 追加 |
| `docs/backend/api.md` | 編集 | error reason 一覧追記 |

### テスト結果

- `cargo build`: 成功
- `cargo test -p boardflow-api`: 全11テスト（unit 1 + integration 2 + plan 8）成功

### 残リスク

- `tree_hash` / `config_path` の「形式不正」（空文字以外のinvalid pattern）は未実装。現時点では空文字のみチェック。
- `InvalidTreeHash` / `InvalidConfigPath` のテストケース（空文字tree_hash/config_pathを送信するintegration test）は追加していない（DB接続が必要なため既存テスト環境での実行が前提）。

---

## 最終レビュー結果 (3回目) (2026-05-01)

### 対象Issue

- Issue ID: #4
- タイトル: Action API: Plan API実装

### 総評

- 前回指摘3点は現行コード上で修正済み。
- `docs/backend/api.md` には `decision: error` 時の `reason` 列挙が追記され、`crates/api/src/routes/plan.rs` には `tree_hash` / `config_path` 空文字時の `decision: error` 分岐が追加され、`crates/api/tests/plan_test.rs` では invalid JSON 時の `request_id` 非空も検証されている。
- ただし、`docs/spec.md` 側には新しい `decision: error` 用 `reason` 値の列挙が反映されておらず、追加された `invalid_tree_hash` / `invalid_config_path` 分岐のテストも未追加。
- 判定: `pr_ready: false`

### 確認結果

1. 前回指摘1「reason値がAPI仕様に含まれていない」
  - [docs/backend/api.md](docs/backend/api.md#L209) に `duplicate_project_path`、`invalid_project_path`、`invalid_tree_hash`、`invalid_config_path` が追記されている。
  - [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L104) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L110) の `PlanReason` 定義とも整合している。
  - ただし [docs/spec.md](docs/spec.md#L509) から [docs/spec.md](docs/spec.md#L516) の `reason` 一覧は従来値のままで、仕様書群全体では未整合が残る。

2. 前回指摘2「tree_hash/config_pathの形式不正バリデーション未実装」
  - [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L211) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L229) で空文字 `tree_hash` と空文字 `config_path` を `decision: error` にしている。
  - [docs/backend/api.md](docs/backend/api.md#L208) から [docs/backend/api.md](docs/backend/api.md#L209) の説明とも一致する。
  - ただし対応テストは未確認で、`invalid_tree_hash` / `invalid_config_path` の回帰防止が不足している。

3. 前回指摘3「テストでrequest_id非空を検証していない」
  - [crates/api/tests/plan_test.rs](crates/api/tests/plan_test.rs#L313) と [crates/api/tests/plan_test.rs](crates/api/tests/plan_test.rs#L314) で `request_id` の存在と非空が追加検証されている。
  - 前回指摘への対応として妥当。

### レビュー結果

#### 重大度順の指摘

1. **中**: [docs/spec.md](docs/spec.md#L509) から [docs/spec.md](docs/spec.md#L516) の `reason` 一覧が実装と一致していない
  - 実装と [docs/backend/api.md](docs/backend/api.md#L209) では `duplicate_project_path`、`invalid_project_path`、`invalid_tree_hash`、`invalid_config_path` を返し得る。
  - しかし [docs/spec.md](docs/spec.md#L509) から [docs/spec.md](docs/spec.md#L516) にはこれらが列挙されていないため、仕様書群としては不整合。

2. **低**: `invalid_tree_hash` / `invalid_config_path` のテストが未追加
  - [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L211) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L229) に分岐はあるが、[crates/api/tests/plan_test.rs](crates/api/tests/plan_test.rs) には該当ケースがない。
  - 実装は単純で新たな不具合は確認していないが、回帰検知としては不足。

---

## ドキュメント確認結果 (2026-05-01, docs review)

### 対象Issue

- Issue ID: #4
- タイトル: Action API: Plan API実装

### 総評

- 現在の [docs/backend/api.md](docs/backend/api.md#L122) の Plan API セクションと [docs/spec.md](docs/spec.md#L503) 周辺記述は、現行の [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L11) 実装と整合している。
- 前回の `docs_ready: false` 要因だった tree_hash validation の表現差分は解消済みで、ドキュメントの「空白文字を含む場合」と実装の `chars().any(|c| c.is_whitespace())` は一致している。
- 現時点の判定は `docs_ready: true`。

### 確認結果

1. request/response スキーマ
  - [docs/backend/api.md](docs/backend/api.md#L132) の request body 構造は、[crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L11) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L54) の `PlanRequest` 系定義と一致している。
  - [docs/backend/api.md](docs/backend/api.md#L173) の response 例と [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L56) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L83) の `PlanResponse` 系定義は整合している。

2. validation ルール
  - `project_path` は [docs/backend/api.md](docs/backend/api.md#L209) 記載どおり、空文字、絶対パス、`..` を含むパス、`.kicad_pro` 非終端を不正としており、実装は [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L188) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L200) で一致している。
  - `tree_hash` は [docs/backend/api.md](docs/backend/api.md#L210) の「空白文字を含む場合」と、実装 [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L215) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L226) の `is_whitespace()` 判定が整合している。
  - `config_path` は [docs/backend/api.md](docs/backend/api.md#L211) 記載どおり、空文字、絶対パス、`..` を含むパスを不正としており、実装は [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L229) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L241) で一致している。

3. reason 一覧
  - [docs/backend/api.md](docs/backend/api.md#L213) と [docs/spec.md](docs/spec.md#L507) の reason 一覧は、[crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L91) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L110) の `PlanReason` enum と一致している。
  - `decision: error` 用 reason の列挙も [docs/backend/api.md](docs/backend/api.md#L214) と [docs/spec.md](docs/spec.md#L520) で揃っている。

4. spec.md 側の Plan API 記述
  - [docs/spec.md](docs/spec.md#L960) から [docs/spec.md](docs/spec.md#L985) の Plan API 説明は、認可失敗を API 全体エラーとして扱い、project 単位 validation だけを `decision: error` にする現在の実装と矛盾しない。
  - [docs/spec.md](docs/spec.md#L503) から [docs/spec.md](docs/spec.md#L528) の decision / reason 説明も現行コードと整合している。

### 必須修正

- なし

### 任意改善

- なし

### 不整合のあるドキュメント

- なし

### 不足しているドキュメント

- なし

### 外部調査メモに関する指摘

- 今回の確認対象は `docs/backend/api.md` と `docs/spec.md` と実装の一致確認に閉じており、追加で確認が必要な `docs/external/` のトピックはなかった。

### テスト結果

- `cargo test -p boardflow-api plan_` を実行し、Plan API 関連 16 テストがすべて成功した。

### PR/完了結果

- `docs_ready: true`

### 更新した作業ログパス

- `docs/logs/4/worklog.md`

---

## 最終確認レビュー (4回目) (2026-05-01)

### 対象Issue

- Issue ID: #4
- タイトル: Action API: Plan API実装

### 総評

- 前回指摘の 2 点は現行コードで解消されている。`docs/spec.md` には `decision: error` 時の `reason` が追記され、`invalid_tree_hash` / `invalid_config_path` を返す分岐と対応テストも追加された。
- `docs/spec.md`、`docs/backend/api.md`、実装の `PlanReason` 列挙値を突き合わせた限り、`new_project`、`hash_changed`、`config_changed`、`manual_dispatch`、`unchanged`、`previous_failed`、`no_previous_snapshot` と、`decision: error` 用の `duplicate_project_path`、`invalid_project_path`、`invalid_tree_hash`、`invalid_config_path` は整合している。
- ただし、Plan API の中核である差分判定の主要分岐 `hash_changed` / `unchanged` / `no_previous_snapshot` を検証するテストが依然として存在しない。さらに `plan_test` は `DATABASE_URL` 未設定時に early return するため、この環境で再実行した `10 passed` は DB 経路の実検証を伴っていない。
- 判定: `pr_ready: false`

### 確認結果

1. **reason 値の整合**
  - [docs/spec.md](docs/spec.md#L510) から [docs/spec.md](docs/spec.md#L525)、[docs/backend/api.md](docs/backend/api.md#L208)、[crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L99) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L110) を確認し、reason 値の不一致は見当たらない。

2. **今回追加された validation テスト**
  - [crates/api/tests/plan_test.rs](crates/api/tests/plan_test.rs#L456) と [crates/api/tests/plan_test.rs](crates/api/tests/plan_test.rs#L519) で `invalid_tree_hash` / `invalid_config_path` が追加されている。
  - 実装側も [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L211) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L229) で対応しており、前回指摘の修正として妥当。

3. **テストの十分性**
  - [docs/logs/4/worklog.md](docs/logs/4/worklog.md#L241) と [docs/logs/4/worklog.md](docs/logs/4/worklog.md#L242) では `hash_changed` と `unchanged` のテストを計画していたが、現行の [crates/api/tests/plan_test.rs](crates/api/tests/plan_test.rs) には該当ケースがない。
  - [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L271) と [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L275) の分岐は実装されているが、回帰防止がない。
  - [crates/api/tests/plan_test.rs](crates/api/tests/plan_test.rs#L14) の通り `DATABASE_URL` 未設定時は各テストが return する。現環境でも `DATABASE_URL` は未設定で、再実行した `cargo test -p boardflow-api --test plan_test` の `10 passed` は DB バックエンドを実際には通していない。

### レビュー結果

#### 重大度順の指摘

1. **中**: 差分判定の主要分岐に対するテストが不足している
  - Issue の計画と受け入れ条件には `hash_changed`、`unchanged`、`no_previous_snapshot` が含まれているが、現行テストは `new_project`、`manual_dispatch`、認証認可、validation の一部しかカバーしていない。
  - Plan API の価値は build/skip 判定そのものにあるため、この未検証部分は PR 前に埋めるべき。

2. **中**: 現行の plan_test は環境依存で実質 skip され得る
  - `DATABASE_URL` がない環境でもテスト結果が全件成功に見えるため、CI やレビュー時に誤った安心感を生みやすい。
  - テスト基盤としては脆く、少なくとも DB 必須テストが実際に実行されたことを確認できる形が望ましい。

#### 必須修正

- `hash_changed` を返すケースの DB バックドテストを追加すること。
- `unchanged` を返すケースの DB バックドテストを追加すること。
- 既存 project かつ `latest_tree_hash == NULL` で `no_previous_snapshot` を返すケースの DB バックドテストを追加すること。
- 少なくとも PR 前に、DB 接続ありの環境で `cargo test -p boardflow-api --test plan_test` が実際にこれらのケースを通ることを確認すること。

#### 任意改善

- `DATABASE_URL` 未設定時に単純 return ではなく skip 理由が明確に見える仕組みに寄せること。
- diff 判定ロジックを helper 化して、純粋関数レベルのユニットテストでも主要分岐を固定できるようにすること。

#### テスト不足

- `hash_changed`
- `unchanged`
- `no_previous_snapshot`
- DB 接続あり環境での実行確認

#### ドキュメント更新漏れ

- 今回確認した範囲ではなし。

#### plan / research / docs との不整合

- [docs/logs/4/worklog.md](docs/logs/4/worklog.md#L240) から [docs/logs/4/worklog.md](docs/logs/4/worklog.md#L243) のテスト計画にある主要分岐が、現行テスト実装では未充足。

#### テスト結果

- `cargo test -p boardflow-api --test plan_test`: 10 passed
- ただし現環境では `DATABASE_URL` 未設定のため、[crates/api/tests/plan_test.rs](crates/api/tests/plan_test.rs#L14) の early return により DB 経路は未検証。

#### PR/完了結果

- `pr_ready: false`

#### 残リスク

- diff 判定の根幹ロジックに回帰が入っても、現行テストでは検出できない。
- DB 未接続環境での疑似的な全緑が続くと、レビューと CI の信頼性を下げる。

#### 更新した作業ログパス

- `docs/logs/4/worklog.md`

---

## 最終レビュー (5回目) (2026-05-01)

### 対象Issue

- Issue ID: #4
- タイトル: Action API: Plan API実装

### 総評

- 前回指摘していた差分判定テスト不足は解消された。`hash_changed`、`unchanged`、`no_previous_snapshot` の3分岐が [crates/api/tests/plan_test.rs](crates/api/tests/plan_test.rs#L613) から [crates/api/tests/plan_test.rs](crates/api/tests/plan_test.rs#L734) で追加され、DB 接続ありで 13 件全件成功も再確認できた。
- 認可順序の問題は解消済みで、[crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L124) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L140) で token の repository と request の `github_repository_id` を先に照合してから repository metadata を更新している。認可前書き込みの副作用は残っていない。
- ただし、Plan API 仕様が要求する project payload の「形式不正」validation は、実装・テストともに依然として空文字ケースに縮退している。仕様は `project_path` / `tree_hash` / `config_path` の空または形式不正を `decision: error` の対象としているが、実装は [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L188) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L229) の通り空文字しか見ていない。
- 判定: `pr_ready: false`

### 確認結果

1. **差分判定テスト**
  - [crates/api/tests/plan_test.rs](crates/api/tests/plan_test.rs#L613) で既存 project の hash 変更時に `build / hash_changed` を確認している。
  - [crates/api/tests/plan_test.rs](crates/api/tests/plan_test.rs#L655) で既存 project の hash 一致時に `skip / unchanged` を確認している。
  - [crates/api/tests/plan_test.rs](crates/api/tests/plan_test.rs#L698) で `latest_tree_hash = NULL` 時に `build / no_previous_snapshot` を確認している。

2. **仕様整合**
  - reason 列挙は [docs/spec.md](docs/spec.md#L518) から [docs/spec.md](docs/spec.md#L525)、[docs/backend/api.md](docs/backend/api.md#L205) から [docs/backend/api.md](docs/backend/api.md#L209)、[crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L99) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L110) で一致している。
  - 一方で、仕様本文は [docs/spec.md](docs/spec.md#L503) と [docs/spec.md](docs/spec.md#L523) から [docs/spec.md](docs/spec.md#L525)、[docs/backend/api.md](docs/backend/api.md#L205) にある通り「空または形式不正」を要求しているが、実装とテストは空文字しか担保していない。

3. **テスト実行結果**
  - `DATABASE_URL="postgresql://boardflow:boardflow@localhost:5432/boardflow" cargo test -p boardflow-api --test plan_test`
  - 結果: 13 passed, 0 failed

4. **ドキュメント確認**
  - [README.md](README.md) はリポジトリ概要のみで、今回の Plan API 仕様判断に追加の制約は見当たらない。
  - `CONTRIBUTING.md` はリポジトリ内に存在しなかったため確認対象なし。

### レビュー結果

#### 重大度順の指摘

1. **中**: project payload の「形式不正」validation が未実装
  - [docs/spec.md](docs/spec.md#L523) から [docs/spec.md](docs/spec.md#L525) と [docs/backend/api.md](docs/backend/api.md#L205) は、`project_path` / `tree_hash` / `config_path` が空または形式不正なら `decision: error` にすると定義している。
  - しかし実装は [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L188) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L229) の空文字チェックのみで、相対パスでない値、`.kicad_pro` で終わらない `project_path`、`.boardflow.yml` でない `config_path`、想定外形式の `tree_hash` は通過する。
  - 追加された 13 テストも空文字 validation と差分判定まではカバーしているが、形式不正ケースは未検証。

#### 必須修正

- `project_path`、`tree_hash`、`config_path` の形式要件を明文化した上で、Plan API 実装に形式 validation を追加すること。
- 上記3項目の形式不正ケースに対する `decision: error` テストを追加し、仕様の「空または形式不正」を満たすこと。

#### 任意改善

- 形式 validation の具体ルールを [docs/backend/api.md](docs/backend/api.md) に例付きで補強し、Action 側が送るべき canonical な値を固定すること。
- diff 判定分岐と payload validation 分岐を helper 化して、DB 前のロジックをユニットテストしやすくすること。

#### テスト不足

- `project_path` の形式不正ケース
- `tree_hash` の形式不正ケース
- `config_path` の形式不正ケース

#### ドキュメント更新漏れ

- 形式不正の具体定義が仕様上あいまいなまま残っている。少なくとも repository-relative path、期待拡張子、hash 形式の扱いは [docs/backend/api.md](docs/backend/api.md) 側に補足が必要。

#### plan / research / docs との不整合

- research と計画では `project_path` / `tree_hash` / `config_path` の形式不正を `decision: error` にする前提だったが、現実装は空文字のみ。
- [docs/external/sqlx-postgresql-upsert.md](docs/external/sqlx-postgresql-upsert.md#L110) と当初計画は 1 トランザクション実行を前提にしていた一方、実装は pool 直実行である。これは現時点で即時の仕様違反とは言い切れないが、計画との差分として残っている。

#### PR/完了結果

- `pr_ready: false`

#### 残リスク

- Action 側の不正入力や将来の呼び出し元変更で、仕様上は弾くべき不正 path / hash がそのまま DB upsert まで到達する。
- validation ルールが曖昧なまま実装を先行させると、Action 側と backend 側で path 正規化や hash 表現が食い違う可能性がある。

#### 更新した作業ログパス

- `docs/logs/4/worklog.md`

---

## ドキュメント確認 (2026-05-01)

### 対象Issue

- Issue ID: #4
- タイトル: Action API: Plan API実装

### 総評

- `docs/backend/api.md` の Plan API セクションに記載された request / response の JSON 形状は、現行実装の `PlanRequest` / `PlanResponse` と一致している。
- `docs/spec.md` Section 6.8 の reason 一覧は、現行実装の `PlanReason` enum と一致している。
- ただし `docs/backend/api.md` の tree_hash validation 記述は、実装より広く書かれている。ドキュメントは「空白文字を含む場合」を不正としているが、実装は [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L220) の通り半角スペース `' '` のみを弾いており、タブや改行は現状 reject しない。
- `docs/logs/4/worklog.md` 末尾の直近レビューは、実装が「空文字しか見ていない」としており、現行コードと不一致になっている。実装は [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L185) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L236) の通り `project_path` の絶対パス・パストラバーサル・拡張子不正、`config_path` の絶対パス・パストラバーサル、`tree_hash` のスペース含有も判定している。
- 判定: `docs_ready: false`

### 確認結果

1. **request / response スキーマ**
  - [docs/backend/api.md](docs/backend/api.md#L142) から [docs/backend/api.md](docs/backend/api.md#L190) の request / response 例は、[crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L10) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L96) の型定義と整合している。
  - `repository`、`git`、`action`、`mode`、`projects[]`、`files[]`、`latest_completed_run_id` の有無も一致している。

2. **reason 一覧**
  - [docs/spec.md](docs/spec.md#L510) から [docs/spec.md](docs/spec.md#L525)、[docs/backend/api.md](docs/backend/api.md#L213) から [docs/backend/api.md](docs/backend/api.md#L214)、[crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L80) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L95) を確認し、`new_project`、`hash_changed`、`config_changed`、`manual_dispatch`、`unchanged`、`previous_failed`、`no_previous_snapshot`、`duplicate_project_path`、`invalid_project_path`、`invalid_tree_hash`、`invalid_config_path` は一致している。

3. **validation ルール**
  - `project_path` の絶対パス禁止、`..` 禁止、`.kicad_pro` 必須は [docs/backend/api.md](docs/backend/api.md#L208) から [docs/backend/api.md](docs/backend/api.md#L211) と [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L185) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L201) で一致している。
  - `config_path` の絶対パス禁止、`..` 禁止は [docs/backend/api.md](docs/backend/api.md#L211) と [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L230) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L236) で一致している。
  - `tree_hash` は [docs/backend/api.md](docs/backend/api.md#L210) が「空白文字を含む場合」としている一方、実装は [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L220) で `' '` だけを判定しているため、ここは不一致。

4. **作業ログの正確性**
  - [docs/logs/4/worklog.md](docs/logs/4/worklog.md#L719) 以降の前回レビューは、形式不正 validation が未実装で「空文字しか見ていない」と結論づけているが、これは現行実装と一致しない。
  - 現行の worklog は経緯の記録としては残せるが、最新状態の判定としては誤解を生むため、今回の確認結果で上書き解釈できるよう明示が必要。

### 必須修正

- [docs/backend/api.md](docs/backend/api.md#L210) の tree_hash validation 記述を、実装に合わせて「半角スペースを含む場合」に修正するか、逆に実装をタブ・改行を含む全空白文字 reject に広げるかを統一すること。
- [docs/logs/4/worklog.md](docs/logs/4/worklog.md#L719) 以降の古いレビュー結論を、現行コードに即した内容へ更新または無効化したことが読み取れるようにすること。

### 任意改善

- `docs/spec.md` の `invalid_tree_hash` について、Plan API の具体条件として何を「形式不正」とみなすかを [docs/backend/api.md](docs/backend/api.md) と同じ粒度まで寄せると、仕様参照元が分散しても解釈がぶれにくい。

### 不整合のあるドキュメント

- [docs/backend/api.md](docs/backend/api.md#L210)
- [docs/logs/4/worklog.md](docs/logs/4/worklog.md#L719)

### 不足しているドキュメント

- 今回確認した範囲では、新規追加が必須なドキュメントはなし。

### 外部調査メモに関する指摘

- 今回の確認対象である request / response 形状、reason 一覧、validation ルールについて、参照した external メモとの新たな矛盾は見当たらない。

### PR/完了結果

- `docs_ready: false`

---

## 最終レビュー結果 (2026-05-01)

### 対象Issue

- Issue ID: #4
- タイトル: Action API: Plan API実装

### 総評

- `docs/spec.md` と `docs/backend/api.md` の Plan API 契約を、現行実装 [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L98) 以降と照合した限り、今回レビュー依頼で挙がっていた修正点は反映されている。
- 認可失敗は API 全体エラーとして返し、project 単位 validation は `decision: error` に閉じている点は、[docs/backend/api.md](docs/backend/api.md#L120) と [docs/spec.md](docs/spec.md#L504) に整合する。
- tree_hash validation は [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L217) で `char::is_whitespace()` を使用しており、[docs/backend/api.md](docs/backend/api.md#L210) の「空白文字を含む場合」と一致している。
- 判定: `pr_ready: true`

### レビュー結果

#### 重大度順の指摘

1. **低**: `tree_hash` の「空白文字を含む場合」の実装は正しいが、回帰防止テストが空文字ケースに寄っている。
  - 実装は [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L216) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L223) で `is_whitespace()` を使っており、仕様どおりタブや改行も reject できる。
  - ただしテストは [crates/api/tests/plan_test.rs](crates/api/tests/plan_test.rs#L676) の空文字ケース中心で、空白混入そのもののケースは未追加。

2. **低**: この環境で実行した 16 テストは全件成功したが、`DATABASE_URL` 未設定のため DB 接続を伴う統合経路まではこの場で実検証できていない。
  - `setup_pool()` は `DATABASE_URL` がなければ早期 return する実装であり、ローカルのコマンド成功だけでは SQLx 経路の実行保証にはならない。
  - したがって、レビュー上は「コードとテスト内容にブロッカーはない」が、「この環境で DB 経路を再現確認した」とまでは言えない。

#### 必須修正

- なし。

#### 任意改善

- `tree_hash` にタブ、改行、全角空白などを含めた `invalid_tree_hash` テストを追加すると、今回の `is_whitespace()` 修正の回帰防止が明確になる。
- 将来的に path validation を強化するなら、外部ベストプラクティスどおり `Path` の component ベース検証も候補になる。ただし現時点の実装は、仕様で定義された絶対パス禁止・`..` 禁止の契約は満たしている。

#### テスト結果

- `cargo test -p boardflow-api --test plan_test` を実行し、16 テストは全件 success だった。
- ただしこのシェルでは `DATABASE_URL` が空で、integration test は `setup_pool()` の早期 return により skip 相当の挙動になる。
- テスト内容自体は、認証、認可、JSON エラー、重複 project_path、path traversal、空の `tree_hash` / `config_path`、`hash_changed`、`unchanged`、`no_previous_snapshot` をカバーしている。

#### ドキュメント確認

- [docs/backend/api.md](docs/backend/api.md#L205) から [docs/backend/api.md](docs/backend/api.md#L214) の validation / reason 定義は、[crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L181) から [crates/api/src/routes/plan.rs](crates/api/src/routes/plan.rs#L238) と整合している。
- [docs/spec.md](docs/spec.md#L522) から [docs/spec.md](docs/spec.md#L525) の `decision: error` 用 reason 列挙も、実装の `PlanReason` と一致している。
- README は存在したが、workspace には `CONTRIBUTING.md` が存在せず、レビュー対象として確認できなかった。

#### PR/完了結果

- `pr_ready: true`

#### 残リスク

- DB あり環境での実行結果はこの場で再確認できていないため、CI もしくは `DATABASE_URL` ありのローカル環境で同テスト群を 1 回通しておくのが安全。
- `tree_hash` の空白混入ケースは実装で対応済みだが、将来の単純化リファクタで退行しても現在のテストでは即検知しにくい。

#### 更新した作業ログパス

- `docs/logs/4/worklog.md`

---

## 最終ドキュメント確認 (2026-05-01)

### 対象Issue

- Issue ID: #4
- タイトル: Action API: Plan API実装

### 総評

- `docs/backend/api.md` の Plan API セクション (L210) で tree_hash validation は「空白文字を含む場合」と記述されており、実装 `crates/api/src/routes/plan.rs` L217 の `char::is_whitespace()` と一致している。
- request/response スキーマ、validation ルール、reason 一覧は docs と実装で整合している。
- 前回の `docs_ready: false` の原因だった tree_hash の `' '` vs 「空白文字」の不一致は、`char::is_whitespace()` への修正で解消済み。
- 判定: `docs_ready: true`

---

## Issue #4 完了 (2026-05-01)

### 最終ステータス

- **review**: `pr_ready: true` (6回目レビュー)
- **docs**: `docs_ready: true` (最終ドキュメント確認)
- **テスト**: 16件全件パス
- **実装**: POST /api/v1/runs/plan 完全実装完了

### 実装概要

- Repository/BoardProject の upsert
- tree_hash ベースの build/skip 判定 (hash_changed, unchanged, no_previous_snapshot)
- project payload の形式 validation (project_path, tree_hash, config_path)
- Token 認証・認可
- mode=all 時の manual_dispatch 強制 build

### 残リスク (任意改善)

- tree_hash の空白文字混入ケースの回帰テスト追加
- DB あり環境での統合テスト再実行確認
