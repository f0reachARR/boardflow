# GitHub APIレートリミットとDBキャッシュ設計

## 要約

BoardFlow Issue #63 に関連し、GitHub REST API のレートリミット仕様、429/403 レスポンスのハンドリングベストプラクティス、PostgreSQL でのキャッシュテーブル設計、sqlx での JSONB upsert パターンをまとめる。

## 確認した情報

### 1. GitHub API Rate Limiting 仕様

#### Primary Rate Limit

| 認証方式 | 制限 |
|---|---|
| 未認証 | 60 req/h（IPベース） |
| ユーザーアクセストークン（OAuth App / GitHub App） | **5,000 req/h**（ユーザー単位で共有） |
| GitHub Enterprise Cloud ユーザートークン | 15,000 req/h |
| GitHub App インストールトークン | 最低 5,000 req/h（リポジトリ数・ユーザー数に応じてスケール、上限 12,500 req/h） |

- **ユーザーアクセストークンのレートリミットは、同じユーザーに紐づく全てのアプリ・PATで共有される**
- BoardFlow は OAuth App 経由の user access token を使用 → **5,000 req/h per user** が上限
- `list_accessible_repo_ids` は `/user/repos?per_page=100` を全ページ取得するため、リポジトリが 500 件あると 5 API コール消費

#### Secondary Rate Limit

- GET リクエスト: 1ポイント/req
- 同時並行リクエスト数の制限（具体的な数値は非公開）
- **予告なく変更される可能性あり**
- 違反すると一時的にブロック（最小1分待機）

#### レスポンスヘッダ

| ヘッダ | 説明 |
|---|---|
| `x-ratelimit-limit` | 1時間あたりの最大リクエスト数 |
| `x-ratelimit-remaining` | 現在のウィンドウ内の残りリクエスト数 |
| `x-ratelimit-used` | 使用済みリクエスト数 |
| `x-ratelimit-reset` | ウィンドウリセット時刻（UTC epoch秒） |
| `retry-after` | リトライまでの秒数（secondary rate limit時のみ、常に返されるとは限らない） |

### 2. 429/403 レスポンスハンドリング

GitHub は primary rate limit 超過時に **403 または 429** を返す。判定方法:

1. **`retry-after` ヘッダがあれば**: 指定秒数だけ待ってリトライ
2. **`x-ratelimit-remaining` が 0 であれば**: `x-ratelimit-reset` の時刻まで待つ
3. **上記いずれもない場合（secondary rate limit）**: **最低1分待機**、失敗が続く場合は指数バックオフ

**重要**: GitHub は secondary rate limit で必ずしも `retry-after` ヘッダを返さない（GitHub公式ドキュメントおよび GitHub Issue #1805 で確認）。

#### BoardFlow の現状

`RealGithubAccessChecker` は既に `403` + `x-ratelimit-remaining=0` と `retry-after` ヘッダの組み合わせで `RateLimited` を判定している。**429 ステータスコードも判定済み**。ただし:
- リトライロジックは未実装（即座に `AccessError::RateLimited` を返す）
- `x-ratelimit-remaining` / `x-ratelimit-reset` の値を保存していない

### 3. PostgreSQL キャッシュテーブル設計

#### UNLOGGED テーブル vs 通常テーブル

| 特性 | UNLOGGED | 通常 |
|---|---|---|
| WALなし → 高速書き込み | ✅ | ❌ |
| クラッシュ復旧後にデータ消失 | ✅（テーブルは空になる） | ❌ |
| レプリケーション不可 | ✅ | ❌ |
| キャッシュ用途 | 適している | 安全だが遅め |

**BoardFlow への推奨**: キャッシュデータはクラッシュ後に再取得すれば良いため **UNLOGGED テーブルが適切**。ただし、将来の Read Replica 構成を考慮するなら通常テーブルも選択肢。現状のシングルインスタンス構成では UNLOGGED で十分。

#### テーブル設計案

```sql
CREATE TABLE github_api_cache (
    cache_key   TEXT PRIMARY KEY,      -- 例: "user_repos:{github_user_id}"
    value_json  JSONB NOT NULL,        -- キャッシュデータ本体
    expires_at  TIMESTAMPTZ NOT NULL,  -- TTL expiry
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_github_api_cache_expires_at ON github_api_cache (expires_at);
```

代替案: ユーザー単位でキャッシュキーを設計する場合

```sql
CREATE TABLE github_api_cache (
    user_id     UUID NOT NULL REFERENCES users(id),
    cache_type  TEXT NOT NULL,           -- 例: "accessible_repo_ids"
    value_json  JSONB NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, cache_type)
);

CREATE INDEX idx_github_api_cache_expires_at ON github_api_cache (expires_at);
```

#### TTL と invalidation 戦略

- **TTL**: 5〜15分が妥当（権限変更の即時反映は不要だが、極端に古いデータは避けたい）
- **明示的 invalidation**: Webhook（`installation_repositories` イベント）受信時にキャッシュ削除
- **期限切れ行の掃除**: `DELETE FROM github_api_cache WHERE expires_at < NOW()` を定期実行（ジョブキューまたは起動時）

### 4. sqlx での JSONB + Upsert パターン

BoardFlow は既に以下のパターンを使用中:
- `serde_json::Value` を JSONB カラムに bind する
- `ON CONFLICT ... DO UPDATE ... RETURNING` パターン（user.rs, repository.rs, github_job.rs）

キャッシュ upsert の例:

```rust
pub async fn upsert_cache(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    user_id: Uuid,
    cache_type: &str,
    value: &serde_json::Value,
    expires_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO github_api_cache (user_id, cache_type, value_json, expires_at, created_at, updated_at)
           VALUES ($1, $2, $3, $4, NOW(), NOW())
           ON CONFLICT (user_id, cache_type) DO UPDATE SET
             value_json = EXCLUDED.value_json,
             expires_at = EXCLUDED.expires_at,
             updated_at = NOW()"#,
    )
    .bind(user_id)
    .bind(cache_type)
    .bind(value)
    .bind(expires_at)
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn get_cache(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    user_id: Uuid,
    cache_type: &str,
) -> Result<Option<serde_json::Value>, sqlx::Error> {
    let row = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT value_json FROM github_api_cache WHERE user_id = $1 AND cache_type = $2 AND expires_at > NOW()"
    )
    .bind(user_id)
    .bind(cache_type)
    .fetch_optional(executor)
    .await?;
    Ok(row)
}
```

sqlx 0.8 は `serde_json::Value` を PostgreSQL JSONB に直接 bind できる（postgres feature に含まれる）。追加の feature flag は不要。

## BoardFlow への示唆

### 推奨アーキテクチャ: CachedGithubAccessChecker (Decorator パターン)

```
GithubAccessChecker trait
  ├── RealGithubAccessChecker          (直接 GitHub API 呼び出し)
  ├── CachedGithubAccessChecker        (DB キャッシュ + fallback to inner)
  │     └── inner: RealGithubAccessChecker
  ├── AllowAllGithubAccessChecker      (テスト用)
  └── ...
```

- `CachedGithubAccessChecker` は `GithubAccessChecker` を実装し、内部に `RealGithubAccessChecker` + `PgPool` を持つ
- `list_accessible_repo_ids` でまず DB キャッシュを確認、有効なら返す
- キャッシュミスまたは期限切れ → GitHub API 呼び出し → 結果を DB に upsert
- **429/RateLimited 時のフォールバック**: API がレートリミットエラーを返した場合、期限切れキャッシュがあればそれを返す（stale-while-error パターン）
- `check_access` は個別リポジトリの権限チェックなのでキャッシュ不要（リスト取得に比べて低頻度）

### TTL 推奨値

| キャッシュ対象 | TTL |
|---|---|
| `list_accessible_repo_ids` | 5〜10分 |
| stale-while-error フォールバック | 期限切れ後も最大1時間保持 |

### Invalidation

- 明示的: `installation_repositories` Webhook で対象ユーザーのキャッシュを DELETE
- 暗黙的: TTL 超過で自動失効
- 手動: 管理 API やキャッシュクリアエンドポイント（将来的に）

## 採用/不採用判断

| 項目 | 判断 |
|---|---|
| DB キャッシュ（PostgreSQL） | **採用** — 既存インフラで追加サービス不要 |
| UNLOGGED テーブル | **不採用** — キャッシュの persistence は不要だが、将来のレプリカ構成対応と sqlx マイグレーション管理の簡便さを考慮し通常テーブルで問題ない |
| Redis/Memcached | **不採用** — オーバーキル、運用負荷増 |
| Decorator パターン (CachedGithubAccessChecker) | **採用** — トレイト設計が既にこれを想定 |
| stale-while-error | **採用** — レートリミット時の耐障害性向上 |
| インメモリキャッシュ併用 | **不採用（現時点）** — マルチインスタンス不整合リスク、DB キャッシュで十分な性能 |

## 制約と pitfall

1. **ユーザートークンのレートリミットは共有**: BoardFlow 以外のアプリ・PAT も同じ 5,000 req/h バジェットを消費する → キャッシュは必須
2. **UNLOGGED テーブルはレプリケーション不可**: 将来 Read Replica を導入する場合、通常テーブルに変更する必要あり
3. **secondary rate limit は `retry-after` を返さない場合がある**: 固定の 1分待機 + 指数バックオフの実装が必要
4. **キャッシュの陳腐化**: ユーザーが GitHub 側でリポジトリ権限を変更しても、TTL が切れるまで古い情報が返る
5. **`list_accessible_repo_ids` の全ページ取得**: リポジトリ数が多いユーザーでは複数ページの API コールが発生 → キャッシュの恩恵が大きい
6. **github_user_id vs user_id**: キャッシュキーは BoardFlow の `user_id` (UUID) を使うべき（users テーブルとの FK 整合性）

## 未解決の疑問

1. `check_access` （個別リポジトリの権限チェック）もキャッシュすべきか？ → 現時点では不要（低頻度）だが、将来的に検討
2. `installation_repositories` Webhook の受信とキャッシュ invalidation の実装タイミング → 別 Issue で対応可能
3. キャッシュの TTL 値は設定ファイルで外部化すべきか → 初回実装はハードコードで十分、後で Config crate に移動

## 参照URL

- [GitHub REST API Rate Limits](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api)
- [Rate limits for OAuth Apps](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/rate-limits-for-oauth-apps)
- [Troubleshooting REST API - Rate limit errors](https://docs.github.com/en/rest/using-the-rest-api/troubleshooting-the-rest-api#rate-limit-errors)
- [GitHub Issue: Retry-After header not always sent (#1805)](https://github.com/hub4j/github-api/issues/1805)
- [PostgreSQL as a Cache (Martin Heinz)](https://martinheinz.dev/blog/105)
- [Storing sessions/cache in PostgreSQL](https://tech-couch.com/post/storing-sessions-or-cache-data-in-postgresql)
- [sqlx::types::Json docs](https://docs.rs/sqlx/latest/sqlx/types/struct.Json.html)
- [SQLx PostgreSQL Upsert パターン（BoardFlow既存調査メモ）](../external/sqlx-postgresql-upsert.md)
