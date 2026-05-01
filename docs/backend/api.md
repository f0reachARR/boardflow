# Backend API 詳細仕様

このドキュメントは、BoardFlow backend の API 契約を OpenAPI 化する直前の粒度で整理する。
実際の canonical source は将来 `api/openapi.yaml` に置くが、MVP ではまず本書で Action、Web UI、worker 境界の判断を固定する。

## 1. 共通規約

### 1.1 基本形式

- API prefix は `/api/v1` とする。
- request / response body は JSON とする。
- 時刻は RFC3339 UTC の文字列に統一する。
- `Content-Type: application/json` を要求する。
- 金額やサイズなどの数値は JSON number とし、64bit 整数が必要な値は OpenAPI 化時に `int64` を指定する。

### 1.2 認証

Action API は repository 単位の BoardFlow API token で認証する。

```http
Authorization: Bearer <boardflow_api_token>
```

token は DB に hash のみ保存する。
認証に成功した場合のみ `last_used_at` を更新する。
revoke 済み token は認証エラー (`unauthorized`) として扱い、`last_used_at` は更新しない。

Web UI 向け read API は GitHub OAuth session を前提にする。
最終的な閲覧可否は backend が GitHub App installation と repository 権限に基づいて判定する。

### 1.3 ID

repository API / URL では `github_repository_id` を併用する。
BoardProject、BoardRun、Artifact、ArtifactBundle は BoardFlow 公開 ID を使う。

```text
board_project_id: bp_...
board_run_id:     br_...
artifact_id:      art_...
bundle_id:        ab_...
```

BoardProject の同一性は DB 上では `github_repository_id + project_path` で決まる。
ただし Web UI と read API の主キーは `board_project_id` とし、`project_path` は識別補助、表示、validation に使う。

### 1.4 エラー形式

エラーレスポンスは全 API で同じ形にする。

```json
{
  "error": {
    "code": "validation_failed",
    "message": "project_path is invalid",
    "details": {
      "field": "projects[0].project_path",
      "reason": "path_must_be_repository_relative"
    },
    "request_id": "req_abc123"
  }
}
```

MVP で固定する `error.code` は以下に限定する。
token revoke、repository mismatch、terminal run などの詳細理由は `details.reason` で表す。

| code | 主な HTTP status | 用途 |
|---|---:|---|
| `unauthorized` | 401 | token / session がない、または認証できない |
| `forbidden` | 403 | installation 解除、権限不足、repository 不一致 |
| `validation_failed` | 400 | request body、path parameter、query の形式不正 |
| `not_found` | 404 | 指定された repository / board / run / artifact が存在しない、または閲覧不可 |
| `conflict` | 409 | 同一 run への異なる bundle import など状態競合 |
| `gone` | 410 | terminal run への再 import など、過去状態として扱う操作 |
| `rate_limited` | 429 | repository / installation 単位の rate limit |
| `internal_error` | 500 | 想定外の server error |

`request_id` は structured log / trace と対応させる。
Action は `error.code` で大きな分岐を行い、ユーザー向け Job Summary では `message` と `details.reason` を表示してよい。

### 1.5 Cursor Pagination

一覧系 read API は cursor pagination を使う。

```http
GET /api/v1/repositories?limit=50&cursor=...
```

- `limit` の default は 50、最大は 100 とする。
- `cursor` は opaque string とし、frontend は中身を解釈しない。
- 並び順は endpoint ごとに固定し、同一 cursor で安定して次ページを取得できるようにする。

レスポンス形式:

```json
{
  "items": [],
  "next_cursor": "cur_next",
  "has_more": true
}
```

次ページがない場合、`next_cursor` は `null`、`has_more` は `false` とする。

### 1.6 Artifact URL

MVP では `viewer-sources` が返す URL は artifact proxy URL を標準にする。
S3 署名付き URL は内部実装候補に留め、公開 API のレスポンス形状には出さない。

```text
https://artifacts.boardflow.example.com/proxy/artifacts/art_abc123?token=...
```

URL は短命で、`expires_at` まで有効とする。
GitHub Issue には artifact proxy URL、署名URL、直接画像リンクを載せず、SaaS の認可付きページ URL のみを載せる。

## 2. Action API

Action API は BoardFlow API token による Bearer 認証を必須とする。
認証失敗（revoke 済み token を含む）、認可失敗（installation 解除、権限不足、repository 不一致）は API 全体のエラーであり、per-project `decision: error` にはしない。

### 2.1 Plan API

```http
POST /api/v1/runs/plan
```

Action が検出済み BoardProject 候補と tree hash を送信し、build 対象を問い合わせる。
Plan API は Repository / BoardProject の作成または取得、latest completed snapshot との比較、decision 返却までを行う。
Issue 作成ジョブは enqueue しない。

Request:

```json
{
  "repository": {
    "github_repository_id": "123456789",
    "owner": "ForteFibre",
    "name": "hardware"
  },
  "git": {
    "ref": "refs/heads/board/motor-driver-v2",
    "branch": "board/motor-driver-v2",
    "commit_sha": "abc123",
    "event_name": "push"
  },
  "action": {
    "workflow": "BoardFlow",
    "run_id": "987654321",
    "run_attempt": "1"
  },
  "mode": "auto",
  "projects": [
    {
      "project_path": "hardware/motor_driver/motor_driver.kicad_pro",
      "config_path": "hardware/motor_driver/.boardflow.yml",
      "project_dir": "hardware/motor_driver",
      "tree_hash": "sha256:...",
      "files": [
        {
          "path": "hardware/motor_driver/motor_driver.kicad_pcb",
          "sha256": "sha256:..."
        }
      ]
    }
  ]
}
```

Response:

```json
{
  "repository": {
    "github_repository_id": "123456789",
    "owner": "ForteFibre",
    "name": "hardware"
  },
  "projects": [
    {
      "project_path": "hardware/motor_driver/motor_driver.kicad_pro",
      "board_project_id": "bp_abc123",
      "decision": "build",
      "reason": "hash_changed"
    },
    {
      "project_path": "hardware/power_board/power_board.kicad_pro",
      "board_project_id": "bp_def456",
      "decision": "skip",
      "reason": "unchanged",
      "latest_completed_run_id": "br_prev123"
    }
  ]
}
```

`decision` は以下。

| decision | 意味 |
|---|---|
| `build` | Action は BoardRun を作成し、成果物生成へ進む |
| `skip` | 変更なしとして成果物生成しない |
| `error` | SaaS 側の project payload validation により project 単位で処理できない |

`decision: error` は、同一 request 内の `project_path` 重複、`project_path` / `tree_hash` / `config_path` の形式不正などに限定する。
Action 側で検出できる `.boardflow.yml` schema 不備、`.kicad_pro` 不在、必須 KiCad ファイル除外などは Plan API へ送らない。

形式不正の判定基準:
- `project_path`: 空文字、絶対パス（`/` 始まり）、パストラバーサル（`..` を含む）、`.kicad_pro` 拡張子でない場合
- `tree_hash`: 空文字、空白文字を含む場合
- `config_path`: 空文字、絶対パス（`/` 始まり）、パストラバーサル（`..` を含む）場合

`reason` は `new_project`、`hash_changed`、`config_changed`、`manual_dispatch`、`unchanged`、`previous_failed`、`no_previous_snapshot` を使う。
`decision: error` の場合の `reason` は `duplicate_project_path`、`invalid_project_path`、`invalid_tree_hash`、`invalid_config_path` を使う。
`mode: all` の場合、差分がなくても `decision: build`、`reason: manual_dispatch` としてよい。

### 2.2 BoardRun 作成 API

```http
POST /api/v1/board-runs
```

`decision: build` になった BoardProject について、KiCad 実行前に BoardRun を作成する。
BoardRun は成果物生成を試みた記録であり、DRC/ERC の成功失敗とは分ける。

Request:

```json
{
  "board_project_id": "bp_abc123",
  "project_path": "hardware/motor_driver/motor_driver.kicad_pro",
  "tree_hash": "sha256:...",
  "commit_sha": "abc123",
  "branch": "board/motor-driver-v2",
  "ref": "refs/heads/board/motor-driver-v2",
  "github_run_id": "987654321",
  "github_run_attempt": "1"
}
```

Response:

```json
{
  "board_run_id": "br_abc123",
  "status": "created",
  "artifact_bundle": {
    "upload_mode": "staging_s3",
    "object_key": "staging/runs/br_abc123/bundle.zip",
    "upload_url": "https://storage.example.com/...",
    "method": "PUT",
    "expires_at": "2030-01-01T12:00:00Z"
  }
}
```

作成直後の `board_runs.status` は `created` とする。
staging upload 用 URL を発行した run は `uploading` として扱ってよい。

冪等性:

- `board_project_id + github_run_id + github_run_attempt` を冪等キーにする。
- 同一 attempt の再送では新規 BoardRun を作らず、既存の `board_run_id` と状態を返す。
- 既存 status が `created` / `uploading` の場合は、有効な `artifact_bundle` を返してよい。
- 既存 status が `importing` の場合は、追加 upload を促さないため `artifact_bundle` を返さない。
- 既存 status が `completed` / `failed` / `timed_out` の場合は、terminal 状態を返し、Action は追加 build / upload / import をしない。

### 2.3 Artifact Bundle Import API

```http
POST /api/v1/board-runs/{board_run_id}/artifact-bundles/import
```

Action が staging bucket へ upload した zip bundle を backend に import させる。
API は import job の enqueue までを同期的に行い、zip 展開、manifest 検証、final bucket 保存、DB 保存は worker が行う。

Request:

```json
{
  "staging_object_key": "staging/runs/br_abc123/bundle.zip",
  "bundle_sha256": "sha256:...",
  "bundle_size_bytes": 12345678
}
```

Response:

```json
{
  "bundle_id": "ab_abc123",
  "status": "queued"
}
```

冪等性と競合:

- 同一 `board_run_id + staging_object_key + bundle_sha256` の再送は同じ `bundle_id` と現在の状態を返す。
- 同一 run に異なる `staging_object_key` または `bundle_sha256` が送られた場合は `409 conflict` とし、BoardRun の状態は変更しない。
- `completed` の run には新しい import job を作らず、既存 bundle 状態を返す。
- `failed` / `timed_out` の run への import は再 import 不可として `410 gone` を返す。
- import 受理時に BoardRun は `importing` へ遷移する。

`status` は `queued`、`running`、`completed`、`failed` を取る。
import worker が manifest / zip / artifact 検証に失敗した場合、BoardRun は `failed` になる。
DRC/ERC failed は import failure ではなく、manifest と check 結果を保存できる限り BoardRun は `completed` になる。

### 2.4 Fail API

```http
POST /api/v1/board-runs/{board_run_id}/fail
```

KiCad 実行、zip 作成、staging upload、Import API 要求前の失敗を BoardRun に記録する。
DRC/ERC の検査結果が failed だったことを通知する API ではない。

Request:

```json
{
  "status": "failed",
  "error": {
    "message": "kicad-cli export failed",
    "details": {
      "phase": "artifact_generation",
      "command": "kicad-cli pcb export svg"
    }
  }
}
```

Response:

```json
{
  "board_run_id": "br_abc123",
  "status": "failed",
  "failed_at": "2030-01-01T12:00:00Z"
}
```

冪等性:

- 同じ `board_run_id` に同じ失敗内容が再送された場合、既存の `failed` 状態を返す。
- 既存 BoardRun が `completed` の場合は `409 conflict` とし、`failed` へ戻さない。
- 既存 BoardRun が `timed_out` の場合は `timed_out` を維持し、`410 gone` を返す。

`fail-on-drc=true` または `fail-on-erc=true` の場合でも、Action は可能な成果物 bundle を upload し、Import API 要求を完了してから GitHub Actions job の終了コードだけを失敗にする。
この場合、SaaS 上の BoardRun は import 成功により `completed` になり得る。

## 3. Web UI Auth API

Web UI の認証は GitHub OAuth を使う。session は HTTP-only cookie で管理する。

### 3.0.1 Login

```http
GET /api/v1/auth/login
```

GitHub OAuth 認証画面へリダイレクトする。
server 側で CSRF 防御用の `state` (UUID v4) を生成し、`boardflow_oauth_state` cookie に保存する。

Response: `302 Found` → GitHub OAuth authorize URL

### 3.0.2 Callback

```http
GET /api/v1/auth/callback?code=...&state=...
```

GitHub OAuth コールバック。`state` を cookie と照合し、不一致時は `403 forbidden` を返す。
`code` を GitHub token endpoint で access token に交換し、user 情報を取得、DB upsert、session 作成を行う。

Response: `302 Found` → `/` (固定、open redirect 防止)

Cookie 設定:
- `boardflow_session`: session ID (HTTP-only, SameSite=Lax)
- `boardflow_oauth_state`: 削除

### 3.0.3 Logout

```http
POST /api/v1/auth/logout
```

session を削除し、cookie をクリアする。

Response:

```json
{ "ok": true }
```

### 3.0.4 Me

```http
GET /api/v1/auth/me
```

現在の session に紐づくユーザー情報を返す。

Response:

```json
{
  "github_user_id": "12345",
  "github_login": "username",
  "github_avatar_url": "https://avatars.githubusercontent.com/u/12345"
}
```

未認証の場合は `401 unauthorized` を返す。

## 3.0.5 Token Management API

Token 管理 API は GitHub OAuth session を前提にし、repository へのアクセス権を確認する。
トークンの作成、一覧取得、失効（revoke）の3つのエンドポイントを提供する。

### 3.0.5.1 Token 作成

```http
POST /api/v1/repositories/{github_repository_id}/api-tokens
```

認証: GitHub OAuth session (cookie)

Request:

```json
{
  "name": "CI token"
}
```

- `name`: 1〜100 文字（前後空白は trim）。空白のみは不可。

Response (201):

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "CI token",
  "token": "bft_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
  "created_at": "2026-05-01T00:00:00+00:00"
}
```

- `token`: 平文のトークン。`bft_` プレフィックス + 64 hex 文字。**この一回のみ表示**。DB には SHA-256 hash のみ保存。

エラーケース:

| status | code | 条件 |
|--------|------|------|
| 400 | `validation_failed` | request body が不正な JSON、name が空白のみ、name が 100 文字超 |
| 401 | `unauthorized` | session がない、または無効 |
| 404 | `not_found` | repository が存在しない、またはアクセス権がない（情報隠蔽） |

### 3.0.5.2 Token 一覧

```http
GET /api/v1/repositories/{github_repository_id}/api-tokens?limit=50&cursor=...
```

認証: GitHub OAuth session (cookie)

Query parameters:

| name | type | default | 説明 |
|------|------|---------|------|
| `limit` | integer | 50 | 1〜100 |
| `cursor` | string | - | opaque cursor（次ページ取得用） |

Response (200):

```json
{
  "items": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "name": "CI token",
      "created_at": "2026-05-01T00:00:00+00:00",
      "last_used_at": "2026-05-01T12:00:00+00:00",
      "revoked_at": null
    }
  ],
  "next_cursor": "eyJ0cyI6...",
  "has_more": true
}
```

- `token_hash` や平文は返さない。
- revoke 済み token も含めて返す（UI で状態表示に使う）。
- 並び順: `created_at DESC, id DESC`。

エラーケース:

| status | code | 条件 |
|--------|------|------|
| 400 | `validation_failed` | cursor の形式不正 |
| 401 | `unauthorized` | session がない、または無効 |
| 404 | `not_found` | repository が存在しない、またはアクセス権がない（情報隠蔽） |

### 3.0.5.3 Token 失効（Revoke）

```http
POST /api/v1/repositories/{github_repository_id}/api-tokens/{token_id}/revoke
```

認証: GitHub OAuth session (cookie)

Request body: なし

Response (200):

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "CI token",
  "created_at": "2026-05-01T00:00:00+00:00",
  "last_used_at": "2026-05-01T12:00:00+00:00",
  "revoked_at": "2026-05-01T13:00:00+00:00"
}
```

- 冪等: 既に revoke 済みの場合は既存の `revoked_at` を保持して返す。
- token が指定 repository に属さない場合は 404 を返す。

エラーケース:

| status | code | 条件 |
|--------|------|------|
| 400 | `validation_failed` | `token_id` の形式が不正（UUID でない） |
| 401 | `unauthorized` | session がない、または無効 |
| 404 | `not_found` | repository が存在しない、アクセス権がない、または token が存在しない/別 repository に属する |

## 3.1 Web UI Read API

Web UI read API は GitHub OAuth session を前提にし、backend が repository 権限を確認する。
存在しない resource と閲覧不可 resource は、情報漏洩を避けるためどちらも `404 not_found` を返してよい。

### 3.1 Repository 一覧

```http
GET /api/v1/repositories?limit=50&cursor=...
```

Response:

```json
{
  "items": [
    {
      "github_repository_id": "123456789",
      "owner": "ForteFibre",
      "name": "hardware",
      "installation_id": "98765",
      "board_project_count": 2,
      "latest_run_status": "completed",
      "updated_at": "2030-01-01T12:00:00Z"
    }
  ],
  "next_cursor": null,
  "has_more": false
}
```

並び順は `updated_at desc, github_repository_id desc` とする。

### 3.2 Repository 詳細

```http
GET /api/v1/repositories/{github_repository_id}
```

Response:

```json
{
  "github_repository_id": "123456789",
  "owner": "ForteFibre",
  "name": "hardware",
  "installation_id": "98765",
  "html_url": "https://github.com/ForteFibre/hardware",
  "board_project_count": 2,
  "created_at": "2030-01-01T12:00:00Z",
  "updated_at": "2030-01-01T12:00:00Z"
}
```

### 3.3 BoardProject 一覧

```http
GET /api/v1/repositories/{github_repository_id}/board-projects?limit=50&cursor=...
```

Response:

```json
{
  "items": [
    {
      "board_project_id": "bp_abc123",
      "project_path": "hardware/motor_driver/motor_driver.kicad_pro",
      "project_dir": "hardware/motor_driver",
      "display_name": "motor_driver",
      "state": "completed",
      "latest_completed_run_id": "br_abc123",
      "latest_tree_hash": "sha256:...",
      "issue_url": "https://github.com/ForteFibre/hardware/issues/12",
      "updated_at": "2030-01-01T12:00:00Z"
    }
  ],
  "next_cursor": null,
  "has_more": false
}
```

`state` は UI 用の集約状態で、少なくとも `detected`、`processing`、`failed`、`timed_out`、`completed` を返す。
初回 completed run 前の BoardProject も一覧に出す。

### 3.4 BoardProject 詳細

```http
GET /api/v1/board-projects/{board_project_id}
```

Response:

```json
{
  "board_project_id": "bp_abc123",
  "repository": {
    "github_repository_id": "123456789",
    "owner": "ForteFibre",
    "name": "hardware"
  },
  "project_path": "hardware/motor_driver/motor_driver.kicad_pro",
  "project_dir": "hardware/motor_driver",
  "display_name": "motor_driver",
  "state": "completed",
  "latest_completed_run_id": "br_abc123",
  "latest_tree_hash": "sha256:...",
  "issue_number": 12,
  "issue_url": "https://github.com/ForteFibre/hardware/issues/12",
  "recreate_issue_on_update": true,
  "created_at": "2030-01-01T12:00:00Z",
  "updated_at": "2030-01-01T12:00:00Z"
}
```

### 3.5 BoardRun 一覧

```http
GET /api/v1/board-projects/{board_project_id}/board-runs?limit=50&cursor=...
```

Response:

```json
{
  "items": [
    {
      "board_run_id": "br_abc123",
      "status": "completed",
      "commit_sha": "abc123",
      "branch": "board/motor-driver-v2",
      "ref": "refs/heads/board/motor-driver-v2",
      "github_run_id": "987654321",
      "github_run_attempt": "1",
      "tree_hash": "sha256:...",
      "erc_status": "passed",
      "erc_errors": 0,
      "erc_warnings": 2,
      "drc_status": "failed",
      "drc_errors": 1,
      "drc_warnings": 4,
      "created_at": "2030-01-01T12:00:00Z",
      "completed_at": "2030-01-01T12:05:00Z"
    }
  ],
  "next_cursor": null,
  "has_more": false
}
```

並び順は `created_at desc, board_run_id desc` とする。
`status: completed` は artifact import 成功を表し、DRC/ERC 成功を意味しない。

### 3.6 BoardRun 詳細

```http
GET /api/v1/board-runs/{board_run_id}
```

Response:

```json
{
  "board_run_id": "br_abc123",
  "board_project_id": "bp_abc123",
  "status": "completed",
  "commit_sha": "abc123",
  "branch": "board/motor-driver-v2",
  "ref": "refs/heads/board/motor-driver-v2",
  "github_run_id": "987654321",
  "github_run_attempt": "1",
  "tree_hash": "sha256:...",
  "checks": [
    {
      "kind": "erc",
      "status": "passed",
      "error_count": 0,
      "warning_count": 2,
      "notice_count": 0
    },
    {
      "kind": "drc",
      "status": "failed",
      "error_count": 1,
      "warning_count": 4,
      "notice_count": 0
    }
  ],
  "artifact_summary": {
    "available": 8,
    "missing": 1,
    "failed": 0,
    "skipped": 1
  },
  "created_at": "2030-01-01T12:00:00Z",
  "completed_at": "2030-01-01T12:05:00Z"
}
```

### 3.7 Artifact 一覧

```http
GET /api/v1/board-runs/{board_run_id}/artifacts
```

Response:

```json
{
  "items": [
    {
      "artifact_id": "art_schematic_pdf",
      "type": "schematic_pdf",
      "status": "available",
      "filename": "motor_driver-schematic.pdf",
      "content_type": "application/pdf",
      "sha256": "sha256:...",
      "size_bytes": 123456,
      "source_path": null,
      "logical_name": null,
      "created_at": "2030-01-01T12:05:00Z"
    },
    {
      "type": "drill_zip",
      "status": "missing",
      "status_reason": "not generated"
    }
  ]
}
```

`available` の artifact のみ `artifact_id`、`content_type`、`sha256`、`size_bytes` を必須とする。
`missing` / `failed` / `skipped` は実体ファイルを持たないが、期待 artifact の状態表示のため行として返す。

### 3.8 Viewer Sources API

```http
GET /api/v1/board-runs/{board_run_id}/viewer-sources
```

Run 内の成果物 preview / download 導線を viewer 用途ごとに返す。
KiCanvas 専用 API は作らず、KiCanvas は viewer の一種として扱う。

Response:

```json
{
  "board_run_id": "br_abc123",
  "expires_at": "2030-01-01T12:10:00Z",
  "viewers": {
    "kicanvas": {
      "status": "available",
      "sources": [
        {
          "artifact_id": "art_project",
          "kind": "project",
          "name": "motor_driver.kicad_pro",
          "source_path": "hardware/motor_driver/motor_driver.kicad_pro",
          "url": "https://artifacts.boardflow.example.com/proxy/artifacts/art_project?token=..."
        },
        {
          "artifact_id": "art_schematic",
          "kind": "schematic",
          "name": "motor_driver.kicad_sch",
          "source_path": "hardware/motor_driver/motor_driver.kicad_sch",
          "url": "https://artifacts.boardflow.example.com/proxy/artifacts/art_schematic?token=..."
        },
        {
          "artifact_id": "art_board",
          "kind": "board",
          "name": "motor_driver.kicad_pcb",
          "source_path": "hardware/motor_driver/motor_driver.kicad_pcb",
          "url": "https://artifacts.boardflow.example.com/proxy/artifacts/art_board?token=..."
        }
      ]
    },
    "schematic": {
      "status": "available",
      "primary": {
        "artifact_id": "art_schematic_pdf",
        "artifact_type": "schematic_pdf",
        "url": "https://artifacts.boardflow.example.com/proxy/artifacts/art_schematic_pdf?token=..."
      }
    },
    "pcb_preview": {
      "status": "available",
      "sources": [
        {
          "artifact_id": "art_top",
          "artifact_type": "pcb_top_svg",
          "url": "https://artifacts.boardflow.example.com/proxy/artifacts/art_top?token=..."
        },
        {
          "artifact_id": "art_bottom",
          "artifact_type": "pcb_bottom_svg",
          "url": "https://artifacts.boardflow.example.com/proxy/artifacts/art_bottom?token=..."
        }
      ]
    },
    "ibom": {
      "status": "available",
      "iframe_url": "https://artifacts.boardflow.example.com/proxy/artifacts/art_ibom?token=..."
    },
    "bom": {
      "status": "available",
      "downloads": [
        {
          "artifact_id": "art_bom",
          "artifact_type": "bom_csv",
          "url": "https://artifacts.boardflow.example.com/proxy/artifacts/art_bom?token=..."
        }
      ]
    },
    "fabrication": {
      "status": "partial",
      "downloads": [
        {
          "artifact_id": "art_gerber",
          "artifact_type": "gerber_zip",
          "status": "available",
          "url": "https://artifacts.boardflow.example.com/proxy/artifacts/art_gerber?token=..."
        },
        {
          "artifact_type": "drill_zip",
          "status": "missing",
          "status_reason": "not generated"
        }
      ]
    }
  }
}
```

viewer 単位の `status` は以下。

| status | 意味 |
|---|---|
| `available` | viewer に必要な主要 source が揃っている |
| `partial` | 一部 source だけ利用でき、限定表示または一部 download が可能 |
| `missing` | 期待 source が存在せず表示できない |
| `failed` | source artifact または URL 生成で失敗して表示できない |
| `skipped` | project 構成や設定上、この viewer を提供しない |

`kicanvas` が `missing` / `failed` / `skipped` の場合でも、`schematic` や `pcb_preview` が `available` なら静的 fallback を提供する。
KiCad source artifact は private design data として扱い、GitHub Issue には viewer-sources の URL を載せない。

### 3.9 Diff 詳細

```http
GET /api/v1/board-runs/{board_run_id}/diff
```

BoardRun に紐づく差分情報を返す。差分レビュー画面で使用される。
base_run は同一 BoardProject の直近 completed BoardRun として import worker が決定する。

Response:

```json
{
  "board_run_id": "br_abc123",
  "base_board_run_id": "br_prev123",
  "status": "ready",
  "summary": {
    "file_changes": { "added": 1, "removed": 0, "changed": 3, "unchanged": 10 },
    "bom_changes": { "added": 2, "removed": 1, "changed": 1 },
    "checks": {
      "erc": { "status_change": "passed -> passed", "error_delta": 0, "warning_delta": -1 },
      "drc": { "status_change": "failed -> passed", "error_delta": -1, "warning_delta": -2 }
    },
    "artifacts": { "added": 0, "removed": 0, "changed": 1 }
  },
  "metadata": {
    "file_hashes": { ... },
    "bom_summary": { ... },
    "checks_summary": { ... },
    "artifacts_summary": { ... },
    "previews": { ... }
  },
  "error_message": null,
  "created_at": "2030-01-01T12:05:00Z"
}
```

`status` は以下。

| status | 意味 |
|---|---|
| `ready` | 差分サマリを作成できた |
| `no_baseline` | 初回 run などで比較元がない |
| `unavailable` | 比較元または現在 run の必要データが不足している |
| `failed` | 差分作成処理自体が失敗した |

`status` が `no_baseline` の場合、`base_board_run_id` は `null`、`summary` は `null` とする。
`status` が `failed` の場合、`error_message` に失敗理由を含めてよい。
`metadata` は import worker が diff_metadata を保存した場合のみ返す。未保存時は `null`。

diff レコードが存在しない BoardRun（import 中、または diff 作成前）に対しては `404 not_found` を返す。

### 3.10 Findings 一覧

```http
GET /api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings?limit=50&cursor=...&severity=error
```

BoardRun に紐づく ERC/DRC チェック結果（findings）一覧を返す。Checks 画面で finding 一覧を表示するために使用される。

パスパラメータ:
- `board_run_id`: `br_<uuid>` 形式の BoardRun ID
- `check_kind`: `erc` | `drc`

クエリパラメータ:
- `limit`: 1〜100 (デフォルト 50)
- `cursor`: opaque cursor string (base64url encoded)
- `severity`: `error` | `warning` | `notice` (省略時は全件)

認可: `board_run_id` → `find_repository_by_board_run_id` → `access_checker.check_access`

Response:

```json
{
  "items": [
    {
      "id": "uuid-string",
      "severity": "error",
      "rule_code": "power_pin_not_driven",
      "title": "Input Power pin not driven by any Output Power pins",
      "message": "Symbol #PWR019 Pin 1",
      "subject_kind": "schematic",
      "subject_ref": "#PWR019",
      "sheet_path": "/",
      "pcb_layer": null,
      "pos_mm": { "x": 5.715, "y": 2.667 }
    }
  ],
  "next_cursor": "...",
  "has_more": true
}
```

注意事項:
- `pos_mm` は DB の `x_um`, `y_um` (整数 µm) を mm に変換して返す (`x_um / 1000.0`)。座標が未設定の場合は `null`。
- `raw_payload_json` と `bbox_json` は帯域節約のためレスポンスから除外される。
- 指定 `board_run_id` + `check_kind` の `run_check` が存在しない場合は空リストを返す（404 ではない）。
- 並び順: `sort_index ASC, id ASC`
- cursor: `(sort_index, id)` ペアをエンコードした opaque 文字列

エラーレスポンス:
| ステータス | 条件 |
|---|---|
| 400 | 不正な `board_run_id` format / 不正な `check_kind` / 不正な `severity` / 不正な cursor |
| 401 | 未認証 / セッション期限切れ |
| 404 | `board_run` が存在しない / アクセス拒否 |

## 4. Artifact Proxy API

artifact proxy は `viewer-sources` が返した短命 URL から利用される。
通常のアプリ画面から直接組み立てて呼ばない。

```http
GET /proxy/artifacts/{artifact_id}?token=...
```

要件:

- token は短命(1時間)で、artifact_id、user_id、expiry を含む HMAC 署名済みトークン。viewer-sources API が認証済みユーザーにのみ発行する。proxy 側では token の署名検証と expiry チェックのみ行い、追加の session 検証は不要（bearer token 設計）。
- `Content-Type` は import 済み artifact metadata から設定する。
- `X-Content-Type-Options: nosniff` を付与する。
- iframe 用 artifact には制限付き `Content-Security-Policy` と sandbox 前提の配信ヘッダを付与する。
- 許可 origin は app domain に限定する（`Access-Control-Allow-Origin` ヘッダで制御）。

設定:

- `BOARDFLOW_ARTIFACT_BASE_URL`: viewer-sources が返す artifact proxy URL のベース URL。本番例: `https://artifacts.boardflow.example.com`。未設定時のデフォルト: `http://localhost:8080`。
- `BOARDFLOW_APP_DOMAIN`: CORS と frame-ancestors で許可する app ドメイン。本番例: `https://app.boardflow.example.com`。未設定時のデフォルト: `http://localhost:3000`。
- `BOARDFLOW_ARTIFACT_SECRET`: HMAC 署名に使用するシークレット。必須。

## 5. 契約テスト観点

> **注記: `run_check_findings` read API について**
>
> `run_check_findings` テーブルの read API（一覧取得）は Issue #36 で実装済み（セクション 3.10 参照）。
> 個別 finding の詳細取得 API は今後の Issue で追加予定。

MVP では以下を API contract test として優先する。

- Bearer token 認証に成功し、revoke 済み token は `401 unauthorized` になる。
- token に紐づく repository と request repository が不一致の場合、Plan API は API 全体で `403 forbidden` を返す。
- Plan API は `build`、`skip`、project 単位の `error` を返せる。
- Plan API の認可失敗は per-project `decision: error` にならない。
- BoardRun 作成 API は同一 `board_project_id + github_run_id + github_run_attempt` の再送で既存 run を返す。
- BoardRun 作成 API は terminal 状態の既存 run に upload URL を再発行しない。
- Import API は同一 bundle 再送で同じ `bundle_id` を返す。
- Import API は同一 run への異なる bundle を `409 conflict` で拒否する。
- `failed` / `timed_out` の run への import は `410 gone` になる。
- Fail API は `completed` の run を `failed` に戻さない。
- cursor pagination は `limit`、`cursor`、`next_cursor`、`has_more` を返す。
- viewer-sources は `available`、`partial`、`missing`、`failed`、`skipped` を viewer 単位で返せる。
- artifact proxy URL は `expires_at` 以降に使えず、private artifact URL が GitHub Issue コメント本文に入らない。
