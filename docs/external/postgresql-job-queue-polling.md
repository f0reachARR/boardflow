# PostgreSQL ジョブキューポーリング (Rust + SQLx)

対象Issue: #7, #26

## 要約

Worker は `github_jobs` テーブルから複数のジョブタイプを優先度順にポーリングし処理する。`SELECT ... FOR UPDATE SKIP LOCKED` パターンがデファクトスタンダードで、複数 worker インスタンスでの安全な並行処理が可能。既存の `github_jobs` スキーマに `run_after` と `attempts` カラムが存在するため、リトライとバックオフにそのまま利用できる。ポーリング間隔そのものは実装側で調整可能で、現行 worker は `POLL_INTERVAL_SECS` で上書きできる。

ポーリング対象ジョブタイプ（優先度順）:
1. `artifact_bundle_import` — staging zip の検証・展開・保存
2. `create_issue` — BoardProject の GitHub Issue 作成
3. `create_dashboard_comment` — Dashboard コメント新規作成
4. `update_dashboard_comment` — Dashboard コメント更新
5. `create_run_result_comment` — Run Result コメント作成

GitHub App 未設定時（`GITHUB_APP_ID` / `GITHUB_PRIVATE_KEY_PEM` なし）は GitHub API ジョブ（2〜5）をスキップし、`artifact_bundle_import` のみ処理する。

## 確認した情報

### 使用クレート

| クレート | バージョン | 備考 |
|---|---|---|
| `sqlx` | `"0.8"` + PG features | workspace Cargo.toml に追加済み |
| `tokio` | `"1"` + full features | workspace Cargo.toml に追加済み |

### 既存スキーマ (github_jobs)

```sql
CREATE TABLE github_jobs (
    id UUID PRIMARY KEY,
    installation_id BIGINT NOT NULL,
    repository_id UUID NOT NULL REFERENCES repositories(id),
    board_project_id UUID REFERENCES board_projects(id),
    board_run_id UUID REFERENCES board_runs(id),
    type TEXT NOT NULL,
    payload_json JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    attempts INTEGER NOT NULL DEFAULT 0,
    run_after TIMESTAMPTZ NOT NULL,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
```

### dequeue パターン: CTE + FOR UPDATE SKIP LOCKED

```rust
use sqlx::PgPool;
use boardflow_domain::models::github_job::GithubJob;

const MAX_ATTEMPTS: i32 = 5;

/// ジョブを1件取得し、status を running に遷移する（アトミック）
pub async fn dequeue_job(
    pool: &PgPool,
    job_type: &str,
) -> Result<Option<GithubJob>, sqlx::Error> {
    sqlx::query_as::<_, GithubJob>(
        r#"
        WITH next_job AS (
            SELECT id
            FROM github_jobs
            WHERE type = $1
              AND status = 'pending'
              AND run_after <= NOW()
              AND attempts < $2
            ORDER BY run_after ASC, created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE github_jobs
        SET status = 'running',
            attempts = attempts + 1,
            updated_at = NOW()
        FROM next_job
        WHERE github_jobs.id = next_job.id
        RETURNING github_jobs.*
        "#,
    )
    .bind(job_type)
    .bind(MAX_ATTEMPTS)
    .fetch_optional(pool)
    .await
}
```

**CTE の重要性**: `WITH next_job AS (SELECT ... FOR UPDATE SKIP LOCKED)` + `UPDATE ... FROM next_job` は単一のアトミックステートメント。トランザクションを明示的に開く必要がない。CTE 内の SELECT で行ロックを取得し、同じステートメント内で UPDATE するため、race condition が発生しない。

### ジョブ完了 (ack)

```rust
pub async fn complete_job(
    pool: &PgPool,
    job_id: uuid::Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE github_jobs
        SET status = 'completed',
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}
```

### ジョブ失敗 (nack) + リトライ

```rust
pub async fn fail_job(
    pool: &PgPool,
    job_id: uuid::Uuid,
    error_message: &str,
    retry_delay_secs: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE github_jobs
        SET status = 'pending',
            last_error = $2,
            run_after = NOW() + ($3 || ' seconds')::INTERVAL,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .bind(error_message)
    .bind(retry_delay_secs.to_string())
    .execute(pool)
    .await?;
    Ok(())
}
```

status を `pending` に戻し `run_after` を未来に設定することで、バックオフ付きリトライが実現される。
`attempts` カラムは dequeue 時にインクリメント済みなので、MAX_ATTEMPTS に達したジョブは dequeue されなくなる。

### バックオフ戦略

指数バックオフ:

```rust
fn retry_delay_secs(attempts: i32) -> i64 {
    // 指数バックオフ: 10s, 30s, 90s, 270s, 810s
    let base: i64 = 10;
    let multiplier: i64 = 3;
    base * multiplier.pow((attempts - 1).max(0) as u32)
}
```

### 最大リトライ超過時の処理

```rust
pub async fn mark_permanently_failed(
    pool: &PgPool,
    job_id: uuid::Uuid,
    error_message: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE github_jobs
        SET status = 'failed',
            last_error = $2,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .bind(error_message)
    .execute(pool)
    .await?;
    Ok(())
}
```

### ポーリングループ

```rust
use tokio::time::{sleep, Duration};
use tokio::signal;

pub async fn run_worker(pool: PgPool) {
    let poll_interval = Duration::from_secs(5);

    loop {
        tokio::select! {
            _ = signal::ctrl_c() => {
                tracing::info!("Received shutdown signal, stopping worker");
                break;
            }
            result = dequeue_job(&pool, "artifact_bundle_import") => {
                match result {
                    Ok(Some(job)) => {
                        tracing::info!(job_id = %job.id, "Processing job");
                        if let Err(err) = process_job(&pool, &job).await {
                            tracing::error!(
                                job_id = %job.id,
                                error = %err,
                                "Job failed"
                            );
                            let delay = retry_delay_secs(job.attempts);
                            if job.attempts >= MAX_ATTEMPTS {
                                let _ = mark_permanently_failed(
                                    &pool, job.id, &err.to_string()
                                ).await;
                            } else {
                                let _ = fail_job(
                                    &pool, job.id, &err.to_string(), delay
                                ).await;
                            }
                        } else {
                            let _ = complete_job(&pool, job.id).await;
                        }
                        continue; // ジョブがあった場合は即座に次をチェック
                    }
                    Ok(None) => {
                        // キューが空 — poll_interval 待機
                    }
                    Err(err) => {
                        tracing::error!(error = %err, "Failed to dequeue job");
                    }
                }
                sleep(poll_interval).await;
            }
        }
    }
}
```

### Graceful Shutdown

`tokio::select!` + `tokio::signal::ctrl_c()` (または `unix::signal(SignalKind::terminate())`) を使用:

1. **SIGTERM/SIGINT 受信**: 新しいジョブの取得を停止
2. **処理中ジョブの完了を待つ**: 現在処理中のジョブは最後まで実行
3. **タイムアウト**: 一定時間内に完了しない場合は強制終了

```rust
use tokio::time::timeout;

pub async fn run_worker_with_graceful_shutdown(pool: PgPool) {
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

    // シグナルハンドラ
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("failed to listen for ctrl-c");
        tracing::info!("Shutdown signal received");
        let _ = shutdown_tx.send(true);
    });

    let poll_interval = Duration::from_secs(5);

    loop {
        if *shutdown_rx.borrow() {
            tracing::info!("Worker shutting down gracefully");
            break;
        }

        match dequeue_job(&pool, "artifact_bundle_import").await {
            Ok(Some(job)) => {
                // 処理中はシャットダウンを待たない（現在のジョブは完了させる）
                process_and_update(&pool, job).await;
                continue;
            }
            Ok(None) => {}
            Err(err) => {
                tracing::error!(error = %err, "Failed to dequeue");
            }
        }

        // poll_interval 中にシャットダウンされたら即座に終了
        tokio::select! {
            _ = sleep(poll_interval) => {}
            _ = shutdown_rx.changed() => {
                tracing::info!("Shutdown during poll wait");
                break;
            }
        }
    }
}
```

### visibility timeout パターン（代替案）

現在の `github_jobs` スキーマには `run_after` カラムがあるが、visibility timeout 専用の `visible_at` カラムはない。代替として:

- **現行方式** (status ベース): `pending` → `running` → `completed/failed`。Worker クラッシュ時は `running` のまま残る。
- **visibility timeout 方式**: `run_after` を「visibility timeout」として再利用。dequeue 時に `run_after = NOW() + timeout` を設定し、WHERE 条件で `status IN ('pending', 'running') AND run_after <= NOW()` とすることで、クラッシュしたジョブを自動回収可能。

**推奨**: MVP では現行の status ベース方式を使用。ただし、Worker クラッシュ回復のために定期的なスタック検出（`status = 'running' AND updated_at < NOW() - INTERVAL '10 minutes'` を `pending` にリセット）を別途実装するか、`run_after` を visibility timeout として再利用する。

### スタックジョブ回復クエリ

```sql
-- 10分以上 running のまま放置されたジョブを pending に戻す
UPDATE github_jobs
SET status = 'pending',
    run_after = NOW(),
    last_error = 'recovered from stuck running state',
    updated_at = NOW()
WHERE status = 'running'
  AND updated_at < NOW() - INTERVAL '10 minutes';
```

### インデックス推奨

dequeue クエリのパフォーマンスを確保するため:

```sql
CREATE INDEX idx_github_jobs_dequeue
ON github_jobs (type, status, run_after, created_at)
WHERE status = 'pending';
```

部分インデックスにすることで、completed/failed ジョブを除外し、インデックスサイズを抑える。

## BoardFlow への示唆

- `crates/db/src/queries/github_job.rs` に `dequeue_import_job`, `complete_job`, `fail_job` 関数を追加
- `crates/jobs/` にポーリングループとジョブディスパッチロジックを実装
- `crates/worker/src/main.rs` にエントリポイント（DB接続、S3クライアント初期化、ポーリングループ起動、graceful shutdown）
- 部分インデックスの追加マイグレーションが推奨
- `run_after` を visibility timeout として活用すれば、Worker クラッシュ時の自動回復が追加コードなしで実現可能

## 採用/不採用判断

**採用**: CTE + `FOR UPDATE SKIP LOCKED` パターンを採用。SQLx の `query_as` + `fetch_optional` で実装。

## 制約とpitfall

- `FOR UPDATE SKIP LOCKED` はトランザクション終了時にロック解放。CTE 内で使用する場合はステートメント完了でロック解放される
- ポーリング間隔が短すぎると DB 負荷が増加。5秒がバランス良い（LISTEN/NOTIFY でさらに最適化可能だが MVP では不要）
- `attempts` の MAX は dequeue の WHERE 条件で制御。超過したジョブは永久に dequeue されなくなるため、監視・アラートが必要
- トランザクション内で長時間処理を行うと他のクエリに影響。dequeue は即座にコミット（CTE 方式なら自動）し、処理は別途実行する設計が重要
- 同一ジョブの同時処理を防ぐため、`SKIP LOCKED` が不可欠。`FOR UPDATE` のみだとブロッキングが発生する

## 未解決の疑問

- Worker クラッシュ回復のタイミング（定期バッチ vs `run_after` visibility timeout 方式）— 実装時に決定
- 複数 worker インスタンスの同時実行数（MVP では 1 で十分か）
- LISTEN/NOTIFY による即時通知の導入タイミング（MVP 後で良いか）

## 参照URL

- https://www.postgresql.org/docs/current/sql-select.html#SQL-FOR-UPDATE-SHARE
- https://kerkour.com/rust-postgres-everything
- https://aminediro.com/posts/pg_job_queue/
- https://www.inferable.ai/blog/posts/postgres-skip-locked
