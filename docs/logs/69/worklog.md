# Issue #69: 期限切れキャッシュの定期クリーンアップジョブ実装

## 経緯
- ユーザー要望6: 期限切れキャッシュの定期クリーンアップジョブを実装
- 既存Issue #69 (OPEN) がそのまま要望に合致

## ユーザー要望
- `github_api_cache` テーブルの期限切れレコードを定期削除するジョブ

## Issue状態
- 既存Issue #69 がOPENで、内容は十分に明確
- `cleanup_expired_cache` メソッドは実装済み、定期実行トリガーが未実装
- 更新不要、そのまま処理対象とする

---

## 調査結果（2026-05-04 リサーチフェーズ）

### 1. `cleanup_expired_cache` メソッドの実装

- **場所**: `crates/db/src/queries/github_api_cache.rs:86-93`
- **シグネチャ**: `pub async fn cleanup_expired_cache(executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>) -> Result<u64, sqlx::Error>`
- **SQL**: `DELETE FROM github_api_cache WHERE expires_at < NOW() - INTERVAL '1 hour'`
- **戻り値**: 削除された行数
- テスト済み: `crates/api/tests/github_cache_test.rs::test_cleanup_expired_cache`

### 2. Worker crate の構造

- `crates/worker/src/main.rs` — エントリーポイント。`tokio::select!` ループで3分岐:
  1. shutdown シグナル (`ctrl_c`)
  2. `dispatcher::poll_and_dispatch()` — ジョブキューポーリング
  3. `sweep_interval.tick()` — 定期スイープ（デフォルト60秒）
- `crates/worker/src/dispatcher.rs` — ジョブディスパッチ + スイープ関数群
- `crates/worker/src/handlers/` — 個別ジョブハンドラ
- `crates/worker/src/config.rs` — `WorkerConfig` の re-export（実体は `crates/config/src/worker.rs`）

### 3. 既存の定期実行パターン

sweep tick 内で2つの定期タスクが実行されている:
- `dispatcher::sweep_timed_out_runs(&pool)` — 12時間超のBoardRunをタイムアウト
- `dispatcher::sweep_expired_staging_bundles(&pool, &s3_client, &config)` — 期限切れステージングバンドルのS3削除

パターン: `dispatcher.rs` に関数定義 → `main.rs` の sweep tick 分岐から呼び出し

### 4. `github_api_cache` テーブルスキーマ

マイグレーション: `crates/db/migrations/20260503000000_add_github_api_cache.up.sql`

```sql
CREATE TABLE github_api_cache (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    cache_type TEXT NOT NULL,
    value_json JSONB NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, cache_type)
);
CREATE INDEX idx_github_api_cache_expires_at ON github_api_cache (expires_at);
```

- `expires_at` にインデックスあり → DELETE の WHERE 条件で効率的にスキャン可能

### 5. WorkerConfig の構造

`crates/config/src/worker.rs`:
- `poll_interval_secs: u64` (デフォルト2秒) — ジョブポーリング間隔
- `timeout_sweep_interval_secs: u64` (デフォルト60秒) — スイープ間隔
- 環境変数: `POLL_INTERVAL_SECS`, `TIMEOUT_SWEEP_INTERVAL_SECS`

---

## 実装計画

### 方針: 専用 interval を `main.rs` に追加

既存 sweep（60秒）とは間隔が異なる（1時間）ため、独立した interval を追加する。

### 変更対象ファイル（4箇所）

| ファイル | 変更内容 |
|---|---|
| `crates/config/src/worker.rs` | `cache_cleanup_interval_secs: u64` フィールド追加（デフォルト3600） |
| `crates/worker/src/dispatcher.rs` | `sweep_expired_cache(pool)` 関数追加 |
| `crates/worker/src/main.rs` | `cache_cleanup_interval` を作成し `select!` に分岐追加 |
| `.env.example` | `CACHE_CLEANUP_INTERVAL_SECS=3600` 追加 |

### `dispatcher.rs` に追加する関数（イメージ）

```rust
pub async fn sweep_expired_cache(pool: &PgPool) {
    match boardflow_db::queries::github_api_cache::cleanup_expired_cache(pool).await {
        Ok(n) if n > 0 => {
            tracing::info!(count = n, "Cleaned up expired github_api_cache entries");
        }
        Ok(_) => {
            tracing::debug!("No expired github_api_cache entries to clean up");
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to clean up expired github_api_cache");
        }
    }
}
```

### `main.rs` の変更（イメージ）

```rust
let mut cache_cleanup_interval = tokio::time::interval(
    std::time::Duration::from_secs(config.cache_cleanup_interval_secs),
);
cache_cleanup_interval.tick().await;

loop {
    tokio::select! {
        _ = &mut shutdown => { break; }
        _ = dispatcher::poll_and_dispatch(...) => {}
        _ = sweep_interval.tick() => {
            dispatcher::sweep_timed_out_runs(&pool).await;
            dispatcher::sweep_expired_staging_bundles(&pool, &s3_client, &config).await;
        }
        _ = cache_cleanup_interval.tick() => {
            dispatcher::sweep_expired_cache(&pool).await;
        }
    }
}
```

---

## 結論ステータス

`implementation_required`

## 残リスク

- `poll_and_dispatch` の sleep が長い場合、cache_cleanup_interval の tick が遅延する可能性あり（既存 sweep と同じ既知の許容事項）
- `cleanup_expired_cache` は1時間超の失効行のみ削除するため、頻繁実行でもデータ損失リスクなし

## 参照URL

- tokio::time::interval: https://docs.rs/tokio/latest/tokio/time/fn.interval.html
- 調査メモ: `docs/external/tokio-periodic-task-interval.md`

---

## 詳細実装計画（2026-05-04 計画フェーズ）

### 目的

`github_api_cache` テーブルの期限切れレコードを Worker プロセス内で定期的に削除し、テーブル肥大化を防止する。

### 非目的

- 新しいジョブキュー機構の導入
- キャッシュ削除の即時性（分単位の遅延は許容）
- テーブルの `VACUUM` や `REINDEX` の自動実行

### 受け入れ条件

1. Worker 起動後、デフォルト1時間（3600秒）間隔で `cleanup_expired_cache` が呼ばれる
2. 間隔は環境変数 `CACHE_CLEANUP_INTERVAL_SECS` で変更可能
3. 削除件数が0件超の場合 `info` ログ、0件の場合 `debug` ログ、エラーの場合 `error` ログが出力される
4. 既存の sweep / poll 動作に影響しない
5. コンパイルが通り、既存テストが全てパスする

### 詳細要件

| # | 要件 |
|---|---|
| R1 | `WorkerConfig` に `cache_cleanup_interval_secs: u64` を追加（デフォルト3600） |
| R2 | 環境変数名は `CACHE_CLEANUP_INTERVAL_SECS` |
| R3 | `dispatcher.rs` に `pub async fn sweep_expired_cache(pool: &PgPool)` を追加 |
| R4 | `main.rs` に専用 `tokio::time::Interval` を作成し `select!` に分岐追加 |
| R5 | `.env.example` の Worker セクションに変数追記 |
| R6 | 既存テストの `make_config()` ヘルパに新フィールドを追加（コンパイル維持） |

### 影響範囲

- `crates/config/src/worker.rs` — 構造体 + `from_env` メソッド
- `crates/worker/src/dispatcher.rs` — 関数追加のみ（既存関数への変更なし）
- `crates/worker/src/main.rs` — interval 追加 + select 分岐追加
- `.env.example` — 1行追加
- `crates/config/tests/dotenv_integration_test.rs` — テスト内の `.env` 内容と assert 追加
- `crates/worker/tests/create_issue_test.rs` — `make_config()` にフィールド追加
- `crates/worker/tests/dashboard_comment_test.rs` — 同上
- `crates/worker/tests/run_result_comment_test.rs` — 同上

### 設計方針

#### 1. `crates/config/src/worker.rs`

```rust
pub struct WorkerConfig {
    // ... 既存フィールド ...
    pub cache_cleanup_interval_secs: u64,
}

impl WorkerConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        // ... 既存コード ...
        Ok(Self {
            // ... 既存フィールド ...
            cache_cleanup_interval_secs: parse_env_or("CACHE_CLEANUP_INTERVAL_SECS", 3600u64)?,
        })
    }
}
```

#### 2. `crates/worker/src/dispatcher.rs`

ファイル末尾に追加:

```rust
/// Delete expired rows from github_api_cache.
pub async fn sweep_expired_cache(pool: &PgPool) {
    use boardflow_db::queries::github_api_cache;

    match github_api_cache::cleanup_expired_cache(pool).await {
        Ok(n) if n > 0 => {
            tracing::info!(deleted = n, "Cleaned up expired github_api_cache entries");
        }
        Ok(_) => {
            tracing::debug!("No expired github_api_cache entries to clean up");
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to clean up expired github_api_cache");
        }
    }
}
```

#### 3. `crates/worker/src/main.rs`

`sweep_interval` の直後に:

```rust
let mut cache_cleanup_interval = tokio::time::interval(std::time::Duration::from_secs(
    config.cache_cleanup_interval_secs,
));
cache_cleanup_interval.tick().await; // 初回tickを消化
```

`select!` ブロック内に分岐追加:

```rust
_ = cache_cleanup_interval.tick() => {
    dispatcher::sweep_expired_cache(&pool).await;
}
```

#### 4. `.env.example`

Worker セクション末尾に追加:

```
CACHE_CLEANUP_INTERVAL_SECS=3600
```

#### 5. 既存テストの修正

各テストファイルの `make_config()` に `cache_cleanup_interval_secs: 3600` を追加。
`dotenv_integration_test.rs` の `worker_config_reads_values_from_dotenv` テストに:
- `.env` 文字列に `CACHE_CLEANUP_INTERVAL_SECS=1800` を追加
- `assert_eq!(config.cache_cleanup_interval_secs, 1800)` を追加
- `clear_env()` に `"CACHE_CLEANUP_INTERVAL_SECS"` を追加

### テスト観点

| テスト | 種類 | 確認内容 |
|---|---|---|
| `dotenv_integration_test::worker_config_reads_values_from_dotenv` | 統合 | 環境変数から値を読める |
| `WorkerConfig` デフォルト値 | 単体相当 | 環境変数未設定時にデフォルト3600 |
| 既存テスト全パス | 回帰 | `cargo test --workspace` が通る |
| `cleanup_expired_cache` 単体 | 既存 | `crates/api/tests/github_cache_test.rs` にて検証済み |
| `sweep_expired_cache` 関数 | — | DB 接続不要の呼び出しテストは不要（薄いラッパーのため） |

### ドキュメント更新対象

- `.env.example` — 環境変数追加
- `docs/backend/summary.md` — Worker 定期タスク一覧に追記（該当セクションがあれば）

### 実装要否

`implementation_required`

### 未解決の疑問

なし（全情報が揃っている）

### 更新した作業ログパス

`docs/logs/69/worklog.md`

---

## 実装結果（2026-05-04 実装フェーズ）

### 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `crates/config/src/worker.rs` | `cache_cleanup_interval_secs: u64` フィールド追加 + `from_env()` でパース（デフォルト3600） |
| `crates/worker/src/dispatcher.rs` | `pub async fn sweep_expired_cache(pool: &PgPool)` 関数追加 |
| `crates/worker/src/main.rs` | 専用 `cache_cleanup_interval` + `select!` 分岐追加 |
| `crates/worker/src/handlers/create_issue.rs` | テスト内 `WorkerConfig` に新フィールド追加 |
| `.env.example` | `CACHE_CLEANUP_INTERVAL_SECS=3600` 追加 |
| `crates/config/tests/dotenv_integration_test.rs` | `clear_env()` に新環境変数追加 |
| `crates/worker/tests/create_issue_test.rs` | `make_config()` に新フィールド追加 |
| `crates/worker/tests/dashboard_comment_test.rs` | 同上 |
| `crates/worker/tests/run_result_comment_test.rs` | 同上 |

### ビルド結果

- `cargo build --workspace` — **成功**

### テスト結果

- `cargo test -p boardflow-config -p boardflow-worker` — **全テスト成功**
- `cargo test --workspace` — 既存の `test_app_config_from_env`（boardflow-api）が環境変数汚染で失敗（DATABASE_URL が .env から読まれる既存問題、本変更と無関係）

### 受け入れ条件の充足

1. ✅ Worker 起動後、デフォルト1時間間隔で `cleanup_expired_cache` が呼ばれる
2. ✅ 環境変数 `CACHE_CLEANUP_INTERVAL_SECS` で変更可能
3. ✅ 削除件数>0 は info、0件は debug、エラーは error ログ
4. ✅ 既存の sweep / poll 動作に影響なし（独立した interval）
5. ✅ コンパイル成功、関連テスト全パス

### 残リスク

- なし（`cleanup_expired_cache` は1時間超失効行のみ削除するため、頻繁実行でもデータ損失リスクなし）
