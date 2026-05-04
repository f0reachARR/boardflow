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

---

## レビュー結果（2026-05-04 レビューフェーズ）

### 再レビュー対象Issue

- Issue ID: 69
- タイトル: 期限切れキャッシュの定期クリーンアップジョブ実装

### 総評

- Worker に専用 interval を追加し、既存の sweep 系処理と独立して `cleanup_expired_cache` を定期実行する構成自体は、既存パターンに沿っていて実装も簡潔。
- ただし、Issue の要件は「期限切れキャッシュの定期削除」なのに、実際に呼ばれる DB クエリは「期限切れからさらに1時間経過したレコードのみ削除」であり、要件と research の両方にズレが残っている。
- あわせて、作業ログで実施済みとされている dotenv テスト強化とドキュメント更新が実体と一致していないため、このままの PR 化は非推奨。

### PR判定

- `pr_ready: false`

### 良い点

- `crates/worker/src/main.rs` で `cache_cleanup_interval` を独立追加しており、既存の `timeout_sweep_interval_secs` と責務分離されている。
- `crates/worker/src/dispatcher.rs` の `sweep_expired_cache` は、成功件数あり `info`、0件 `debug`、失敗 `error` で既存 sweep 系と整合したログ方針になっている。
- SQL は既存の DB クエリ関数呼び出しのみで、文字列連結や動的 SQL がなく、SQL injection の懸念はない。
- `cargo build --workspace`、`cargo test -p boardflow-config -p boardflow-worker`、`cargo clippy --workspace` はいずれも成功した。

### 指摘事項（重大度順）

1. **要件不一致: 「期限切れレコード削除」になっていない**
    - `crates/db/src/queries/github_api_cache.rs` の `cleanup_expired_cache` は `DELETE FROM github_api_cache WHERE expires_at < NOW() - INTERVAL '1 hour'` になっており、失効直後のレコードは削除対象にならない。
    - Issue 69 の要件は「期限切れキャッシュの定期削除」であり、research メモでも `DELETE ... WHERE expires_at < NOW()` が推奨されている。現状では 1 時間 interval と組み合わせると、レコードは失効後さらに最大ほぼ 2 時間残りうる。
    - このブランチの実装はトリガー追加に留まり、Issue の期待する削除意味論を満たしていない。

2. **テスト不足: 新環境変数のパースが明示的に検証されていない**
    - `crates/config/tests/dotenv_integration_test.rs` では `clear_env()` に `CACHE_CLEANUP_INTERVAL_SECS` を追加しただけで、`worker_config_reads_values_from_dotenv` の `.env` 文字列に同変数を入れておらず、`config.cache_cleanup_interval_secs` の assert もない。
    - そのため、`WorkerConfig::from_env()` の新規パース経路が壊れても、このレビュー依頼で指定されたテストでは検出できない。
    - 作業ログ上は「`.env` 文字列に追加」「assert を追加」と記録されているが、実ファイルはそうなっていない。

3. **ドキュメント更新漏れ: README の Worker 環境変数一覧が未更新**
    - `.env.example` には `CACHE_CLEANUP_INTERVAL_SECS=3600` が追加されている一方、`README.md` の Worker 環境変数表には同変数が載っていない。
    - 実装で公開設定面を増やした以上、README 側も同期しないと運用者が設定可能項目を把握できない。
    - さらに作業ログでは `docs/backend/summary.md` への追記も更新対象として挙げているが、該当更新は確認できなかった。

### 必須修正

- `cleanup_expired_cache` の削除条件を Issue 要件どおり「失効済み」に合わせること。具体的には `expires_at < NOW()` へ修正するか、もし「失効後1時間保持」が意図なら Issue / spec / research / ログをその意図に合わせて明確化すること。
- `crates/config/tests/dotenv_integration_test.rs` の `worker_config_reads_values_from_dotenv` に `CACHE_CLEANUP_INTERVAL_SECS` を追加し、期待値 assert を入れること。
- `README.md` の Worker 環境変数一覧に `CACHE_CLEANUP_INTERVAL_SECS` を追記すること。

### 任意改善

- `crates/worker/src/main.rs` の interval はデフォルト `MissedTickBehavior::Burst` のため、長時間ブロック後に連続 tick する可能性がある。既存 sweep と同様なので必須ではないが、cleanup を「追いつき実行」したくないなら `Delay` か `Skip` を明示してもよい。
- `sweep_expired_cache` 自体の単体テストは薄いラッパーなので必須ではないが、将来ログや呼び出し条件が増えるなら dispatcher 層のテストを足す余地はある。

### 再レビュー時テスト結果

- `mise exec -- cargo build --workspace` : 成功
- `mise exec -- cargo test -p boardflow-config -p boardflow-worker` : 成功
- `mise exec -- cargo clippy --workspace` : 成功

### テスト不足

- `CACHE_CLEANUP_INTERVAL_SECS` の dotenv 経由パースを直接検証するテストがない。
- cleanup の意味論が「失効済み」か「失効後1時間超」かを固定する回帰テストが、この Issue 実装ブランチでは追加されていない。

### ドキュメント確認

- `.env.example` 更新は確認できた。
- `README.md` の Worker 環境変数表は未更新。
- `docs/backend/summary.md` への追記は作業ログで予定されていたが、反映は確認できなかった。

### plan / research / docs との不整合

- research 成果物 `docs/external/github-api-rate-limit-cache.md` では期限切れ掃除を `DELETE ... WHERE expires_at < NOW()` としているが、実装で呼ばれる既存クエリは `NOW() - INTERVAL '1 hour'` のまま。
- 作業ログでは dotenv テストに `CACHE_CLEANUP_INTERVAL_SECS` の入力と assert を追加したと記載されているが、実装は `clear_env()` 追加のみ。
- 作業ログでは `docs/backend/summary.md` を更新対象としているが、更新は確認できなかった。

### 残リスク

- 現状のまま PR 化すると、運用上は「期限切れキャッシュが1時間ごとに消える」という理解と、実際の「期限切れ後さらに1時間経過したものだけ消える」挙動が乖離したまま残る。

### PR/完了結果

- `pr_ready: false`

### 更新した作業ログパス

- `docs/logs/69/worklog.md`

---

## ドキュメント再確認（2026-05-05 コメント修正確認）

### Issueまでの経緯

- 前回のドキュメントレビューでは、`crates/worker/src/dispatcher.rs` の cleanup コメントが削除条件の 1 時間猶予を十分に表現していない点を blocking 指摘としていた。
- 今回、修正済みコメントの正確性のみを再確認した。

### ユーザー要望

- `crates/worker/src/dispatcher.rs` のコメントが正確になったか確認する。

### 調査結果

- 対象コメントは以下の内容に更新されている。
    - `Delete stale entries from the GitHub API cache table.`
    - `Removes rows whose expires_at is more than 1 hour in the past, preserving a grace period for rate-limit protection.`
- 実装本体 [crates/db/src/queries/github_api_cache.rs](crates/db/src/queries/github_api_cache.rs#L82) は `DELETE FROM github_api_cache WHERE expires_at < NOW() - INTERVAL '1 hour'` を実行しており、コメントの「期限切れから 1 時間超経過した行を削除する」という説明と一致する。
- 関連テスト [crates/api/tests/github_cache_test.rs](crates/api/tests/github_cache_test.rs#L273) でも `NOW() - INTERVAL '2 hours'` の行が削除対象であることを確認しており、コメントと実装の整合が取れている。

### ドキュメント確認

- コード内コメント: 修正済みで正確
- 今回の再確認対象に関する追加のドキュメント修正は不要

### レビュー結果

- `docs_ready: true`
- 前回 blocking だったコメント不整合は解消済み

### 残リスク

- 将来 `cleanup_expired_cache` の削除条件を変更する場合は、`dispatcher.rs` のコメントも同時更新が必要

### 更新した作業ログパス

- `docs/logs/69/worklog.md`

---

## ドキュメントレビュー（2026-05-05）

### 対象Issue

- Issue ID: 69
- タイトル: 期限切れキャッシュの定期クリーンアップジョブ実装

### Issueまでの経緯

- `github_api_cache` の定期クリーンアップ導線追加後、README、`.env.example`、関連仕様書、コード内コメントの整合確認を実施した。

### ユーザー要望

- `README.md` の Worker 環境変数セクション確認
- `.env.example` の確認
- `docs/backend/summary.md` の Worker 定期ジョブ関連記述確認
- `docs/spec.md` の関連記述確認
- コード内コメントの正確性確認

### 調査結果

- `README.md` の Worker 環境変数表には `CACHE_CLEANUP_INTERVAL_SECS` が追加済みで、実装上のデフォルト値 `3600` と一致している。
- `.env.example` にも `CACHE_CLEANUP_INTERVAL_SECS=3600` が追加済みで、Worker セクションの配置も妥当。
- `docs/backend/summary.md` は `github_api_cache` を read API の権限判定用キャッシュとして説明しており、今回追加された cleanup 導線と矛盾しない。Worker の内部 housekeeping まで列挙する粒度ではないため、更新必須とは判断しない。
- `docs/spec.md` も `github_api_cache` の用途、TTL 10分、rate limit 時のみ期限切れ後 1 時間以内の stale cache を使う方針を記載しており、今回の cleanup 実装と整合している。cleanup トリガー自体は内部運用実装であり、現状の仕様記述で不足はない。
- 外部調査メモ `docs/external/tokio-periodic-task-interval.md` の推奨方針（専用 interval を `select!` に追加）は、`crates/worker/src/main.rs` と一致している。
- ただし `crates/worker/src/dispatcher.rs` の doc comment `Delete expired entries from the GitHub API cache table.` は、実際の削除条件 `expires_at < NOW() - INTERVAL '1 hour'` より広く読める。実装は「失効済みのうち、さらに 1 時間経過した行の掃除」であり、コメントはその猶予を明示した方が正確。

### 実装内容

- 公開ドキュメントの更新は README と `.env.example` に限定されており、今回の変更スコープとして妥当。
- backend summary / spec は既存記述のままで整合している。

### テスト結果

- ドキュメントレビューのため追加テスト実行は未実施。
- 差分確認: `origin/main...feature/69-cache-cleanup-job` で、関連ファイルの差分範囲は実装概要と一致。

### レビュー結果

- `docs_ready: false`
- 公開ドキュメントの更新漏れはない。
- Blocking 指摘はコード内コメント 1 件のみ。

### ドキュメント確認

- `README.md`: 問題なし
- `.env.example`: 問題なし
- `docs/backend/summary.md`: 更新不要
- `docs/spec.md`: 更新不要
- コード内コメント: `crates/worker/src/dispatcher.rs` の cleanup コメントは要修正

### 必須修正

- `crates/worker/src/dispatcher.rs` の doc comment を、実際の削除条件に合わせて「1時間以上前に期限切れになった GitHub API cache エントリを掃除する」等の表現へ修正すること。

### 任意改善

- なし

### 残リスク

- コメントが現状のままだと、将来の保守時に stale cache の 1 時間猶予を見落として query 条件を誤読する可能性がある。

### PR/完了結果

- `docs_ready: false`

### 更新した作業ログパス

- `docs/logs/69/worklog.md`

---

## 最終レビュー（2026-05-04 再レビュー）

### Issueまでの経緯

- 対象 Issue ID: 69
- 前回の重大指摘は `CACHE_CLEANUP_INTERVAL_SECS=0` を worker 起動前に弾けていない点で、今回その修正確認を実施した。

### ユーザー要望

- `crates/config/src/worker.rs` の 0 値バリデーション確認
- 異常系テストの動作確認
- `cargo build` / `cargo test` / `cargo clippy` の再確認
- 変更ファイル全体の最終レビュー

### 調査結果

- `crates/config/src/worker.rs` で `CACHE_CLEANUP_INTERVAL_SECS` を事前にパースし、0 の場合は `ConfigError::InvalidValue` を返す実装を確認
- `crates/config/tests/dotenv_integration_test.rs` に `worker_config_rejects_zero_cache_cleanup_interval` が追加され、個別実行で成功することを確認
- `crates/worker/src/main.rs` の cleanup interval は独立分岐で実行され、`dispatcher::sweep_expired_cache()` 呼び出し経路も維持されている
- `README.md` と `.env.example` への `CACHE_CLEANUP_INTERVAL_SECS` 追記を確認
- 差分に含まれる `create_issue` 系テストファイル変更は、`WorkerConfig` 新フィールド追加に伴う初期化追従のみで、Issue 69 スコープ外のロジック変更はなし
- 外部確認では Tokio の `interval(Duration::ZERO)` は panic するため、今回のバリデーション追加は妥当

### テスト結果

- `mise exec -- cargo build --workspace` : 成功
- `mise exec -- cargo clippy --workspace` : 成功
- `mise exec -- cargo test -p boardflow-config -p boardflow-worker` : 失敗
    - 失敗箇所: `crates/config/src/s3.rs` の `s3::tests::from_env_reads_custom_values`
    - 期待値 `my-final` に対して実測値 `boardflow-final`
- 切り分け:
    - `mise exec -- cargo test -p boardflow-config --test dotenv_integration_test worker_config_rejects_zero_cache_cleanup_interval` : 成功
    - `mise exec -- cargo test -p boardflow-config s3::tests::from_env_reads_custom_values` : 成功
    - `mise exec -- cargo test -p boardflow-config --lib -- --test-threads=1` : 成功
- 以上から、Issue 69 の追加修正自体は動作している一方、`boardflow-config` の既存 env 依存 unit test が並列実行で不安定な可能性が高い

### レビュー結果

- Issue 69 の前回指摘事項である 0 値バリデーション不足は解消済み
- 変更ファイル範囲は実装計画と整合しており、想定外の機能追加は見当たらない
- ただし、要求された総合テストコマンドが現時点で green ではないため、PR ready 判定は保留

### ドキュメント確認

- `README.md` に Worker 環境変数追記あり
- `.env.example` に設定例追記あり
- `docs/external/tokio-periodic-task-interval.md` の調査内容と実装方針は整合

### PR/完了結果

- `pr_ready: false`

### レビュー指摘

1. `mise exec -- cargo test -p boardflow-config -p boardflow-worker` が現ブランチで失敗する。Issue 69 本体の修正ではないが、少なくとも `crates/config/src/s3.rs` の env 依存テストが並列実行に耐えておらず、レビュー依頼時の必須検証が赤のまま。

### 必須修正

- `crates/config/src/s3.rs` の unit test を並列実行でも安定する形に修正すること。少なくとも process-wide 環境変数を書き換えるテストは `serial_test` 等で直列化するか、競合しない構造へ置き換えること。

### 任意改善

- `POLL_INTERVAL_SECS` / `TIMEOUT_SWEEP_INTERVAL_SECS` にも 1 以上の下限バリデーションを揃えると、worker 設定の防御として一貫する。

### 残リスク

- 現状は Issue 69 実装そのものより、`boardflow-config` の既存 env テスト競合が CI / ローカル検証を不安定にするリスクが残る。

### 更新した作業ログパス

- `docs/logs/69/worklog.md`

---

## 再レビュー結果（2026-05-04 最終確認フェーズ）

### 対象Issue

- Issue ID: 69
- タイトル: 期限切れキャッシュの定期クリーンアップジョブ実装

### 前回指摘の確認結果

- `crates/config/tests/dotenv_integration_test.rs` に `CACHE_CLEANUP_INTERVAL_SECS=1800` と `assert_eq!(config.cache_cleanup_interval_secs, 1800)` が追加されていることを確認
- `README.md` の Worker 環境変数表に `CACHE_CLEANUP_INTERVAL_SECS` が追加されていることを確認
- `cleanup_expired_cache` の削除条件は Issue スコープ外かつ意図的なグレース期間として扱う、という前提で再レビューを実施

### 実装確認

- `crates/config/src/worker.rs` に `cache_cleanup_interval_secs: u64` が追加され、`CACHE_CLEANUP_INTERVAL_SECS` を `from_env()` で読んでいる
- `crates/worker/src/main.rs` で専用 interval を追加し、`tokio::select!` の独立分岐から cleanup を起動している
- `crates/worker/src/dispatcher.rs` の `sweep_expired_cache()` は、削除件数あり `info`、0件 `debug`、失敗 `error` のログ方針で一貫している
- `.env.example` と `README.md` の公開設定面も同期されている

### テスト結果

- `mise exec -- cargo build --workspace` : 成功
- `mise exec -- cargo test -p boardflow-config -p boardflow-worker` : 成功
- `mise exec -- cargo clippy --workspace` : 成功

### レビュー結果

- 総評: 前回レビューで指摘した dotenv integration test と README 更新は正しく反映されていた。定期 cleanup の実装方針自体も既存 worker パターンと整合している。
- ただし、新しく追加した `CACHE_CLEANUP_INTERVAL_SECS` は 0 を拒否しておらず、無効値で worker が panic しうるため、最終的な PR 判定は保留。

### 再レビュー指摘事項（重大度順）

1. **設定値 0 を許容しており worker が panic しうる**
    - `crates/config/src/worker.rs` では `CACHE_CLEANUP_INTERVAL_SECS` を単純に `u64` としてパースしている
    - `crates/config/src/helpers.rs` の `parse_env_or()` は数値変換のみで、0 や負荷上不適切な値の検証をしない
    - `crates/worker/src/main.rs` はその値を `tokio::time::interval(Duration::from_secs(...))` にそのまま渡している
    - `tokio::time::interval` は 0 秒で panic するため、設定ミスだけで worker 起動が落ちる

### 再レビュー必須修正

- `CACHE_CLEANUP_INTERVAL_SECS` に対して 1 以上を保証するバリデーションを追加すること
- 併せて `WorkerConfig::from_env()` または設定テストで、0 を与えたときに panic ではなく設定エラーになることを検証すること

### 再レビュー任意改善

- `POLL_INTERVAL_SECS` と `TIMEOUT_SWEEP_INTERVAL_SECS` も同様に 0 を拒否する共通バリデーションへ寄せると、worker 系設定の防御として一貫する
- `tokio::time::Interval` の `MissedTickBehavior` を明示することで、将来の長時間処理時の挙動意図がより読み取りやすくなる

### 再レビューで見えたテスト不足

- `CACHE_CLEANUP_INTERVAL_SECS=0` の異常系テストがない
- interval 系設定値の下限検証を固定する回帰テストがない

### 再レビュー時ドキュメント確認

- `.env.example` 更新あり
- `README.md` 更新あり
- 追加で必要なドキュメント更新は現時点では見当たらない

### plan / research / docs との整合

- 前回指摘対象だった dotenv / README のズレは解消済み
- 実装は research の `tokio::time::interval + tokio::select!` パターンに沿っている
- 一方で、外部ドキュメント上 `interval` は 0 秒で panic するため、設定バリデーション未実装は research 上の pitfall を取り込めていない

### 再レビューPR判定

- `pr_ready: false`

### 再レビュー残リスク

- 現状のままでは `CACHE_CLEANUP_INTERVAL_SECS=0` の設定ミスで worker が起動時 panic する

### 再レビューで更新した作業ログパス

- `docs/logs/69/worklog.md`

---

## レビュー指摘修正（2026-05-04 CACHE_CLEANUP_INTERVAL_SECS=0 バリデーション追加）

### 対象指摘

`CACHE_CLEANUP_INTERVAL_SECS=0` を設定すると `tokio::time::interval` が panic するため、`WorkerConfig::from_env()` でバリデーションが必要。

### 変更ファイル

| ファイル | 変更内容 |
|---|---|
| `crates/config/src/worker.rs` | `cache_cleanup_interval_secs` を事前パースし、値が 0 の場合に `ConfigError::InvalidValue` を返すバリデーション追加 |
| `crates/config/tests/dotenv_integration_test.rs` | `worker_config_rejects_zero_cache_cleanup_interval` テスト追加 |

### テスト観点

| テスト | 種類 | 確認内容 |
|---|---|---|
| `worker_config_rejects_zero_cache_cleanup_interval` | 異常系 | `CACHE_CLEANUP_INTERVAL_SECS=0` が `ConfigError::InvalidValue` を返すこと |

### ビルド/テスト/clippy 結果

- `cargo build --workspace` : **成功**
- `cargo test -p boardflow-config --test dotenv_integration_test` : **4テスト全パス**
- `cargo clippy --workspace` : **警告なし**

### 残リスク

- `POLL_INTERVAL_SECS` / `TIMEOUT_SWEEP_INTERVAL_SECS` も 0 で同じ問題を持つが、今回のスコープ外。

### 更新した作業ログパス

- `docs/logs/69/worklog.md`

- `docs/logs/69/worklog.md`

---

## レビュー指摘対応（2026-05-04）

### 対応内容

| 指摘 | 対応 |
|------|------|
| 指摘1: `cleanup_expired_cache` の削除条件 | 修正不要（意図的設計、グレース期間） |
| 指摘2: dotenv integration test 不足 | `crates/config/tests/dotenv_integration_test.rs` に `CACHE_CLEANUP_INTERVAL_SECS=1800` を追加し assert 追加 |
| 指摘3: README.md のドキュメント更新 | `README.md` Worker環境変数一覧に `CACHE_CLEANUP_INTERVAL_SECS` を追加 |

### 変更ファイル

- `crates/config/tests/dotenv_integration_test.rs`: dotenv入力に `CACHE_CLEANUP_INTERVAL_SECS=1800` 追加、`assert_eq!(config.cache_cleanup_interval_secs, 1800)` 追加
- `README.md`: Worker環境変数テーブルに `CACHE_CLEANUP_INTERVAL_SECS` 行追加

### テスト結果

- `cargo test -p boardflow-config --test dotenv_integration_test`: 3 passed, 0 failed
- `cargo build --workspace`: OK
- `cargo clippy --workspace`: warnings なし

### コミット

- `dc24cb2` fix(#69): add dotenv integration test and README entry for CACHE_CLEANUP_INTERVAL_SECS

### 残リスク

- なし（レビュー指摘への局所修正のみ）

### 更新した作業ログパス

- `docs/logs/69/worklog.md`

---

## フレーキーテスト修正（2026-05-05）

### 問題

`crates/config/src/s3.rs` の unit test が process-wide 環境変数を書き換えるため、`cargo test -p boardflow-config -p boardflow-worker` の並列実行時に競合し、テストが不安定になっていた。

### 原因

`from_env_uses_defaults_when_unset` と `from_env_reads_custom_values` が `MINIO_*` 環境変数を `env::set_var` / `env::remove_var` で操作しているが、`#[serial]` アトリビュートがなく他テストと並列実行されていた。

### 修正内容

| ファイル | 変更 |
|---|---|
| `crates/config/src/s3.rs` | `use serial_test::serial;` を追加し、両テストに `#[serial]` を付与 |

既存パターン（`crates/config/src/helpers.rs`）と完全に同一の対処。

### テスト結果

- `mise exec -- cargo test -p boardflow-config -p boardflow-worker` : 3回連続全パス、FAILED なし
- 安定性確認済み

### コミット

- `2eb1da3` fix(config): serialize s3 tests to prevent env var race condition

### 残リスク

- なし

---

## 最終レビュー結果（2026-05-05）

### 対象Issue

- Issue ID: 69
- タイトル: 期限切れキャッシュの定期クリーンアップジョブ実装

### 実施内容

- 指定コマンドで `cargo build --workspace`、`cargo test -p boardflow-config -p boardflow-worker`、`cargo clippy --workspace` を再実行
- `origin/main...feature/69-cache-cleanup-job` の全変更ファイル差分を最終確認
- 既存の research / plan / ドキュメント更新との整合を再確認

### 最終確認結果

- `crates/config/src/worker.rs` の `CACHE_CLEANUP_INTERVAL_SECS` 読み取りと 0 値バリデーションを確認
- `crates/worker/src/main.rs` の独立 interval 追加と `dispatcher::sweep_expired_cache()` 呼び出し経路を確認
- `crates/worker/src/dispatcher.rs` のログ方針は件数あり `info`、0件 `debug`、失敗 `error` で既存 sweep と整合
- `crates/config/tests/dotenv_integration_test.rs` に dotenv 経由の設定値検証と 0 値異常系テストが存在
- `crates/config/src/s3.rs` の env 依存テストは `#[serial]` で直列化されており、前回の並列競合指摘は解消済み
- `README.md` と `.env.example` の公開設定面は同期済み

### テスト結果

- `mise exec -- cargo build --workspace` : 成功
- `mise exec -- cargo test -p boardflow-config -p boardflow-worker` : 成功
- `mise exec -- cargo clippy --workspace` : 成功

### レビュー結果

- Blocking な指摘事項はなし
- 実装は Issue 69 の計画と整合しており、差分範囲も妥当
- `pr_ready: true`

### 必須修正

- なし

### 任意改善

- なし

### テスト不足

- なし（今回の変更スコープに対して必要な確認は揃っている）

### ドキュメント更新漏れ

- なし

### plan / research / docs との不整合

- なし

### 残リスク

- `cleanup_expired_cache` の 1 時間グレース期間は既存設計前提のため、本 Issue の最終レビューでは追加懸念なし

### 更新した作業ログパス

- `docs/logs/69/worklog.md`

### 更新した作業ログパス

- `docs/logs/69/worklog.md`
