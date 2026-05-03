# Issue #63: GitHub APIレスポンスのDBキャッシュとレートリミット対策

## Issueまでの経緯

- crates/api/src/routes/read.rs の list_repositories がリクエストごとにGitHub API呼び出し
- crates/api/src/github_access.rs の RealGithubAccessChecker が毎回APIコール
- 現在キャッシュ層は一切存在しない
- GitHub APIレートリミットは5000 req/h per user token
- 既存の関連Issueなし

## ユーザー要望

レポジトリ一覧APIなど一覧系APIで毎回GitHub APIを呼び出しているため、レートリミットに引っかかる可能性がある。適切なinvalidateの実装とともにDBにキャッシュして、GitHub APIの呼び出し回数を減らす必要がある。

## Issue作成内容

- Issue #63として新規作成
- labels: backend, api
- インメモリ/DBキャッシュ導入、TTL設定、invalidate戦略、429フォールバック

## 後続処理タイプの初期仮説

`implementation_required`

---

## 調査フェーズ（2026-05-03 research エージェント）

### 調査結果

#### 1. GitHub API Rate Limiting 仕様

- ユーザーアクセストークン: **5,000 req/h**（ユーザー単位、全アプリ共有）
- GitHub App インストールトークン: 5,000〜12,500 req/h（スケール）
- Secondary rate limit: GET 1ポイント/req、具体的なしきい値は非公開、予告なし変更あり
- レスポンスヘッダ: `x-ratelimit-remaining`, `x-ratelimit-reset`, `x-ratelimit-used`, `retry-after`

#### 2. 429/403 ハンドリング

- GitHub は primary rate limit で **403 または 429** を返す
- `retry-after` ヘッダがあれば指定秒数待機、なければ `x-ratelimit-reset` まで待機
- secondary rate limit では `retry-after` が返されないケースあり → 最低1分待機 + 指数バックオフ
- **BoardFlow の現状**: `RealGithubAccessChecker` は 403 + ヘッダで判定済み、429 も判定済み。ただしリトライロジックは未実装

#### 3. PostgreSQL キャッシュテーブル設計

- UNLOGGED テーブル: WALなしで高速書き込み、クラッシュ後データ消失、レプリケーション不可
- **判断**: 通常テーブルで実装（将来のレプリカ対応、キャッシュデータ量は少量）
- テーブル: `github_api_cache (user_id UUID, cache_type TEXT, value_json JSONB, expires_at TIMESTAMPTZ)`
- PK: `(user_id, cache_type)` でユーザー×キャッシュ種別のユニーク制約
- `expires_at` にインデックスを追加して掃除クエリを高速化

#### 4. sqlx JSONB + Upsert

- sqlx 0.8 は `serde_json::Value` を JSONB に直接 bind 可能（postgres feature で対応済み）
- 追加の feature flag 不要
- BoardFlow 既存コードで `serde_json::Value` + `ON CONFLICT DO UPDATE` パターンは多数使用済み

### 現在の実装構造

#### GithubAccessChecker トレイト（github_access.rs）

- `check_access(token, owner, name) -> AccessResult`: 単一リポジトリの権限チェック
- `list_accessible_repo_ids(token) -> Result<Option<Vec<i64>>, AccessError>`: ユーザーがアクセス可能なリポジトリID一覧
- 実装: `RealGithubAccessChecker` (本番)、`AllowAll/DenyAll/RateLimited/UpstreamError` (テスト)
- `DynGithubAccessChecker = Arc<dyn GithubAccessChecker>`
- `list_accessible_repo_ids` は `/user/repos?per_page=100` を全ページ取得、主に `list_repositories` ルートから呼び出される

#### DB構造

- 既存テーブル: repositories, board_projects, board_runs, artifacts, artifact_bundles, run_checks, run_check_findings, snapshots, diff_metadata, diffs, api_tokens, github_jobs, users, sessions, board_project_issue_history
- マイグレーション規約: `YYYYMMDDHHMMSS_description.{up,down}.sql`（例: `20260502000000_add_board_runs_timeout_sweep_index`）
- sqlx 0.8 使用、`sqlx::query_as` / `sqlx::query` パターン

### キャッシュ設計推奨事項

1. **Decorator パターン**: `CachedGithubAccessChecker` が `GithubAccessChecker` を実装し、内部に `RealGithubAccessChecker` + `PgPool` を持つ
2. **主要キャッシュ対象**: `list_accessible_repo_ids` の結果（`Vec<i64>` を JSONB で保存）
3. **TTL**: 5〜10分（設定はハードコードで開始、後で Config crate に移動可能）
4. **stale-while-error**: レートリミット時に期限切れキャッシュを返す（最大1時間保持）
5. **Invalidation**: TTL 超過で自動失効、将来的に `installation_repositories` Webhook で明示的削除
6. **check_access はキャッシュ不要**: 低頻度で個別リポジトリ単位、キャッシュの複雑性に見合わない

### 参照URL

- https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api
- https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/rate-limits-for-oauth-apps
- https://docs.github.com/en/rest/using-the-rest-api/troubleshooting-the-rest-api#rate-limit-errors
- https://github.com/hub4j/github-api/issues/1805
- https://martinheinz.dev/blog/105
- https://tech-couch.com/post/storing-sessions-or-cache-data-in-postgresql

### 結論ステータス

**`implementation_required`**

### 後続エージェントへの注意点

- `CachedGithubAccessChecker` は `crates/api/src/github_access.rs` に追加する（既存トレイト・モック群と同じファイル）
- マイグレーションファイルは `crates/db/migrations/20260503000000_add_github_api_cache.{up,down}.sql` として作成

---

## 実装フェーズ（2026-05-03 impl エージェント）

### 実装内容

#### 1. DBマイグレーション
- `crates/db/migrations/20260503000000_add_github_api_cache.up.sql`: `github_api_cache` テーブル作成（PK: `(user_id, cache_type)`, `expires_at` インデックス付き）
- `crates/db/migrations/20260503000000_add_github_api_cache.down.sql`: テーブルDROP

#### 2. DBクエリ関数
- `crates/db/src/queries/github_api_cache.rs` 新規作成
  - `get_valid_cache`: expires_at > NOW() のキャッシュを返す
  - `get_stale_cache`: 期限切れだが max_stale_duration 以内のキャッシュを返す（cutoffタイムスタンプ計算方式）
  - `upsert_cache`: INSERT ON CONFLICT DO UPDATE
  - `delete_cache_by_user`: ユーザーの全キャッシュ削除
  - `delete_cache`: 特定キャッシュ削除
  - `cleanup_expired_cache`: 1時間以上期限切れのキャッシュ一括削除
- `crates/db/src/queries/user.rs` に `find_by_github_access_token` 追加（token→user_id逆引き用）

#### 3. CachedGithubAccessChecker
- `crates/api/src/github_access.rs` 末尾に追加
- `GithubAccessChecker` トレイト実装:
  - `check_access`: inner に直接委譲
  - `list_accessible_repo_ids`:
    1. `find_by_github_access_token` で user_id 取得
    2. `get_valid_cache` でキャッシュヒット確認
    3. キャッシュミス → inner 呼び出し → 成功時 `upsert_cache`（TTL 10分）
    4. inner 失敗時 → `get_stale_cache`（最大1時間）でフォールバック
- `invalidate_cache(user_id)` 構造体メソッド追加

#### 4. lib.rs差し替え
- デフォルトの checker 生成を `CachedGithubAccessChecker::new(pool.clone())` に変更
- 既存テストは `access_checker: Some(...)` で mock を渡しているため影響なし

### テスト結果

12件の統合テスト（`crates/api/tests/github_cache_test.rs`）:
- `test_upsert_and_get_valid_cache`: 正常挿入・取得
- `test_get_valid_cache_returns_none_when_expired`: 期限切れは取得不可
- `test_get_stale_cache_returns_recently_expired`: staleウィンドウ内は取得可
- `test_get_stale_cache_returns_none_for_very_old_cache`: staleウィンドウ外は取得不可
- `test_upsert_cache_updates_existing`: UPSERT で上書き確認
- `test_delete_cache_removes_specific_entry`: 個別削除
- `test_delete_cache_by_user_removes_all`: ユーザー全削除
- `test_cleanup_expired_cache`: 期限切れ一括削除
- `test_cached_checker_invalidate_cache`: invalidate_cache 動作確認
- `test_cached_checker_returns_cached_repo_ids`: キャッシュヒット確認
- `test_cached_checker_unknown_token_passes_through`: 未知トークンはパススルー
- `test_cached_checker_stale_fallback_on_rate_limit`: stale エントリの確認

全ワークスペーステスト: **221 passed, 0 failed**

### 更新ドキュメント

- `docs/logs/63/worklog.md`（本ファイル）

### 残リスク

- `find_by_github_access_token` はインデックスなしのカラム検索。ユーザー数が多くなった場合はインデックス追加を検討
- `cleanup_expired_cache` は定期実行が必要（ジョブ/cronで呼び出す仕組みは未実装、将来Issue化推奨）
- Webhook による明示的 invalidation は未実装（将来 `installation_repositories` イベントで実装予定）

---

## レビュー修正フェーズ（2026-05-03 impl エージェント）

### 指摘事項と対応

#### 1. [MAJOR] staleフォールバックをRateLimited限定にする

**対応完了**: `list_accessible_repo_ids` のエラーハンドリングを変更:
- `Err(AccessError::RateLimited)` → staleキャッシュを探索して返す
- `Err(AccessError::TokenExpired)` / `Err(AccessError::Upstream(_))` → staleキャッシュを使わず即座にエラー伝播

#### 2. [MAJOR] invalidateを本番コードから呼べる形にする

**対応完了**: `GithubAccessChecker` トレイトに `invalidate_repo_cache` メソッドを追加:
- デフォルト実装: `Ok(())` (no-op) → 既存の AllowAll/DenyAll/RateLimited/UpstreamError モック全てが壊れずに動作
- `CachedGithubAccessChecker` でオーバーライド: `delete_cache_by_user` を呼び出し
- 本番コードで `DynGithubAccessChecker` (`Arc<dyn GithubAccessChecker>`) 経由で呼び出し可能

#### 3. [MINOR] innerをtrait objectにしてテスト容易性を上げる

**対応完了**: `CachedGithubAccessChecker` の内部構造を変更:
- `inner: RealGithubAccessChecker` → `inner: Arc<dyn GithubAccessChecker>`
- `new(pool)` は従来通り `RealGithubAccessChecker` を内部生成（後方互換）
- `with_inner(inner, pool)` コンストラクタ追加 → テスト時にモック注入可能

#### 4. [MINOR] github_access_tokenにインデックス追加

**対応完了**: マイグレーションファイルに追加:
```sql
CREATE INDEX idx_users_github_access_token ON users (github_access_token) WHERE github_access_token IS NOT NULL;
```
down マイグレーションにも `DROP INDEX IF EXISTS idx_users_github_access_token;` 追加

#### 5. テスト追加

**対応完了**: 5件の新テストを追加:
- `test_cached_checker_stale_fallback_with_mock_inner_rate_limited`: RateLimited時にstaleキャッシュが返ることをモックで実証
- `test_cached_checker_token_expired_no_stale_fallback`: TokenExpired時にstale不使用でエラー伝播
- `test_cached_checker_upstream_error_no_stale_fallback`: Upstream時にstale不使用でエラー伝播
- `test_invalidate_repo_cache_via_trait`: `DynGithubAccessChecker` 経由で `invalidate_repo_cache` が呼べることの確認
- `test_cached_checker_rate_limited_no_stale_returns_error`: RateLimited + staleなし → エラー返却

追加のモック: `TokenExpiredGithubAccessChecker` (常に TokenExpired を返す)

### テスト結果

全ワークスペーステスト: **全パス (0 failures)**

```
cargo build --workspace     ✓
cargo test --workspace      ✓
cargo clippy --workspace    ✓
cargo fmt --all             ✓
```

### 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `crates/api/src/github_access.rs` | トレイトに `invalidate_repo_cache` 追加、`CachedGithubAccessChecker` の inner を `Arc<dyn>` に変更、stale フォールバック限定化、`TokenExpiredGithubAccessChecker` モック追加 |
| `crates/api/tests/github_cache_test.rs` | 5件の新テスト追加、既存テストの重複ユーザー問題修正 |
| `crates/db/migrations/20260503000000_add_github_api_cache.up.sql` | `idx_users_github_access_token` インデックス追加 |
| `crates/db/migrations/20260503000000_add_github_api_cache.down.sql` | インデックスDROP追加 |

### 残リスク

- `cleanup_expired_cache` は定期実行が必要（ジョブ/cronで呼び出す仕組みは未実装、将来Issue化推奨）
- Webhook による明示的 invalidation は未実装（将来 `installation_repositories` イベントで実装予定）
- `invalidate_repo_cache` を実際に呼び出すユースケース（例: Webhook受信時）はこのIssue範囲外

---

## 計画フェーズ（2026-05-03 plan エージェント）

### 目的

- `list_accessible_repo_ids` の結果をDBにキャッシュし、GitHub API呼び出し回数を削減する
- レートリミット (429/403) 発生時に stale キャッシュでフォールバックする
- 将来の Webhook ベース invalidation に対応できるテーブル設計

### 非目的

- `check_access` のキャッシュ化（低頻度、個別リポジトリ単位でコスト対効果が低い）
- インメモリキャッシュ層の追加（マルチインスタンス不整合リスク）
- リトライロジックの実装（レートリミット時は stale キャッシュで対応）
- Webhook による明示的 invalidation の実装（別Issue）
- TTL の外部設定化（初回はハードコード）

### 受け入れ条件

1. `list_accessible_repo_ids` の結果がDBにキャッシュされ、TTL内は GitHub API を呼ばない
2. キャッシュミス時は GitHub API を呼び、結果をDBに upsert する
3. レートリミットエラー時、期限切れでも1時間以内のキャッシュがあればそれを返す
4. レートリミットエラー時、キャッシュも無ければ従来通りエラーを返す
5. 既存テストが全てパスする
6. トレイトシグネチャは変更しない

### 詳細要件

#### キャッシュ対象

| メソッド | キャッシュ | 理由 |
|---|---|---|
| `list_accessible_repo_ids` | **する** | 高頻度呼び出し、全ページ取得でAPIコスト高い |
| `check_access` | しない | 低頻度、個別リポジトリ、結果変動が大きい |

#### キャッシュキー設計

- `user_id` (UUID) をキーに使用
- `CachedGithubAccessChecker` は `github_access_token` から `users` テーブルで `user_id` を引き当てる
- これにより将来の Webhook invalidation (user_id ベース) に対応可能

#### TTL

| 状態 | 保持時間 |
|---|---|
| 正常キャッシュ | **10分** |
| stale-while-error (フォールバック用) | expires_at 後も **最大1時間** 行を保持 |

#### stale-while-error ロジック

1. キャッシュヒット (`expires_at > NOW()`) → そのまま返す
2. キャッシュミス → GitHub API 呼び出し → 成功なら upsert して返す
3. GitHub API が `RateLimited` → 期限切れだが1時間以内のキャッシュを探す (`expires_at > NOW() - INTERVAL '1 hour'`)
4. stale キャッシュあり → stale を返す (warning ログ)
5. stale キャッシュもなし → `AccessError::RateLimited` を返す

### 影響範囲

| ファイル | 変更内容 |
|---|---|
| `crates/db/migrations/20260503000000_add_github_api_cache.up.sql` | 新規: テーブル作成 |
| `crates/db/migrations/20260503000000_add_github_api_cache.down.sql` | 新規: テーブル削除 |
| `crates/db/src/queries/github_api_cache.rs` | 新規: upsert, get_valid, get_stale, delete_by_user, cleanup クエリ |
| `crates/db/src/queries/mod.rs` | 変更: `pub mod github_api_cache;` 追加 |
| `crates/api/src/github_access.rs` | 変更: `CachedGithubAccessChecker` 構造体 + impl 追加 |
| `crates/api/src/lib.rs` | 変更: checker 生成を `CachedGithubAccessChecker::new(pool)` に差し替え |

### 設計方針

#### 1. DBマイグレーション

```sql
-- UP (20260503000000_add_github_api_cache.up.sql)
CREATE TABLE github_api_cache (
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    cache_type  TEXT NOT NULL,
    value_json  JSONB NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, cache_type)
);

CREATE INDEX idx_github_api_cache_expires_at ON github_api_cache (expires_at);
```

```sql
-- DOWN (20260503000000_add_github_api_cache.down.sql)
DROP TABLE IF EXISTS github_api_cache;
```

#### 2. DB層クエリ関数 (`crates/db/src/queries/github_api_cache.rs`)

```rust
/// 有効なキャッシュ取得 (expires_at > NOW())
pub async fn get_valid(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    user_id: Uuid,
    cache_type: &str,
) -> Result<Option<serde_json::Value>, sqlx::Error>

/// stale キャッシュ取得 (期限切れだが1時間以内)
pub async fn get_stale(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    user_id: Uuid,
    cache_type: &str,
) -> Result<Option<serde_json::Value>, sqlx::Error>

/// キャッシュ upsert
pub async fn upsert(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    user_id: Uuid,
    cache_type: &str,
    value: &serde_json::Value,
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error>

/// ユーザー単位でキャッシュ削除 (将来のWebhook invalidation用)
pub async fn delete_by_user(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    user_id: Uuid,
) -> Result<(), sqlx::Error>

/// 期限切れ行の掃除 (1時間以上前に期限切れしたもの)
pub async fn cleanup_expired(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
) -> Result<u64, sqlx::Error>
```

#### 3. CachedGithubAccessChecker (`crates/api/src/github_access.rs`)

```rust
/// キャッシュ付き GitHub アクセスチェッカー (Decorator パターン)
pub struct CachedGithubAccessChecker {
    inner: RealGithubAccessChecker,
    pool: PgPool,
}

impl CachedGithubAccessChecker {
    pub fn new(pool: PgPool) -> Self {
        Self {
            inner: RealGithubAccessChecker::new(),
            pool,
        }
    }
}
```

**`list_accessible_repo_ids` 実装フロー:**

1. `SELECT id FROM users WHERE github_access_token = $1` で `user_id` を取得
   - 見つからない場合: inner に委譲（キャッシュスキップ）
2. `github_api_cache::get_valid(pool, user_id, "accessible_repo_ids")` でキャッシュ確認
   - ヒット: JSONB → `Vec<i64>` にデシリアライズして返す
3. キャッシュミス: `inner.list_accessible_repo_ids(token)` を呼び出す
   - 成功: `Vec<i64>` → JSONB にシリアライズ、`expires_at = NOW() + 10min` で upsert、結果を返す
   - `RateLimited`: `github_api_cache::get_stale(pool, user_id, "accessible_repo_ids")` で stale キャッシュ確認
     - stale あり: warning ログ + stale を返す
     - stale なし: `Err(AccessError::RateLimited)` を返す
   - その他エラー: そのまま返す

**`check_access` 実装:** inner に直接委譲（キャッシュなし）

#### 4. `crates/api/src/lib.rs` 変更

```rust
// Before:
let checker: DynGithubAccessChecker =
    access_checker.unwrap_or_else(|| Arc::new(RealGithubAccessChecker::new()));

// After:
let checker: DynGithubAccessChecker =
    access_checker.unwrap_or_else(|| Arc::new(CachedGithubAccessChecker::new(pool.clone())));
```

テスト時は `access_checker: Some(...)` で明示的にモックを渡しているため、テストは影響を受けない。

### Invalidate戦略

| トリガー | 方法 | 実装時期 |
|---|---|---|
| TTL超過 | `expires_at > NOW()` チェックで自動失効 | 今回 |
| stale掃除 | `cleanup_expired()` で1時間超え行を削除 | 今回（呼び出し元は将来のジョブキュー or アプリ起動時） |
| Webhook `installation_repositories` | `delete_by_user(user_id)` 呼び出し | 将来Issue |
| ユーザー削除 | `ON DELETE CASCADE` で自動削除 | テーブル定義で対応済み |

### テスト観点

1. **DB層ユニットテスト** (`crates/db` or integrationテスト)
   - `upsert` → `get_valid` で正しく取得できる
   - TTL切れ → `get_valid` が None を返す
   - TTL切れ1時間以内 → `get_stale` で取得できる
   - TTL切れ1時間超え → `get_stale` も None
   - `delete_by_user` でキャッシュが消える

2. **CachedGithubAccessChecker ユニットテスト**（既存テストとの整合性）
   - 既存のモックテスト（AllowAll, DenyAll, RateLimited, UpstreamError）は `access_checker` パラメータでモックを直接渡しているため、影響なし
   - `CachedGithubAccessChecker` 自体の結合テストは DB が必要 → 既存の integration test パターンに準じる

3. **既存テストの影響確認**
   - `crates/api/tests/read_api_test.rs`: モック使用、影響なし
   - `crates/api/tests/api_token_test.rs`: モック使用、影響なし
   - `crates/api/tests/proxy_test.rs`: モック使用、影響なし

### ドキュメント更新対象

| ファイル | 内容 |
|---|---|
| `docs/backend/api.md` | GitHub API キャッシュ層の説明追加 |
| `docs/backend/summary.md` | アーキテクチャ図にキャッシュ層を追記 |

### 実装順序

1. **DBマイグレーション作成** — テーブル定義（up/down）
2. **DB層クエリ関数** — `crates/db/src/queries/github_api_cache.rs` + mod.rs 登録
3. **CachedGithubAccessChecker 実装** — `crates/api/src/github_access.rs` に追加
4. **lib.rs 差し替え** — checker 生成を CachedGithubAccessChecker に変更
5. **ビルド確認** — `cargo build` + `cargo test`
6. **ドキュメント更新** — backend/api.md, backend/summary.md

### 実装要否

**`implementation_required`**

### 未解決の疑問

- なし（Research 調査で十分な情報が得られた。TTLはハードコード10分で開始）

### 残リスク

1. **キャッシュ陳腐化**: ユーザーが GitHub 側でリポジトリ権限を変更すると、最大10分古い情報が返る（許容範囲）
2. **token → user_id 逆引きコスト**: キャッシュヒット時でも `users` テーブルへの SELECT が1回発生する（テーブルサイズ小、高速）
3. **マルチインスタンス**: DBキャッシュなので問題なし（インメモリと異なり共有可能）
4. **Webhook invalidation 未実装**: TTL で最大10分の遅延は許容。別Issue で対応予定
- `crates/db/src/queries/` に `github_api_cache.rs` を追加し、`mod.rs` に登録
- `crates/api/src/lib.rs` の `RealGithubAccessChecker` の生成箇所を `CachedGithubAccessChecker` に差し替え
- テスト用モック（`AllowAll` 等）はキャッシュ不要なのでそのまま

## 残リスク

- キャッシュの陳腐化: リポジトリ権限変更がリアルタイム反映されない（TTL 5〜10分の遅延）
- `installation_repositories` Webhook によるキャッシュ invalidation は別 Issue で対応が自然
- ユーザーの GitHub トークンが他アプリと共有 → BoardFlow 側でのレートリミット残量は把握できない
- secondary rate limit の正確なしきい値は非公開 → キャッシュで呼び出し頻度を下げるのが最善策

---

## レビューフェーズ（2026-05-03 review エージェント）

### 対象

- Issue ID: #63
- タイトル: GitHub APIレスポンスのDBキャッシュとレートリミット対策

### レビュー結果

- `pr_ready: false`

### 指摘事項

1. **major**: stale フォールバックが `RateLimited` だけでなく `TokenExpired` や任意の `Upstream` エラーにも適用されている
    - `CachedGithubAccessChecker::list_accessible_repo_ids` は inner 呼び出し失敗時に無条件で stale キャッシュを返そうとするため、GitHub セッション失効時でも古い repository access list を返しうる
    - `list_repositories` は本来 `TokenExpired` を 401 に変換する設計だが、この実装だと stale キャッシュが優先されて 401 が握りつぶされる
    - 修正案: stale フォールバックは `AccessError::RateLimited` のみ、必要なら明示的に `AccessError::Upstream` の一部だけへ限定する。`TokenExpired` は必ずそのまま返す

2. **major**: invalidate は API/worker/webhook などの実運用経路に接続されておらず、Issue 要件としては未完了
    - `invalidate_cache(user_id)` は concrete type のメソッドとして追加されているが、アプリ起動時には `Arc<dyn GithubAccessChecker>` に erase されるため、通常経路から呼べない
    - 現状の参照はテストのみで、本番コードからは invalidate 不能
    - 修正案: webhook や明示 API から invalidate を呼べる導線を追加するか、少なくとも invalidate 用の抽象を別 trait / service として公開する

3. **minor**: 429/stale fallback の統合テストが実際の振る舞いを検証していない
    - テスト自身が `RealGithubAccessChecker` 固定のため true fallback を検証できないことをコメントで認めており、実際には stale 取得クエリの確認に留まっている
    - 修正案: `CachedGithubAccessChecker` が任意の `GithubAccessChecker` を inner に取れるようにして、`RateLimitedGithubAccessChecker` を注入した振る舞いテストを追加する

4. **minor**: token 逆引きクエリが無索引で、キャッシュ導入後の新しいホットパスになっている
    - `find_by_github_access_token` は request ごとに `users.github_access_token` を検索するが、現行 schema に index がない
    - 修正案: `github_access_token` へ index 追加を検討する。トークン列の取り扱い方針次第ではハッシュ化 + hash index/BTREE への移行も候補

### 必須修正

- stale フォールバック条件を `AccessError::RateLimited` に限定し、`TokenExpired` を確実に 401 へ流す
- invalidate を実際に利用できる本番経路へ接続し、Issue 要件の「invalidate」を満たす
- 429/RateLimited 時の stale fallback を本当に検証するテストを追加する

### 任意改善

- `CachedGithubAccessChecker` の inner を trait object / generic にして、Decorator としての再利用性とテスト容易性を上げる
- `users.github_access_token` の索引戦略を明示する
- `cleanup_expired_cache` の定期実行経路を追加し、古いキャッシュ行の滞留を防ぐ

### テスト不足

- `RateLimited` 発生時に stale が返る実挙動のテスト
- `TokenExpired` 発生時に stale を返さず 401 相当エラーが返るテスト
- invalidate の本番導線（例: webhook / service）を通したテスト

### ドキュメント確認

- 確認対象: `docs/spec.md`, `docs/technology.md`, `docs/external/github-api-rate-limit-cache.md`, `README.md`, `docs/logs/63/worklog.md`
- `CONTRIBUTING.md` は repository 内に見当たらず未確認
- docs への恒久的な反映は未実施。少なくとも invalidate 方針と stale fallback の制約は backend 方針や運用メモへ残した方がよい

### plan / research / docs との不整合

- 計画では「429/レートリミット時は stale キャッシュでフォールバック」となっているが、実装は `TokenExpired` を含む全エラーへ拡張されている
- 計画では invalidate を要件化しているが、実装は concrete method の追加のみで、運用経路がない
- 実装概要では stale fallback テスト完了と読めるが、実テストは fallback 振る舞いを直接検証していない

### PR/完了結果

- 現時点では `pr_ready: false`
- 主因はセッション失効時の stale 返却リスクと invalidate 未接続

### 残リスク

- GitHub token の失効・権限変更直後に stale データが返ると、閲覧可能 repository の判定が過大になる可能性がある
- cleanup が未接続のため、テーブルサイズは運用期間に応じて増加する

---

## レビューフェーズ（2026-05-03 review エージェント / 再レビュー）

### 対象

- Issue ID: #63
- タイトル: GitHub APIレスポンスのDBキャッシュとレートリミット対策

### レビュー結果

- `pr_ready: true`

### 総評

- 前回レビューで指摘した 4 件のコード修正とテスト補強は、現行実装ではいずれも反映済み
- `CachedGithubAccessChecker` は `RateLimited` 時だけ stale を返し、`TokenExpired` / `Upstream` はそのまま上位へ返すため、前回の 401 握りつぶしリスクは解消済み
- invalidate は `GithubAccessChecker` トレイト経由で呼べる形になっており、`DynGithubAccessChecker` 越しの呼び出しテストでも成立している
- `inner` の trait object 化と `with_inner` により、レートリミット系の振る舞いをモック注入で検証できるようになっている
- `users.github_access_token` へのインデックス追加も実装済みで、今回導入した逆引きホットパスと整合している

### 指摘事項

1. **minor**: 1件のテストが外部 GitHub 応答に依存しており、CI のネットワーク条件次第で非決定的になる
    - `crates/api/tests/github_cache_test.rs` の `test_cached_checker_unknown_token_passes_through` は `CachedGithubAccessChecker::new` を使って実際の `RealGithubAccessChecker` を呼ぶ
    - 現状の assert は `is_err()` のみなので壊れにくいが、外部通信・DNS・GitHub 側の遅延に影響される integration test になっている
    - 修正案: `with_inner(...)` で専用モックを注入し、「未知トークン時は DB キャッシュを使わず inner に委譲する」こと自体をローカルに検証する

### 必須修正

- なし

### 任意改善

- `test_cached_checker_unknown_token_passes_through` をモックベースに置き換えてテストの決定性を上げる
- `cleanup_expired_cache` の実行導線を将来 Issue で追加し、1時間超の失効行が残留し続けないようにする
- webhook など invalidate の実呼び出し元を追加する際は、今回追加した trait メソッドをそのまま利用して結合テストも追加する

### テスト不足

- blocking な不足は見当たらない
- ただし未知トークン時の pass-through は現在 external dependency を含むため、純粋なローカルテストへ寄せる余地がある

### ドキュメント確認

- 確認対象: `docs/spec.md`, `docs/external/github-api-rate-limit-cache.md`, `README.md`, `docs/logs/63/worklog.md`
- `CONTRIBUTING.md` は repository 内に存在せず未確認
- 恒久ドキュメントへの追記は必須ではないが、現状この変更の運用上の注意点は `docs/logs/63/worklog.md` に集約されている

### plan / research / docs との整合

- research の推奨事項である DB キャッシュ、TTL、RateLimited 限定 stale fallback、Decorator パターン、token 逆引きインデックス追加と整合している
- plan の受け入れ条件 1 から 5 に対して、コードとテストの両方で概ね一致している
- 前回レビューで指摘した stale 条件、invalidate 導線、inner の trait object 化、インデックス、テスト補強もいずれも反映済み

### PR/完了結果

- `pr_ready: true`
- 現時点では PR 作成を止める blocking issue は確認できなかった

### 残リスク

- webhook 連携による明示 invalidate と cleanup の定期実行は未接続のため、権限変更即時反映とキャッシュ掃除は TTL / 手動運用に依存する
- users テーブルに GitHub access token を保持して逆引きしている構造自体は既存設計の延長であり、今回の変更で新たな SQL injection リスクは見当たらないが、トークン保管方針そのものは将来的な設計論点として残る

---

## ドキュメント確認フェーズ（2026-05-03 docs エージェント）

### 確認対象

- `docs/backend/api.md`
- `docs/backend/summary.md`
- `docs/spec.md`
- `docs/external/github-api-rate-limit-cache.md`
- `docs/logs/63/worklog.md`

### 確認結果

- 実装済みの `CachedGithubAccessChecker`、`github_api_cache`、TTL 10分、RateLimited 時のみ stale-while-error 最大1時間、`invalidate_repo_cache` フック追加を確認
- `docs/external/github-api-rate-limit-cache.md` の採用判断と実装は整合している
- 一方で backend / spec ドキュメントには、Web UI read API の認可用 GitHub API キャッシュが未記載だった
- 既存の webhook 実装には `invalidate_repo_cache` 呼び出しが未接続であり、ここを「invalidate 実装済み」とだけ書くと過剰記述になるため、TTL ベース運用と将来拡張を明記するのが正しい

### 反映した更新

- `docs/backend/api.md`: read API の repository 可視性判定で DB キャッシュを使うこと、TTL 10分、RateLimited 時のみ stale cache、TokenExpired / Upstream では stale 不使用、Webhook invalidation 未接続を追記
- `docs/backend/summary.md`: backend アーキテクチャに `github_api_cache` と `CachedGithubAccessChecker` の役割を追記
- `docs/spec.md`: 補助テーブル `github_api_cache` と read API 権限判定用キャッシュ運用を追記

### ドキュメント判定

- `docs_ready: true`

### 必須修正

- なし

### 任意改善

- 将来 `installation_repositories` webhook から `invalidate_repo_cache` を実際に呼ぶ段階で、`docs/backend/api.md` と `docs/spec.md` の invalidation 記述を「将来拡張」から「実装済み」に更新する
- 運用タスクとして `cleanup_expired_cache` の定期実行方式が決まったら、backend 運用手順書にも追記する

### テスト結果

- ドキュメント更新のみのため追加テストは未実施

### レビュー結果

- 実装とドキュメントの不整合は解消済み
- PR作成可否: `docs_ready: true`

### 残リスク

- explicit invalidation のフック自体は実装済みだが、本番コードからの呼び出し経路は未接続
- `cleanup_expired_cache` の定期実行手段は引き続き未文書化

---

## PR作成フェーズ（2026-05-03 pr エージェント）

### PR/完了結果

- PRを作成: **https://github.com/f0reachARR/boardflow/pull/68**
- タイトル: `feat: GitHub APIレスポンスのDBキャッシュとレートリミット対策 (#63)`
- base: `main` ← head: `feature/63-github-api-cache`
- Closes #63

### PR作成前確認

| 確認項目 | 結果 |
|---|---|
| `pr_ready: true`（review エージェント） | ✓ |
| `docs_ready: true`（docs エージェント） | ✓ |
| 未コミット変更なし（worklog除く） | ✓ |
| `cargo build --workspace` | ✓ |
| `cargo test --workspace` | ✓ |
| `cargo clippy --workspace` | ✓ |
| `cargo fmt --all` | ✓ |

### コミット構成

| コミット | 内容 |
|---|---|
| `9b1738c` | feat(#63): add GitHub API cache layer with DB-backed TTL and stale-while-error |
| `4c6e3ec` | test(#63): add integration tests for GitHub API cache layer |
| `b8e0898` | docs(#63): update worklog with implementation details and test results |
| `07b284a` | fix(#63): address review feedback on GitHub API cache |
| `3d37cf6` | docs(#63): update worklog with review fix phase details |
| `b1912c8` | docs(#63): add cache layer documentation to api.md, summary.md and spec.md |

### 残リスク

- `cleanup_expired_cache` の定期実行（ジョブ/cron）は未実装 → 将来 Issue 化推奨
- Webhook (`installation_repositories`) による明示的 invalidation は未実装 → 別 Issue 予定
- `invalidate_repo_cache` の実際の呼び出しユースケース（Webhook 受信時など）は本 Issue 範囲外
