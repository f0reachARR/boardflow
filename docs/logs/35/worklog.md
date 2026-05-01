# Issue #35: Diff詳細Read API実装 - 作業ログ

## Issueまでの経緯

BoardFlowでは board_run ごとに diff（前回Runとの差分情報）を生成する仕組みがある。diff情報をフロントエンドから参照するためのRead APIが必要。

## ユーザー要望

- `GET /api/v1/board-runs/{board_run_id}/diff` エンドポイントを実装
- diff status、summary、metadata を一括で返す
- 既存の権限チェックパターン（board_run → board_project → repository → GitHub API）に従う

## 計画

1. `crates/db/src/queries/diff.rs` に SELECT クエリ追加
2. `crates/api/src/routes/read.rs` にレスポンス型とハンドラ追加
3. `crates/api/src/lib.rs` にルート登録
4. `crates/api/tests/read_api_test.rs` にテスト追加

## 実装内容

### 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `crates/db/src/queries/diff.rs` | `find_diff_by_board_run_id`, `find_diff_metadata_by_board_run_id` の2クエリ追加 |
| `crates/api/src/routes/read.rs` | `BoardRunDiffResponse`, `DiffMetadataResponse` 型追加、`get_board_run_diff` ハンドラ追加 |
| `crates/api/src/lib.rs` | `get_board_run_diff` ルート登録 |
| `crates/api/tests/read_api_test.rs` | 5テストケース追加 + ヘルパー関数 |

### 技術的判断

- 計画では `check_repo_access(token, github_repository_id)` とあったが、実際のコードベースでは `check_access(token, owner, name)` パターンを使用していたため後者に合わせた
- 計画では `queries::repository::find_repository_by_board_run_id` とあったが、実際は `queries::board_run::find_repository_by_board_run_id` に存在するためそちらを使用

## テスト結果

```
running 5 tests
test test_get_board_run_diff_ready ... ok
test test_get_board_run_diff_denied ... ok
test test_get_board_run_diff_no_baseline ... ok
test test_get_board_run_diff_invalid_id ... ok
test test_get_board_run_diff_not_found ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 42 filtered out
```

### テスト観点

| テスト名 | 観点 |
|---------|------|
| `test_get_board_run_diff_ready` | 正常系: diff status=ready、metadata付きでレスポンス構造を検証 |
| `test_get_board_run_diff_no_baseline` | 正常系: status=no_baseline、base_board_run_id=null、metadataなし |
| `test_get_board_run_diff_not_found` | diff未作成時に404が返ること |
| `test_get_board_run_diff_invalid_id` | 不正なIDフォーマットで400が返ること |
| `test_get_board_run_diff_denied` | アクセス権限なしで404が返ること（情報漏洩防止） |

## 残リスク

- なし。既存パターンに完全に従った実装。

## レビュー結果

### レビュー日時

- 2026-05-01

### 総評

- 実装本体は `board_run -> repository -> GitHub access check` の既存 Read API パターンに揃っており、404 による存在秘匿も維持されている。
- `board_run_diffs` / `board_run_diff_metadata` の返却内容も `docs/spec.md` の diff データモデルと整合している。
- 一方で、backend 契約ドキュメントへの反映がなく、追加した状態分岐に対するテストも最小限に留まっているため、このまま PR ready とは判定しない。

### 指摘事項

1. major: backend の API 契約ドキュメントに `GET /api/v1/board-runs/{board_run_id}/diff` が未記載。`docs/backend/api.md` は既存の board run Read API を列挙しているが、新エンドポイントは追加されていない。Issue の成果物が worklog とコードだけに閉じており、実装済み API と公開仕様がずれている。
2. minor: テストが `ready` / `no_baseline` / `not_found` / `invalid_id` / `denied` に偏っており、`failed` と `unavailable` の応答形状、`error_message` 返却、GitHub access checker の 429 / 500 分岐をこのエンドポイント単位では抑えていない。実装は単純だが、追加した enum 分岐と共有エラーハンドリングの回帰検知としては薄い。

### 良い点

- [crates/api/src/routes/read.rs](crates/api/src/routes/read.rs#L1353) のハンドラは [crates/api/src/routes/read.rs](crates/api/src/routes/read.rs#L804) や [crates/api/src/routes/read.rs](crates/api/src/routes/read.rs#L972) と同じ認証・認可パターンを踏襲している。
- [crates/db/src/queries/diff.rs](crates/db/src/queries/diff.rs#L43) のクエリ追加は責務が明確で、既存 insert/query 群の延長として自然。
- [crates/api/tests/read_api_test.rs](crates/api/tests/read_api_test.rs#L1801) 以降で正常系と存在秘匿の基本ケースを追加しており、最低限の契約は押さえている。

### テスト確認

- `mise exec -- cargo test -p boardflow-api --test read_api_test test_get_board_run_diff -- --nocapture`
- 実行結果: 5 件とも ok。ただしこの環境では `DATABASE_URL not set` のため各ケースは early return でスキップ可能な構成になっており、DB 接続ありの実動作まではこのレビューでは再検証できていない。

### PR / 完了結果

- pr_ready: false

### 必須修正

1. `docs/backend/api.md` もしくは同等の canonical API ドキュメントへ diff Read API を追加し、応答項目と 404 方針を明文化する。

### 任意改善

1. `failed` / `unavailable` の応答ケースを追加し、`error_message` と `summary` / `metadata` の有無を固定する。
2. rate limit / upstream error の共有分岐をこのエンドポイントでも 1 ケースずつ持たせ、`access_result_to_error()` との接続を回帰検知できるようにする。

### ドキュメント確認

- `docs/spec.md` の diff データモデルと UI 要件は確認済み。
- `docs/backend/api.md` には新エンドポイントの記載なし。
- `docs/backend/summary.md` にも diff Read API の記載なし。

### 残リスク

- 公開契約ドキュメント未更新のまま進むと、frontend / API 利用側が `viewer-sources` までは認識しても diff API の存在を追えない。
- `failed` / `unavailable` 系の回帰が入った場合、現行テストでは検知が遅れる可能性がある。

## レビュー結果（再レビュー）

### レビュー日時

- 2026-05-01

### 総評

- 前回の major 指摘だった [docs/backend/api.md](docs/backend/api.md#L761) の Diff 詳細セクション追加は解消済み。
- 前回の minor 指摘だった `failed` / `unavailable` テスト追加も解消済みで、[crates/api/tests/read_api_test.rs](crates/api/tests/read_api_test.rs#L1973) と [crates/api/tests/read_api_test.rs](crates/api/tests/read_api_test.rs#L2010) で対象ケースが追加されている。
- ただし、[docs/backend/api.md](docs/backend/api.md#L807) と [docs/backend/api.md](docs/backend/api.md#L809) は `summary` / `metadata` を `null` で返す契約として記述している一方、実装の [crates/api/src/routes/read.rs](crates/api/src/routes/read.rs#L322) から [crates/api/src/routes/read.rs](crates/api/src/routes/read.rs#L326) は `Option::is_none` で項目自体を省略する。契約と実装がまだ一致していないため、PR ready とは判定しない。

### 指摘事項

1. major: Diff API のレスポンス契約がドキュメントと実装で不一致。`docs/backend/api.md` は `no_baseline` 時の `summary: null` と metadata 未保存時の `metadata: null` を明記しているが、実装は [crates/api/src/routes/read.rs](crates/api/src/routes/read.rs#L322) から [crates/api/src/routes/read.rs](crates/api/src/routes/read.rs#L326) の `skip_serializing_if` により省略する。クライアントコード生成や strict JSON contract 前提の consumer では挙動差になりうる。
2. minor: 追加された `failed` / `unavailable` テストは状態分岐自体を抑えているが、[crates/api/tests/read_api_test.rs](crates/api/tests/read_api_test.rs#L2041) の `json["error_message"].is_null()` はフィールド欠落でも通るため、`null` を返す契約を固定していない。`metadata` も [crates/api/tests/read_api_test.rs](crates/api/tests/read_api_test.rs#L1877) で absent / null の両方を許容している。

### テスト結果

- `mise exec -- cargo test -p boardflow-api --test read_api_test test_get_board_run_diff -- --nocapture`
- 実行結果: 7 passed, 0 failed, 42 filtered out
- ただしこの環境では `DATABASE_URL not set` のため DB 依存ケースは skip 可能な構成で、HTTP レスポンスの完全な実動作までは再検証できていない。

### PR / 完了結果

- pr_ready: false

### 必須修正

1. Diff API で `null` を返す契約を維持するなら [crates/api/src/routes/read.rs](crates/api/src/routes/read.rs#L322) から [crates/api/src/routes/read.rs](crates/api/src/routes/read.rs#L326) の `skip_serializing_if` を外し、項目省略を意図するなら [docs/backend/api.md](docs/backend/api.md#L807) と [docs/backend/api.md](docs/backend/api.md#L809) を省略可能契約へ修正する。

### 任意改善

1. `unavailable` ケースでも `summary` / `metadata` / `error_message` の存在有無を `get()` ベースで明示的に検証し、API 契約をテストに固定する。

### ドキュメント確認

- [docs/spec.md](docs/spec.md#L561) の diff status 定義とは整合している。
- [docs/backend/api.md](docs/backend/api.md#L761) の Diff 詳細セクション追加は確認済み。
- ただし [docs/backend/api.md](docs/backend/api.md#L807) と [docs/backend/api.md](docs/backend/api.md#L809) の null 契約は実装と未整合。

### 残リスク

- frontend が `null` と項目欠落を区別する実装になった場合、契約解釈差で表示崩れやデシリアライズ失敗が起こりうる。
