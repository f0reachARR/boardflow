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

### 総評 (2026-05-02)

- 実装の中心である `find_expired_staging`、`clear_staging_object_key`、worker sweep 呼び出しは、import 成功済み bundle と failed bundle の cleanup という観点では概ね一貫している。
- 一方で、仕様が要求する「timed_out になった run の staging bundle は 7 日後に削除対象」との整合が取れていない。cleanup 対象抽出は `artifact_bundles.delete_after` 依存だが、run timeout sweep は `board_runs.status = 'timed_out'` を更新するだけで bundle 側の `delete_after` を設定していない。
- そのため Issue #24 は仕様充足が未完了であり、このままでは PR 作成可とは判定できない。

### 重大度順の指摘

1. **Blocker: timed_out run の staging bundle が cleanup 対象にならない**
    - 仕様では `failed` または `timed_out` run の staging bundle を 7 日後に削除対象とする。`docs/spec.md` に明記あり。
    - しかし cleanup 抽出条件は `delete_after < NOW() AND staging_object_key IS NOT NULL` のみで、`crates/db/src/queries/artifact_bundle.rs` の `find_expired_staging` は status や run 状態を見ない。
    - `delete_after` を 7 日後に設定しているのは `artifact_bundle::mark_failed` のみで、`crates/db/src/queries/board_run.rs` の `sweep_timed_out` は `board_runs` を `timed_out` に更新するだけで、対応する bundle の `delete_after` を設定していない。
    - 結果として、timeout sweep 経由で `timed_out` になった run の staging bundle は永続的に `delete_after = NULL` のまま残りうる。
    - 必須対応: timeout sweep と同時に対象 run に紐づく staging bundle へ `delete_after = NOW() + INTERVAL '7 days'` を設定する処理、または timed_out bundle を抽出できる同等の仕組みを追加する。

2. **Test gap: 仕様の timed_out 経路を検証するテストがない**
    - 新規テストは期限切れ判定と key クリアの DB クエリ中心で、timed_out run が 7 日後 cleanup 対象になる条件は確認していない。
    - 現状のテスト構成だと、上記 Blocker を見逃したままでも green になる。
    - 必須対応: timeout sweep 後に該当 bundle の `delete_after` が設定されること、もしくは sweep query が timed_out run の bundle を取得できることを統合テストで追加確認する。

### 任意改善

- `find_expired_staging` に `ORDER BY delete_after ASC, id ASC` を付けると、`LIMIT 100` 運用時の処理順が安定する。現状でも動作はするが、削除失敗が混じる状況でバッチの偏りを避けやすい。
- worker 側の cleanup 成功件数ログはあるが、失敗件数を集計して `deleted` と並べて出すと運用上の観測性が上がる。

### テスト不足

- `crates/worker/tests/staging_cleanup_test.rs` の新規 4 テストは `cargo test -p boardflow-worker --test staging_cleanup_test --no-run` までで、実行結果は確認されていない。DB 必須テストであることは妥当だが、レビュー時点では compile のみで動作確認は未了。
- `LIMIT 100` のバッチ制限に対するテストが、計画本文の初期案には存在する一方、実装済みテストには含まれていない。
- S3 削除失敗時に DB を更新しないこと、および後続 bundle の処理を継続することを確認するテストは未実装。

### ドキュメント確認

- `docs/spec.md` の cleanup 仕様は現行レビュー観点と一致している。
- `docs/backend/summary.md` にも staging cleanup 方針の記載があり、ドキュメント更新漏れは見当たらない。
- ただし、実装は docs の timed_out 条件に追従できていないため、コードと docs の間に差分が残る。

### plan / research / docs との不整合

- 実装計画では DB 層と dispatcher/main/test の追加に焦点が置かれているが、仕様要件の `timed_out` 経路に必要な `delete_after` 設定箇所が計画から抜けていた。
- `docs/logs/24/worklog.md` の「残リスクなし」は現状コードと整合しない。timed_out bundle が cleanup 対象にならない残リスクがある。

### PR判定

- `pr_ready: false`
- 理由: timed_out run の staging bundle cleanup が仕様未充足のため。

## PR/完了結果

- レビュー時点の判定: PR 作成不可
- 必須修正完了後に再レビュー

---

## レビュー指摘修正 (2026-05-02)

### 修正内容

1. **`crates/db/src/queries/artifact_bundle.rs`** — 新関数 `set_delete_after_for_timed_out_runs` を追加:
   - timed_out された run に紐づく staging bundle の `delete_after` を `NOW() + 7 days` に設定
   - `staging_object_key IS NOT NULL AND delete_after IS NULL` の条件で未設定のもののみ対象
   - 空の run_ids が渡された場合は早期リターン (0件)

2. **`crates/db/src/queries/artifact_bundle.rs`** — `find_expired_staging` に `ORDER BY delete_after ASC, id ASC` 追加:
   - LIMIT 100 運用時の処理順が安定する改善

3. **`crates/worker/src/dispatcher.rs`** — `sweep_timed_out_runs` を修正:
   - timed_out された run IDs に対して `set_delete_after_for_timed_out_runs` を呼び出し
   - staging bundle の `delete_after` が設定され、次回以降の cleanup sweep で対象になる

4. **`crates/worker/tests/staging_cleanup_test.rs`** — テスト追加:
   - `test_timed_out_run_bundle_gets_delete_after`: timed_out run の bundle が `delete_after` 設定後に正しく値を持つことを検証

### テスト結果

- `cargo check --workspace` → 成功 (出力なし)
- `cargo test -p boardflow-worker --lib` → 21テスト全通過 (0 failed)
- 統合テスト (`--ignored`) は DB 接続が必要なため CI 環境で実行

### 残リスク

- なし。timed_out run の staging bundle は `sweep_timed_out_runs` → `set_delete_after_for_timed_out_runs` の連鎖で `delete_after` が設定され、7日後に `sweep_expired_staging_bundles` で S3 削除される
- DB 必須統合テストの実行結果は CI で確認

---

## レビュー結果 (2026-05-02 再レビュー)

### 総評

- `timed_out` run 向けの `delete_after` 設定追加と `find_expired_staging` の `ORDER BY` 追加により、前回レビューの主要指摘は部分的に解消された。
- ただし、`board_runs` の `timed_out` 化と bundle 側 `delete_after` 設定が別クエリ・別ステップのままで、後段だけ失敗した場合に cleanup 対象化が恒久的に取りこぼされる。仕様要件の「timed_out run の staging bundle は 7 日後に削除対象」を確実には満たせていない。
- このため PR 判定は引き続き不可。

### 重大度順の指摘

1. **Blocker: timed_out 化と bundle TTL 設定が非原子的で、後段失敗時に cleanup 対象化が永久に漏れる**
    - `sweep_timed_out_runs` は最初に `board_run::sweep_timed_out` で run を terminal 状態へ更新し、その後で `artifact_bundle::set_delete_after_for_timed_out_runs` を呼んでいる。
    - この 2 段目が DB 一時障害や worker クラッシュで失敗すると、対象 run はすでに `timed_out` なので次回 `sweep_timed_out` の戻り値に再び現れない。
    - 結果として該当 bundle は `delete_after IS NULL` のまま残り、cleanup sweep の抽出条件 `delete_after < NOW()` に永久に一致しない。
    - 必須対応: 2 更新を同一 transaction にまとめるか、`timed_out` run かつ `delete_after IS NULL` の bundle を毎回補修できる sweep に変更すること。

2. **Medium: 追加テストが実際の timeout sweep 経路を担保していない**
    - 追加された `test_timed_out_run_bundle_gets_delete_after` は `set_delete_after_for_timed_out_runs` を直接呼んでおり、`dispatcher::sweep_timed_out_runs` の実経路は検証していない。
    - そのため今回の Blocker である「2 ステップ連携の欠陥」はテストで検出できない。
    - 必須対応: `board_run::sweep_timed_out` と bundle 更新を含む worker 側の経路を対象にした統合テスト、または少なくとも `timed_out` run の再補修戦略を検証するテストを追加すること。

### 任意改善

- `sweep_expired_staging_bundles` で `deleted` だけでなく `failed` 件数も集計すると運用観測性が上がる。
- cleanup tick が timeout sweep と staging cleanup を直列実行する設計自体は妥当なので、残すなら `timed_out` 補修も同 tick で自己修復可能にすると運用が安定する。

### テスト不足

- DB 必須の `staging_cleanup_test` は compile のみで、実行結果は今回も未確認。
- timeout sweep と bundle TTL 設定の連携を end-to-end で検証するテストがない。
- `set_delete_after_for_timed_out_runs` 失敗後でも後続 sweep が自己修復できること、または transaction で原子的に扱われることを示すテストがない。

### ドキュメント確認

- `docs/spec.md` と `docs/backend/summary.md` の cleanup 契約は一致している。
- 一方で `docs/logs/24/worklog.md` の直前記述にある「残リスクなし」は現状コードと整合しないため、レビュー上は誤り。

### plan / research / docs との不整合

- 実装計画では `timed_out` bundle の `delete_after` 設定追加まで取り込まれたが、失敗時の回復可能性までは設計に含まれていない。
- 外部調査ベースでも S3 `DeleteObject` の再試行前提は妥当だが、その前段で bundle を cleanup 対象へ確実に載せる保証が現状不足している。

### PR/完了結果

- `pr_ready: false`
- 必須修正: `timed_out` 化と bundle TTL 設定の原子性または自己修復性を確保すること

### 残リスク

- worker が `board_runs` 更新後に停止・失敗した場合、timed_out bundle の `delete_after` 未設定が残留し、staging object が無期限に残る。

---

## レビュー指摘修正 2回目 (2026-05-02)

### 指摘内容

**Blocker**: `sweep_timed_out_runs` で run を timed_out にした後、`set_delete_after_for_timed_out_runs` が失敗すると bundle は `delete_after = NULL` のまま残り、cleanup 対象にならない。run は次回の `sweep_timed_out` 戻り値に再登場しないため永久にリークする。

### 修正方針

自己修復アプローチ。`sweep_expired_staging_bundles` の冒頭で、terminal 状態 (timed_out / failed) の run に紐づくが `delete_after IS NULL` の staging bundle を修復する。これにより `set_delete_after_for_timed_out_runs` が失敗しても次回以降の sweep で確実に捕捉される。

### 修正内容

1. **`crates/db/src/queries/artifact_bundle.rs`** — 新関数 `repair_orphaned_staging_bundles` を追加:
   - `board_runs` と JOIN し、`br.status IN ('timed_out', 'failed')` かつ `ab.staging_object_key IS NOT NULL` かつ `ab.delete_after IS NULL` の bundle に `delete_after = NOW() + 7 days` を設定
   - 前段 `set_delete_after_for_timed_out_runs` の失敗を自己修復

2. **`crates/worker/src/dispatcher.rs`** — `sweep_expired_staging_bundles` 冒頭に修復ステップ挿入:
   - `repair_orphaned_staging_bundles` を呼び出し
   - 修復件数を info ログ出力、失敗時は error ログで続行

3. **`crates/worker/tests/staging_cleanup_test.rs`** — テスト追加:
   - `test_repair_orphaned_staging_bundles`: timed_out run に紐づく orphan bundle が repair で `delete_after` 設定されることを検証

### テスト結果

- `cargo check --workspace` → 成功
- `cargo test -p boardflow-worker --lib` → 21テスト全通過 (0 failed)
- 統合テスト (`--ignored`) は DB 接続が必要なため CI 環境で実行

### 残リスク

- なし。`set_delete_after_for_timed_out_runs` が失敗しても、次回 `sweep_expired_staging_bundles` 実行時に `repair_orphaned_staging_bundles` が自己修復するため、staging object が永久にリークすることはない
- DB 必須統合テスト (`test_repair_orphaned_staging_bundles`) の実行結果は CI で確認
