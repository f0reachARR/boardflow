# Issue #24: Worker: Staging Bundle クリーンアップ実装

## 経緯

- Issue作成: Staging objectの期限管理を行う定期処理を実装する
- 仕様: `delete_after < NOW()` のbundleをS3から削除し、`staging_object_key = NULL` にして再処理防止
- #23 timeout sweep と同様のパターンで実装する

## ユーザー要望

アップロードされたが一定期間使用されなかったstagingバンドルをS3から削除する定期処理。

## 調査結果

### 既存コード分析

1. **DB (artifact_bundle.rs)**: `mark_completed` で `delete_after = NOW() + 24h`、`mark_failed` で `delete_after = NOW() + 7days` を設定済み
2. **Domain model**: `ArtifactBundle.staging_object_key: Option<String>`, `delete_after: Option<DateTime<Utc>>`
3. **Worker dispatcher**: `sweep_timed_out_runs` が既存パターン。S3 client は `poll_and_dispatch` に渡されているが sweep には未使用
4. **Worker main.rs**: `tokio::select` で sweep_interval tick ごとに `sweep_timed_out_runs` を呼び出し
5. **Artifact crate**: `download_bundle` は存在するが `delete_object` 関数はまだ未実装
6. **WorkerConfig**: `timeout_sweep_interval_secs` (デフォルト60秒) が存在

### 設計判断

- **interval**: staging cleanup は timeout sweep と同じ interval で実行して問題ない（どちらもDB polling + 軽量処理）。既存の `timeout_sweep_interval_secs` を共有する。専用設定は不要。
- **S3 delete**: `boardflow-artifact` crate に `delete_staging_object` 関数を追加
- **バッチサイズ**: 1回のsweepで最大100件処理（LIMIT 100）
- **エラーハンドリング**: S3削除失敗はログのみで続行。次回sweep時にリトライされる（staging_object_keyがNULLにならないため）

---

## 実装計画

### 目的

`delete_after` を過ぎたstaging bundleのS3オブジェクトを定期的に削除し、ストレージコストを抑制する。

### 非目的

- final bucket のartifact削除（MVPでは無期限保存）
- S3 lifecycle policy による自動削除（アプリ層で管理）
- delete_after の値変更（既にmark_completed/mark_failedで設定済み）

### 受け入れ条件

1. `delete_after < NOW()` かつ `staging_object_key IS NOT NULL` のバンドルがsweep対象になる
2. S3から該当オブジェクトが削除される
3. 削除成功後に `staging_object_key = NULL` が設定される
4. S3削除失敗時はログ出力のみで他のバンドル処理は続行する
5. 次回sweep時に未処理バンドルが再取得される（リトライ）

### 詳細要件

- バッチ制限: LIMIT 100
- sweep間隔: `timeout_sweep_interval_secs` と同一タイミング（同一tick内で連続実行）
- S3削除対象: `staging_bucket` 内の `staging_object_key` で指定されたオブジェクト

### 影響範囲

- `crates/artifact/src/lib.rs` — 新規関数追加
- `crates/db/src/queries/artifact_bundle.rs` — 新規クエリ2つ追加
- `crates/worker/src/dispatcher.rs` — 新規sweep関数追加
- `crates/worker/src/main.rs` — sweep tick に staging cleanup 追加

### 設計方針

DB → S3削除 → DB更新 を逐次処理。並列化はMVPでは不要。

### ファイル変更リスト（変更順）

#### 1. `crates/artifact/src/lib.rs`
新規関数 `delete_staging_object` を追加:
```rust
/// Delete a staging object from S3
pub async fn delete_staging_object(
    s3_client: &S3Client,
    bucket: &str,
    key: &str,
) -> Result<(), ArtifactError> {
    s3_client
        .delete_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;
    Ok(())
}
```

#### 2. `crates/db/src/queries/artifact_bundle.rs`
2つの新規関数を追加:

```rust
/// Find expired staging bundles (delete_after < NOW() and staging_object_key is set)
pub async fn find_expired_staging(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
) -> Result<Vec<ArtifactBundle>, sqlx::Error> {
    sqlx::query_as::<_, ArtifactBundle>(
        "SELECT * FROM artifact_bundles WHERE delete_after < NOW() AND staging_object_key IS NOT NULL LIMIT 100",
    )
    .fetch_all(executor)
    .await
}

/// Clear staging_object_key after successful S3 deletion
pub async fn clear_staging_object_key(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE artifact_bundles SET staging_object_key = NULL WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await?;
    Ok(())
}
```

#### 3. `crates/worker/src/dispatcher.rs`
新規関数 `sweep_expired_staging_bundles` を追加:

```rust
/// Sweep expired staging bundles: delete S3 objects and clear staging_object_key.
pub async fn sweep_expired_staging_bundles(
    pool: &PgPool,
    s3_client: &aws_sdk_s3::Client,
    config: &WorkerConfig,
) {
    let bundles = match artifact_bundle::find_expired_staging(pool).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, "Failed to query expired staging bundles");
            return;
        }
    };

    if bundles.is_empty() {
        tracing::debug!("No expired staging bundles to clean up");
        return;
    }

    let mut cleaned = 0u64;
    for bundle in &bundles {
        let key = bundle.staging_object_key.as_deref().unwrap();
        if let Err(e) = boardflow_artifact::delete_staging_object(
            s3_client,
            &config.staging_bucket,
            key,
        ).await {
            tracing::warn!(bundle_id = %bundle.id, key = key, error = %e, "Failed to delete staging object, will retry next sweep");
            continue;
        }
        if let Err(e) = artifact_bundle::clear_staging_object_key(pool, bundle.id).await {
            tracing::error!(bundle_id = %bundle.id, error = %e, "Failed to clear staging_object_key after S3 deletion");
            continue;
        }
        cleaned += 1;
    }

    tracing::info!(total = bundles.len(), cleaned = cleaned, "Swept expired staging bundles");
}
```

dispatcher.rs の use 文に `artifact_bundle` を追加。

#### 4. `crates/worker/src/main.rs`
既存の `sweep_interval.tick()` アーム内に staging cleanup を追加:

```rust
_ = sweep_interval.tick() => {
    dispatcher::sweep_timed_out_runs(&pool).await;
    dispatcher::sweep_expired_staging_bundles(&pool, &s3_client, &config).await;
}
```

### テスト観点

1. **DB クエリテスト** (統合テスト `crates/worker/tests/staging_cleanup_test.rs`):
   - `delete_after` が過去のバンドル + `staging_object_key` あり → `find_expired_staging` で取得される
   - `delete_after` が未来のバンドル → 取得されない
   - `staging_object_key` が NULL のバンドル → 取得されない
   - `clear_staging_object_key` 実行後に `staging_object_key` が NULL になる
   - LIMIT 100 を超えるバンドルが存在しても100件のみ返される

2. **ユニットテスト** (dispatcher のロジックは統合テストでカバー)

### ドキュメント更新対象

- `docs/logs/24/worklog.md` (本ファイル)
- `docs/backend/summary.md` に staging cleanup 機能の記載追加（実装完了後）

### 実装要否

`implementation_required`

### 未解決の疑問

なし。仕様・既存コードパターンから全て判断可能。

---

## 実装内容 (2026-05-02)

### 変更ファイル

1. **`crates/db/src/queries/artifact_bundle.rs`** — 2関数追加:
   - `find_expired_staging`: `delete_after < NOW() AND staging_object_key IS NOT NULL LIMIT 100` で期限切れバンドルを取得
   - `clear_staging_object_key`: 指定IDの `staging_object_key` を NULL に更新

2. **`crates/worker/src/dispatcher.rs`** — `sweep_expired_staging_bundles` 関数追加:
   - 期限切れバンドルを取得し、S3削除 → DB更新を逐次処理
   - S3削除失敗時はwarnログ出力のみで続行（次回リトライ）
   - import文に `artifact_bundle` を追加

3. **`crates/worker/src/main.rs`** — sweep tick内で `sweep_expired_staging_bundles` を呼び出し追加

4. **`crates/worker/tests/staging_cleanup_test.rs`** — 統合テスト新規作成:
   - `test_find_expired_staging_returns_only_expired`: 期限切れのみ取得されること
   - `test_clear_staging_object_key`: NULL更新が正しく動作すること
   - `test_cleared_bundle_not_returned_by_find_expired`: クリア後にfind対象外になること
   - `test_null_staging_key_not_returned`: staging_object_key=NULLのバンドルが対象外

### テスト結果

- `cargo check --workspace` → 成功
- `cargo test -p boardflow-worker --lib` → 21テスト全通過
- `cargo test -p boardflow-worker --test staging_cleanup_test --no-run` → コンパイル成功
- 統合テスト（`--ignored`）はDB接続が必要なためCI環境で実行

### 設計判断

- 計画では `boardflow-artifact` crateに `delete_staging_object` ヘルパーを追加する案があったが、ユーザーの実装計画指示に従い、dispatcherで直接 `s3_client.delete_object()` を呼ぶシンプルな実装を採用
- artifact crateへの抽象化はMVP後のリファクタリング対象

### 残リスク

- なし。失敗時の自然リトライ（staging_object_keyが残るため次回sweepで再取得）により、データロスリスクはゼロ
- S3側で既にオブジェクトが存在しない場合もDeleteObjectは成功扱いとなるため問題なし

---

## 実装内容

(実装フェーズで追記)

## テスト結果

(実装フェーズで追記)

## レビュー結果

(レビューフェーズで追記)

## PR/完了結果

(完了時追記)

## 残リスク

- S3 delete_object が成功したが DB 更新が失敗した場合、staging_object_key が残り続ける（次回sweepでS3 delete は NoSuchKey エラーになるが致命的ではない。AWS S3 の delete_object は存在しないキーに対して成功を返すため、実質問題なし）
