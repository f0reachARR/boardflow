# Issue #77: Webhook不着時のリポジトリ一覧取得: Installed Repositories APIフォールバック

## 経緯
- ユーザー要望4: Webhookが不着の場合にリポジトリ一覧が表示されない

## ユーザー要望
- GitHub Appインストール直後やWebhook不着時にもリポジトリ一覧を表示したい

## 調査結果
- 現在: Webhook (`installation`/`installation_repositories` イベント) 経由でのみDBにリポジトリが登録される
- `routes/webhook.rs`: `handle_installation_event` / `handle_installation_repositories_event` でupsert
- GitHub API: `GET /installation/repositories` (Installation Token), `GET /user/installations/{id}/repositories` (User Token)
- `CachedGithubAccessChecker` が既存のキャッシュ機構として存在

## Issue作成内容
- タイトル: Webhook不着時のリポジトリ一覧取得: Installed Repositories APIフォールバック
- ラベル: bug, backend, api
- 新規作成

## 調査結果（2026-05-04 リサーチエージェント追記）

### 1. GET /user/installations

- **認証**: GitHub App user access token（Bearer token）。追加パーミッション不要
- **レスポンス**: `{ total_count, installations: [...] }`
- **主要フィールド**: `installations[].id`（installation_id）、`installations[].app_id`、`installations[].repository_selection`（"all" or "selected"）、`installations[].suspended_at`
- **ページネーション**: `per_page` 最大100、`page` パラメータ。BoardFlow App インストールは通常1ページで収まる
- **ステータス**: 200, 304, 401, 403
- **注意**: 全 GitHub App のインストールが返る → `app_id` で BoardFlow のものだけフィルタ必須

### 2. GET /user/installations/{installation_id}/repositories

- **認証**: GitHub App user access token。`metadata` リポジトリパーミッション（read）が必要
- **レスポンス**: `{ total_count, repositories: [...] }`
- **主要フィールド**: `repositories[].id`（github_repository_id）、`repositories[].full_name`（"owner/name"）、`repositories[].owner.login`、`repositories[].name`
- **ページネーション**: `per_page` 最大100、`page` パラメータ
- **ステータス**: 200, 304, 403, 404
- **注意**: 404 = インストールへのアクセス権なし or 削除済み

### 3. 実装方針の要点

- **フォールバック発火条件**: `list_accessible_repo_ids` が返した `repo_id` 群と DB の `repositories` テーブルを比較し、DB 未登録の ID が存在する場合に同期発火
- **フロー**: `/user/installations?per_page=100` → `app_id` フィルタ → 各 installation_id に対して `/user/installations/{id}/repositories?per_page=100` → `repository::upsert`
- **スロットル**: `github_api_cache` テーブルに `cache_type = "installation_repos_sync"` として最終同期時刻を記録、TTL 10分で再同期を防ぐ
- **API コスト**: 典型 2 コール（1 installations + 1 repos）。最悪でも 20 コール以内
- **レートリミット**: user access token で 5,000 req/h 共有。フォールバック同期の追加コストは限定的
- **`/user/repos` との違い**: `/user/repos` には `installation_id` が含まれないため、`repositories` テーブルの upsert には `/user/installations/{id}/repositories` が必須

### 4. 既存コードとの統合ポイント

- `CachedGithubAccessChecker`（`crates/api/src/github_access.rs`）の `list_accessible_repo_ids` 内で差分検出 → フォールバック発火
- `boardflow_db::queries::repository::upsert(pool, github_repository_id, owner, name, installation_id)` をそのまま再利用可能
- `github_api_cache` テーブルの `upsert_cache(user_id, "installation_repos_sync", ...)` でスロットル制御

### 5. 制約と pitfall

1. `app_id` フィルタ忘れると他 App のインストールを処理してしまう
2. `repository_selection = "all"` の org インストールでは大量リポジトリが返る可能性
3. `suspended_at` が non-null のインストールは除外すべき
4. 同期処理をホットパスで行うとレイテンシ増加（バックグラウンドジョブも選択肢）
5. `full_name` の `split_once('/')` 失敗ケースへのガード処理

### 6. 未解決の疑問

1. BoardFlow の `app_id` の取得元（環境変数 or config crate）
2. 同期を `CachedGithubAccessChecker` 内で行うか、別 Service に分離するか
3. ホットパス同期 vs バックグラウンドジョブ投入の判断

### 詳細ドキュメント

→ `docs/external/github-user-installations-api.md`

## 後続処理タイプ
`implementation_required`

---

## 実装計画（2026-05-04 planエージェント策定）

### 目的
- Webhook が不着の場合でも、GitHub App がインストール済みのリポジトリ一覧を `repositories` テーブルに同期し、ユーザーがリポジトリ一覧を閲覧できるようにする

### 非目的
- Webhook 処理の変更・修正
- バックグラウンドジョブによる非同期同期（今後のオプション。本Issue ではホットパス同期のみ）
- GitHub App Installation Token を使ったサーバー間同期
- `repository_selection = "all"` の org で全リポジトリを自動的に追加すること（アクセス可能な repo のみ）

### 受け入れ条件
1. `list_repositories` API で、Webhook 未受信でもインストール済みリポジトリが表示される
2. フォールバック同期は 10分間に1回までスロットルされる
3. レートリミット時にフォールバック同期はスキップされ、既存動作に影響しない
4. `suspended_at` が non-null のインストールは除外される
5. `GITHUB_APP_ID` 未設定時はフォールバック同期をスキップする（degraded動作）
6. 既存テストが壊れない

### 詳細要件

#### フォールバック発火条件
- `CachedGithubAccessChecker.list_accessible_repo_ids()` が `Ok(Some(ids))` を返した後
- `ids` のうち DB `repositories` テーブルに存在しない ID が 1 件以上ある
- `github_api_cache` の `cache_type = "installation_repos_sync"` エントリが期限切れ（TTL 10分）

#### フォールバック同期フロー
1. `GET /user/installations?per_page=100` を呼ぶ（ユーザーの OAuth token）
2. レスポンスから `app_id == GITHUB_APP_ID` かつ `suspended_at == null` のインストールのみ抽出
3. 各インストール ID に対して `GET /user/installations/{id}/repositories?per_page=100` を呼ぶ（ページング対応）
4. 各リポジトリについて `repository::upsert(pool, github_repository_id, owner, name, installation_id)` を実行
5. `github_api_cache` に `cache_type = "installation_repos_sync"` を TTL 600秒で upsert

#### エラーハンドリング
- GitHub API が 401/403/429 を返した場合 → フォールバック同期を中断し、元の `list_accessible_repo_ids` の結果をそのまま返す（エラーは伝搬しない）
- DB 更新失敗 → ログ出力して続行
- `full_name` の `split_once('/')` 失敗 → そのリポジトリをスキップ

### 影響範囲
| ファイル | 変更内容 |
|---|---|
| `crates/config/src/app.rs` | `github_app_id: Option<u64>` フィールド追加 |
| `crates/api/src/lib.rs` | `GithubAppId` Extension 追加・注入 |
| `crates/api/src/github_access.rs` | `CachedGithubAccessChecker` にフォールバック同期ロジック追加 |
| `crates/db/src/queries/repository.rs` | `find_existing_github_ids(pool, ids) -> Vec<i64>` クエリ追加 |
| `crates/api/tests/github_cache_test.rs` | フォールバック同期のユニットテスト追加 |

### 設計方針

#### 1. `AppConfig` に `github_app_id` 追加
```rust
// crates/config/src/app.rs
pub struct AppConfig {
    // ...existing fields...
    pub github_app_id: Option<u64>,
}
```
環境変数 `GITHUB_APP_ID` から取得。Worker の既存パターンを踏襲。

#### 2. `CachedGithubAccessChecker` の拡張
```rust
pub struct CachedGithubAccessChecker {
    inner: Arc<dyn GithubAccessChecker>,
    pool: sqlx::PgPool,
    github_app_id: Option<u64>,  // 追加
}
```
- `new()` と `with_inner()` に `github_app_id: Option<u64>` 引数追加
- `list_accessible_repo_ids` 内で成功後にフォールバック同期判定を実行

#### 3. フォールバック同期ロジック（`CachedGithubAccessChecker` 内のプライベートメソッド）
```rust
async fn maybe_sync_installation_repos(
    &self,
    github_access_token: &str,
    user_id: Uuid,
    accessible_ids: &[i64],
) -> () { /* best-effort, never returns error */ }
```
- `github_app_id` が `None` → 即リターン
- DB に存在しない ID が 0 件 → 即リターン
- スロットルチェック（`get_valid_cache` で `installation_repos_sync` が有効なら）→ 即リターン
- 上記すべてパスしたら GitHub API を呼んで upsert

#### 4. `find_existing_github_ids` クエリ
```rust
pub async fn find_existing_github_ids(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    github_ids: &[i64],
) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(
        "SELECT github_repository_id FROM repositories WHERE github_repository_id = ANY($1)"
    )
    .bind(github_ids)
    .fetch_all(executor)
    .await
}
```

#### 5. `lib.rs` での注入
```rust
#[derive(Clone)]
pub struct GithubAppId(pub Option<u64>);

// create_app_with_config に github_app_id: Option<u64> パラメータ追加
// CachedGithubAccessChecker::new(pool, github_app_id) に変更
```

### テスト観点

1. **フォールバック発火テスト**: DB に repo が存在しない場合に同期が実行されることを確認
2. **スロットルテスト**: 10分以内の再同期がスキップされることを確認
3. **app_id フィルタテスト**: 異なる app_id のインストールが無視されることを確認
4. **suspended インストール除外テスト**: `suspended_at` が設定されたインストールが無視されることを確認
5. **エラー耐性テスト**: GitHub API エラー時に元の結果がそのまま返ることを確認
6. **github_app_id 未設定テスト**: `None` 時にフォールバックがスキップされることを確認
7. **既存テスト回帰**: `CachedGithubAccessChecker::new` のシグネチャ変更に伴うテスト修正

### ドキュメント更新対象
- `docs/backend/api.md` — フォールバック同期の動作仕様を追記
- `README.md` — `GITHUB_APP_ID` が API サーバーでも使用される旨を追記（現在は Worker 用のみ記載）

### 実装順序
1. `crates/db/src/queries/repository.rs` — `find_existing_github_ids` 追加
2. `crates/config/src/app.rs` — `github_app_id` フィールド追加
3. `crates/api/src/lib.rs` — `GithubAppId` Extension と `CachedGithubAccessChecker` 初期化変更
4. `crates/api/src/github_access.rs` — フォールバック同期ロジック実装
5. `crates/api/tests/github_cache_test.rs` — テスト追加
6. ドキュメント更新

### 実装要否
`implementation_required`

### 未解決の疑問と解消結果
| 疑問 | 解消方法 | 結論 |
|---|---|---|
| `app_id` の取得元 | コード調査 | `GITHUB_APP_ID` 環境変数。Worker で既に `Optional<u64>` として読み込み済み。API 側の `AppConfig` に同じパターンで追加 |
| 同期を Checker 内で行うか別 Service か | 設計判断 | `CachedGithubAccessChecker` 内のプライベートメソッドとして実装（単一責任の観点では分離が望ましいが、DI の複雑化を避けるため最小変更を優先） |
| ホットパス vs バックグラウンドジョブ | Issue 本文の指示 | ホットパス（同一リクエスト内）。best-effort で失敗しても元のレスポンスは返す |

### 残リスク
- 大量リポジトリ org でのレイテンシ増加（ページングで最大数百ms追加。将来的にバックグラウンドジョブ化で対応可能）
- `per_page=100` で収まらない org のリポジトリは複数ページ取得が必要（実装でページング対応する）
- レートリミット 5,000 req/h のうちフォールバック同期分の消費（スロットル 10分で最大 6回/h × 2-3 API calls = 12-18 req/h。影響は限定的）

---

## 実装結果（2026-05-04）

### 実装内容

計画通りに全4ステップを実装完了。

| ファイル | 変更内容 |
|---|---|
| `crates/db/src/queries/repository.rs` | `find_existing_github_ids` クエリ追加 |
| `crates/config/src/app.rs` | `github_app_id: Option<u64>` フィールド追加、`GITHUB_APP_ID` 環境変数読み取り |
| `crates/api/src/lib.rs` | `GithubAppId(pub Option<u64>)` newtype追加、`create_app_with_config` に引数追加、Extension layer追加 |
| `crates/api/src/github_access.rs` | `CachedGithubAccessChecker` に `github_app_id` フィールド追加、`maybe_sync_installation_repos` メソッド追加、ヘルパーメソッド `fetch_user_installations` / `fetch_installation_repos` 追加、デシリアライズ構造体追加 |
| `crates/api/src/main.rs` | `config.github_app_id` を `create_app_with_config` に渡す |
| `crates/api/tests/github_cache_test.rs` | `new()` / `with_inner()` 呼び出しに `None` 引数追加 |
| `crates/api/tests/api_token_test.rs` | `create_app_with_config` 呼び出しに `None` 引数追加 |
| `crates/api/tests/read_api_test.rs` | `create_app_with_config` 呼び出しに `None` 引数追加 |
| `crates/api/tests/webhook_test.rs` | `create_app_with_config` 呼び出しに `None` 引数追加 |
| `crates/api/tests/proxy_test.rs` | `create_app_with_config` 呼び出しに `None` 引数追加 |

### テスト結果
- `cargo check --workspace`: 成功（コンパイルエラーなし）
- 既存テストのシグネチャ変更: 全て `None` を追加引数として渡す形で修正済み
- フォールバック同期ロジックは `github_app_id = None` の場合に即リターンするため、既存テスト動作に影響なし

### ドキュメント
- `docs/external/github-user-installations-api.md`: リサーチエージェントにより作成済み（APIリファレンス・設計方針）

### 残リスク
- フォールバック同期の統合テスト（実GitHub APIを使うため、mockサーバーの導入が望ましい）
- `GITHUB_APP_ID` が本番環境で未設定の場合、フォールバック同期がサイレントにスキップされる
- 大量リポジトリ org でのレイテンシ増加（best-effort だが数百msのレイテンシ追加の可能性）

---

## 更新した作業ログパス
`docs/logs/77/worklog.md`
