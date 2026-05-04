# tokio::time::interval を用いた定期タスクパターン

## 要約

BoardFlow Worker は `tokio::time::interval` + `tokio::select!` ループで定期タスクを実行している。Issue #69 のキャッシュクリーンアップもこのパターンに乗せるのが自然。

## 確認した情報

### 既存のパターン（`crates/worker/src/main.rs`）

Worker の `main` 関数は以下の構造:

```rust
let mut sweep_interval = tokio::time::interval(Duration::from_secs(config.timeout_sweep_interval_secs));
sweep_interval.tick().await; // 初回tick消化

loop {
    tokio::select! {
        _ = &mut shutdown => { break; }
        _ = dispatcher::poll_and_dispatch(...) => {}
        _ = sweep_interval.tick() => {
            dispatcher::sweep_timed_out_runs(&pool).await;
            dispatcher::sweep_expired_staging_bundles(&pool, &s3_client, &config).await;
        }
    }
}
```

- `sweep_interval` はデフォルト60秒間隔（`TIMEOUT_SWEEP_INTERVAL_SECS`）
- sweep tick で `sweep_timed_out_runs` と `sweep_expired_staging_bundles` を実行

### キャッシュクリーンアップの要件

- 実行間隔: 1時間程度（sweep の60秒とは異なる）
- 対象: `github_api_cache` テーブルの `expires_at < NOW() - INTERVAL '1 hour'` のレコード
- DB呼び出しのみ（S3等の外部リソース不要）

### 設計選択肢

1. **専用 interval を追加** — `cache_cleanup_interval` を新設し `select!` に分岐追加
2. **既存 sweep に相乗り** — sweep_interval tick 内で毎回呼ぶ（60秒ごとに DELETE 実行）
3. **カウンタ付き相乗り** — sweep tick N回ごとに実行

## BoardFlow への示唆

- **選択肢1（専用 interval）が推奨**。理由:
  - 1時間と60秒では桁が違うため、責務を分離した方がログの可読性・設定の独立性が高い
  - `tokio::select!` に分岐追加するだけで、既存コードへの影響が最小限
  - `CACHE_CLEANUP_INTERVAL_SECS` を `WorkerConfig` に追加すれば、環境ごとの調整が容易
- **選択肢2も許容範囲**。DELETE 文は `expires_at` インデックスを使うため、60秒ごとに空振りしてもコストは極めて低い。実装がさらに簡素になる利点がある。

## 採用/不採用判断

- **採用**: `tokio::time::interval` + `select!` パターン
- **不採用**: 外部 cron / 別プロセスでのスケジューリング（Worker 内で完結するため不要）

## 制約と pitfall

- `tokio::select!` の biased モードは使っていないため、分岐の優先度は不定。shutdown シグナルの `&mut` pin で先頭チェックは保証される。
- `poll_and_dispatch` がジョブ無し時に `poll_interval_secs` 秒 sleep するため、sweep/cleanup の tick が即座に処理されない場合がある（既知の許容事項）。
- `cleanup_expired_cache` は1時間超の失効行のみ削除するため、仮に頻繁に呼んでもデータ損失リスクなし。

## 未解決の疑問

- なし（パターンが確立済み）

## 参照URL

- tokio::time::interval: https://docs.rs/tokio/latest/tokio/time/fn.interval.html
- tokio::select!: https://docs.rs/tokio/latest/tokio/macro.select.html
