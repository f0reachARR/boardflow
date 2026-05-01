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
