# Issue #36: run_check_findings Read API 実装

## Issueまでの経緯

- Worker の Import Job (Issue #7 追加実装) で `run_check_findings` テーブルへの INSERT が実装済み
- `docs/backend/api.md` セクション5 に「`run_check_findings` read API は今後の Issue で追加予定」と明記されている
- BoardRun 詳細 API (3.6) は集計値のみ返しており、finding 明細の取得経路は未提供
- Checks 画面で finding 一覧を表示するために Read API が必要

## ユーザー要望

board_run の DRC/ERC チェック結果（findings）一覧を返す Read API を実装する。

## 調査結果 (2026-05-01)

### 1. 認可パターン（board_run_id → リポジトリアクセスチェック）

既存の `board_run_id` ベースのエンドポイント（`get_board_run`, `list_artifacts`, `get_viewer_sources`, `get_board_run_diff`）は全て同じパターン:

```
1. parse_board_run_id() で br_ prefix を検証・UUID 変換
2. board_run::find_repository_by_board_run_id() でリポジトリ情報を取得
3. access_checker.check_access() で GitHub OAuth session に基づくアクセス権確認
4. access_result_to_error() でDenied/Errorをnot_foundに変換
```

findings API も同じパターンを踏襲すべき。

### 2. Cursor Pagination 実装パターン

既存の一覧 API (`list_repositories`, `list_board_projects`, `list_board_runs`) で共通のパターン:

- `PaginationParams` struct: `limit` (default 50, max 100), `cursor` (opaque string)
- `PaginatedResponse<T>`: `items`, `next_cursor`, `has_more`
- Cursor: `encode_cursor(ts, id)` / `decode_cursor()` で `(DateTime<Utc>, Uuid)` ペアをbase64エンコード
- クエリで `limit + 1` 件取得し、`has_more` を判定
- 最後のアイテムの `(created_at, id)` を次の cursor として返す

**注意**: findings の場合、`sort_index` での並び順が自然。cursor は `(sort_index, id)` ペアが適切。ただし既存パターンは `(DateTime<Utc>, Uuid)` なので、findings 用に `(i32, Uuid)` の cursor を新設するか、`created_at` + `id` で代用するか判断が必要。

→ `sort_index ASC, id ASC` で並べるのが UI に最適。cursor は `(sort_index, id)` ペアで実装推奨。

### 3. run_check_findings テーブルスキーマ

```sql
CREATE TABLE run_check_findings (
    id UUID PRIMARY KEY,
    run_check_id UUID NOT NULL REFERENCES run_checks(id),
    severity TEXT NOT NULL CHECK (severity IN ('error', 'warning', 'notice')),
    rule_code TEXT,
    title TEXT,
    message TEXT,
    subject_kind TEXT CHECK (subject_kind IN ('schematic', 'pcb', 'net', 'footprint', 'symbol')),
    subject_ref TEXT,
    sheet_path TEXT,
    pcb_layer TEXT,
    x_um INTEGER,
    y_um INTEGER,
    bbox_json JSONB,
    raw_payload_json JSONB,
    sort_index INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_run_check_findings_run_check_id ON run_check_findings(run_check_id);
```

- `run_check_id` → `run_checks.id` (FK)
- `run_checks.board_run_id` → `board_runs.id` (FK)
- 認可チェーンは `board_run_id` → `board_project` → `repository`

### 4. RunCheckFinding domain model

ファイル: `crates/domain/src/models/run_check.rs`

```rust
pub struct RunCheckFinding {
    pub id: Uuid,
    pub run_check_id: Uuid,
    pub severity: FindingSeverity,     // enum: Error, Warning, Notice
    pub rule_code: Option<String>,
    pub title: Option<String>,
    pub message: Option<String>,
    pub subject_kind: Option<SubjectKind>,  // enum: Schematic, Pcb, Net, Footprint, Symbol
    pub subject_ref: Option<String>,
    pub sheet_path: Option<String>,
    pub pcb_layer: Option<String>,
    pub x_um: Option<i32>,
    pub y_um: Option<i32>,
    pub bbox_json: Option<serde_json::Value>,
    pub raw_payload_json: Option<serde_json::Value>,
    pub sort_index: i32,
    pub created_at: DateTime<Utc>,
}
```

`sqlx::FromRow` 実装済み。

### 5. 既存 DB query パターン（list系）

`crates/db/src/queries/run_check_finding.rs` には現在 `insert` のみ存在。`list_by_run_check_id` のような読み取りクエリは未実装。

他のクエリの参考:
- `run_check::list_by_board_run`: `SELECT * FROM run_checks WHERE board_run_id = $1 ORDER BY check_kind, created_at`
- `artifact::list_by_board_run`: ページネーションなしの全件取得

findings は件数が多い可能性があるため、cursor pagination が必要。

### 6. API 仕様における findings endpoint の期待される URL 構造

`docs/backend/api.md` には findings endpoint の具体的なURL/レスポンス仕様はまだ定義されていない（「今後の Issue で追加予定」と注記のみ）。

既存のリソース URL 設計から推測される URL:

```
GET /api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings
```

- `check_kind` は `erc` or `drc`
- board_run 配下に checks (erc/drc) → findings とネストする構造が自然
- 代替案: `GET /api/v1/board-runs/{board_run_id}/findings?check_kind=erc` (フラット)

### 7. 実装に必要なコンポーネント

| コンポーネント | ファイル | 作業内容 |
|---|---|---|
| DB query | `crates/db/src/queries/run_check_finding.rs` | `list_by_run_check_id` (paginated) 追加 |
| DB query | `crates/db/src/queries/run_check.rs` | `find_by_board_run_and_kind` 追加 (board_run_id + check_kind で run_check 特定) |
| API handler | `crates/api/src/routes/read.rs` | `list_findings` handler 追加 |
| API response | `crates/api/src/routes/read.rs` | `FindingListItem` response struct 追加 |
| Cursor | `crates/api/src/routes/read.rs` | `(sort_index, id)` cursor encode/decode 追加 |
| Router | `crates/api/src/lib.rs` | `.routes(routes!(routes::read::list_findings))` 追加 |

### 8. docs/external/kicad-erc-drc-findings.md の充足性

既存ドキュメントは以下をカバー:
- KiCad JSON フォーマット (ERC/DRC)
- manifest.json → run_check_findings テーブルへのマッピング
- severity の値と BoardFlow マッピング
- violation type 一覧 (ERC/DRC)
- Worker INSERT 実装パターン

**Read API 実装に追加の外部調査は不要。** テーブルスキーマ、domain model、既存パターンが全て揃っている。

## 計画 (2026-05-01)

### 目的

board_run の DRC/ERC チェック結果（findings）一覧を返す Read API を実装し、Checks 画面で finding 一覧を表示可能にする。

### 非目的

- finding 個別詳細取得 API (GET /findings/{id}) — 本 Issue では一覧のみ
- finding の更新・削除 API
- フロントエンド UI 実装

### 受け入れ条件

1. `GET /api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings` が正しい JSON レスポンスを返す
2. cursor pagination (`limit`, `cursor`, `next_cursor`, `has_more`) が動作する
3. `severity` クエリパラメータによるフィルタリングが動作する
4. 認可チェック (board_run_id → repository → GitHub access) が既存パターンと同様に機能する
5. `raw_payload_json` と `bbox_json` はレスポンスから除外される
6. 不正な `check_kind` に対して 400 が返る
7. 存在しない board_run_id に対して 404 が返る
8. 未認証リクエストに対して 401 が返る

### 詳細要件

#### エンドポイント設計

```
GET /api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings
```

**パスパラメータ:**
- `board_run_id`: `br_<uuid>` 形式の BoardRun ID
- `check_kind`: `erc` | `drc`

**クエリパラメータ:**
- `limit`: 1〜100 (デフォルト 50)
- `cursor`: opaque cursor string
- `severity`: `error` | `warning` | `notice` (省略時は全件)

**レスポンス (200):**
```json
{
  "items": [
    {
      "id": "uuid",
      "severity": "error",
      "rule_code": "ERC001",
      "title": "Unconnected pin",
      "message": "Pin A1 is not connected",
      "subject_kind": "schematic",
      "subject_ref": "U1",
      "sheet_path": "/root/sub",
      "pcb_layer": null,
      "x_um": 12500,
      "y_um": 34000,
      "sort_index": 0,
      "created_at": "2026-05-01T00:00:00Z"
    }
  ],
  "next_cursor": "...",
  "has_more": false
}
```

注: `raw_payload_json` と `bbox_json` はレスポンスから除外。

**エラーレスポンス:**
- 400: 不正な board_run_id format / 不正な check_kind / 不正な severity / 不正な cursor
- 401: 未認証 / セッション期限切れ
- 404: board_run が存在しない / アクセス拒否

**仕様判断:**
- 指定 board_run_id + check_kind の run_check が存在しない場合 → 空リストを返す (404 ではない)
- run_check が `skipped` の場合 → 空の findings リストを返す

### 影響範囲

- `crates/db/src/queries/run_check_finding.rs` — list クエリ追加
- `crates/db/src/queries/run_check.rs` — `find_by_board_run_and_kind` クエリ追加
- `crates/domain/src/models/run_check.rs` — `RunCheckFindingListRow` 構造体追加
- `crates/api/src/routes/read.rs` — ハンドラ、レスポンス型、cursor ヘルパー追加
- `crates/api/src/lib.rs` — ルート登録
- `crates/api/tests/read_api_test.rs` — テスト追加
- `docs/backend/api.md` — API ドキュメント更新

### 設計方針

1. **認可**: 既存 `board_run_id` パターンをそのまま踏襲 (`find_repository_by_board_run_id` → `check_access`)
2. **Cursor**: findings は `sort_index ASC, id ASC` で並べるため、`(i32, Uuid)` の新しい cursor 形式を導入。既存の `(DateTime, Uuid)` cursor とは独立した `encode_finding_cursor` / `decode_finding_cursor` ヘルパーを追加。cursor payload は `FindingCursorPayload { si: String, id: String }` で JSON → base64url encode。
3. **フィルタ**: `severity` パラメータは DB クエリの WHERE 条件に追加。未指定時は全 severity を返す。
4. **レスポンスサイズ抑制**: `raw_payload_json` と `bbox_json` は一覧レスポンスから除外。DB SELECT で必要カラムのみ取得。
5. **check_kind バリデーション**: パスパラメータの `check_kind` は `erc` / `drc` のみ許可。それ以外は 400。
6. **run_check 不在時**: 指定 board_run_id + check_kind の run_check が存在しない場合、空リストを返す (404 ではない)。

### テスト観点

1. **正常系**: findings 一覧取得成功 (複数件)
2. **Pagination**: limit=2 で has_more=true、cursor で次ページ取得
3. **Severity filter**: severity=error で error のみ取得
4. **空リスト**: findings なしで空リスト返却
5. **認証系**: セッションなしで 401
6. **認可系**: アクセス拒否で 404
7. **バリデーション**: 不正 board_run_id で 400、不正 check_kind で 400
8. **存在しない board_run**: 404

### ドキュメント更新対象

- `docs/backend/api.md` — セクション 3 にエンドポイント仕様追加、セクション 5 の注記を「実装済み」に更新

### 実装要否

`implementation_required`

### 未解決の疑問

なし — 全て既存パターンから判断可能。

---

## 変更ファイル一覧と各ファイルの変更内容

### 1. `crates/domain/src/models/run_check.rs`

**追加**: `RunCheckFindingListRow` — list API レスポンス用 (`bbox_json`, `raw_payload_json` 除外)

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunCheckFindingListRow {
    pub id: Uuid,
    pub run_check_id: Uuid,
    pub severity: FindingSeverity,
    pub rule_code: Option<String>,
    pub title: Option<String>,
    pub message: Option<String>,
    pub subject_kind: Option<SubjectKind>,
    pub subject_ref: Option<String>,
    pub sheet_path: Option<String>,
    pub pcb_layer: Option<String>,
    pub x_um: Option<i32>,
    pub y_um: Option<i32>,
    pub sort_index: i32,
    pub created_at: DateTime<Utc>,
}
```

### 2. `crates/db/src/queries/run_check.rs`

**追加**: `find_by_board_run_and_kind`

```rust
pub async fn find_by_board_run_and_kind(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    board_run_id: Uuid,
    check_kind: &str,
) -> Result<Option<RunCheck>, sqlx::Error> {
    sqlx::query_as::<_, RunCheck>(
        "SELECT * FROM run_checks WHERE board_run_id = $1 AND check_kind = $2"
    )
    .bind(board_run_id)
    .bind(check_kind)
    .fetch_optional(executor)
    .await
}
```

### 3. `crates/db/src/queries/run_check_finding.rs`

**追加**: `list_by_run_check_id` — cursor pagination + severity filter

```rust
pub async fn list_by_run_check_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    run_check_id: Uuid,
    limit: i64,
    cursor: Option<(i32, Uuid)>,
    severity: Option<&str>,
) -> Result<Vec<RunCheckFindingListRow>, sqlx::Error>
```

SQL:
```sql
SELECT id, run_check_id, severity, rule_code, title, message,
       subject_kind, subject_ref, sheet_path, pcb_layer, x_um, y_um,
       sort_index, created_at
FROM run_check_findings
WHERE run_check_id = $1
  AND ($4::text IS NULL OR severity = $4)
  AND (($2::int IS NULL) OR (sort_index > $2) OR (sort_index = $2 AND id > $3))
ORDER BY sort_index ASC, id ASC
LIMIT $5
```

### 4. `crates/api/src/routes/read.rs`

**追加**:
- `FindingCursorPayload` struct
- `encode_finding_cursor(sort_index: i32, id: Uuid) -> String`
- `decode_finding_cursor(cursor: &str) -> Option<(i32, Uuid)>`
- `FindingsPaginationParams` struct (`limit`, `cursor`, `severity`)
- `FindingListItem` response struct (ToSchema)
- `list_findings` handler

### 5. `crates/api/src/lib.rs`

**変更**: `.routes(routes!(routes::read::list_findings))` を追加

### 6. `crates/api/tests/read_api_test.rs`

**追加**: findings API テストケース群 (8項目)

### 7. `docs/backend/api.md`

**追加**: `3.8 Findings 一覧 GET /api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings`
**更新**: セクション 5 の注記を「実装済み」に変更

---

## 実装順序

1. `crates/domain/src/models/run_check.rs` — `RunCheckFindingListRow` 追加
2. `crates/db/src/queries/run_check.rs` — `find_by_board_run_and_kind` 追加
3. `crates/db/src/queries/run_check_finding.rs` — `list_by_run_check_id` 追加
4. `crates/api/src/routes/read.rs` — cursor ヘルパー、パラメータ型、レスポンス型、ハンドラ追加
5. `crates/api/src/lib.rs` — ルート登録
6. `crates/api/tests/read_api_test.rs` — テスト追加
7. `docs/backend/api.md` — ドキュメント更新
8. コンパイル確認・テスト実行

---

## 残リスク

- findings が大量（数千件）の場合のパフォーマンス → `idx_run_check_findings_run_check_id` インデックスが存在し、limit 制約もあるため MVP では問題なし
- `sort_index` cursor の一意性 → 同一 `sort_index` の場合は `id` で tie-break するため問題なし

## 参照URL

- 既存 read.rs パターン: `crates/api/src/routes/read.rs`
- run_check_findings スキーマ: `crates/db/migrations/20260430000001_create_schema.up.sql` L112-130
- RunCheckFinding domain model: `crates/domain/src/models/run_check.rs` L54-78
- DB insert query: `crates/db/src/queries/run_check_finding.rs`
- API 仕様: `docs/backend/api.md` L838-841
- findings フォーマット調査: `docs/external/kicad-erc-drc-findings.md`

---

## 実装結果 (2026-05-01)

### 実装内容

Issue #36 の findings read API を TDD で実装完了。

#### 変更ファイル

| ファイル | 変更内容 |
|---|---|
| `crates/domain/src/models/run_check.rs` | `RunCheckFindingListRow` struct 追加 |
| `crates/db/src/queries/run_check.rs` | `find_by_board_run_and_kind` クエリ追加 |
| `crates/db/src/queries/run_check_finding.rs` | `list_by_run_check_id` paginated クエリ追加 |
| `crates/api/src/routes/read.rs` | `list_findings` handler、`FindingsQueryParams`、`FindingListItem`、`CoordinateMmResponse`、findings cursor helpers 追加 |
| `crates/api/src/lib.rs` | `.routes(routes!(routes::read::list_findings))` 追加 |
| `crates/api/tests/read_api_test.rs` | 11 テスト追加 |
| `docs/backend/api.md` | セクション 3.10 追加、セクション 5 注記更新 |

#### 実装詳細

- **エンドポイント**: `GET /api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings`
- **Cursor pagination**: `(sort_index, id)` ペアで base64url エンコードした opaque cursor
- **Severity filter**: クエリパラメータ `?severity=error|warning|notice`
- **pos_mm 変換**: `x_um / 1000.0`, `y_um / 1000.0` で µm → mm
- **帯域節約**: `raw_payload_json` と `bbox_json` は DB クエリレベルで除外
- **認可**: 既存パターン踏襲 (`find_repository_by_board_run_id` → `check_access`)
- **run_check 不在時**: 空リスト返却 (404 ではない)

### テスト結果

全 11 テスト合格:
1. `test_list_findings_success` — 正常系: findings取得、pos_mm変換確認
2. `test_list_findings_empty` — 空リスト (run_checkはあるがfindingsなし)
3. `test_list_findings_no_run_check_returns_empty` — run_check不在時の空リスト
4. `test_list_findings_severity_filter` — severity=error で1件のみ取得
5. `test_list_findings_pagination` — limit=2 で has_more=true、cursor で次ページ
6. `test_list_findings_invalid_check_kind` — 不正check_kindで400
7. `test_list_findings_invalid_board_run_id` — 不正board_run_idで400
8. `test_list_findings_invalid_severity` — 不正severityで400
9. `test_list_findings_unauthenticated` — 未認証で401
10. `test_list_findings_access_denied` — アクセス拒否で404
11. `test_list_findings_board_run_not_found` — 存在しないboard_runで404

全ワークスペーステスト: 133 passed, 0 failed

### 更新ドキュメント

- `docs/backend/api.md`: セクション 3.10 (Findings 一覧) 追加、セクション 5 注記を「実装済み」に更新

### 残リスク

- なし。既存パターンに完全準拠しており、全テスト合格。
