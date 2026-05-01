# PostgreSQL Job Queue Enqueue パターン (Rust + SQLx)

対象Issue: #5

## 要約

Artifact Bundle Import APIで、import jobをPostgreSQLキュー (`github_jobs` テーブル) にenqueueする必要がある。SQLx 0.8 の `query` / `query_as` で INSERT を行い、`ON CONFLICT` で冪等性を担保するパターンを調査した。既存の `github_jobs` テーブルスキーマに合わせた具体的な実装方針を整理する。

## 確認した情報

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

### 冪等性のための UNIQUE 制約追加

現状のスキーマには `board_run_id + type` の UNIQUE 制約がない。import jobの重複防止には、以下のいずれかが必要:

**方法A: UNIQUE INDEX を追加** (推奨)
```sql
CREATE UNIQUE INDEX idx_github_jobs_board_run_id_type
ON github_jobs (board_run_id, type)
WHERE board_run_id IS NOT NULL;
```

部分インデックスを使うことで `board_run_id IS NULL` のジョブ (repository レベルのジョブ) には影響しない。

**方法B: アプリケーション層で SELECT + INSERT**
```sql
SELECT id FROM github_jobs
WHERE board_run_id = $1 AND type = $2
LIMIT 1;
-- 存在しなければ INSERT
```

推奨はA。DB制約でrace conditionを防止できる。

### Enqueue パターン (INSERT ... ON CONFLICT)

```rust
use sqlx::PgPool;
use uuid::Uuid;
use chrono::{DateTime, Utc};

pub async fn enqueue_import_job(
    pool: &PgPool,
    id: Uuid,
    installation_id: i64,
    repository_id: Uuid,
    board_project_id: Uuid,
    board_run_id: Uuid,
    payload: &serde_json::Value,
    now: DateTime<Utc>,
) -> Result<Uuid, sqlx::Error> {
    let row = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO github_jobs (
            id, installation_id, repository_id, board_project_id,
            board_run_id, type, payload_json, status,
            attempts, run_after, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, 'artifact_bundle_import', $6, 'pending', 0, $7, $7, $7)
        ON CONFLICT (board_run_id, type) WHERE board_run_id IS NOT NULL
        DO UPDATE SET updated_at = $7
        RETURNING id
        "#,
    )
    .bind(id)
    .bind(installation_id)
    .bind(repository_id)
    .bind(board_project_id)
    .bind(board_run_id)
    .bind(payload)
    .bind(now)
    .fetch_one(pool)
    .await?;

    Ok(row)
}
```

### Job Type 設計

| type | 用途 | trigger |
|---|---|---|
| `artifact_bundle_import` | staging zip → 検証 → final bucket保存 → DB保存 | Import API |
| `create_issue` | BoardProject の GitHub Issue 作成 | import 完了後 (worker) |
| `update_dashboard_comment` | Dashboard コメント更新 | import 完了後 (worker) |
| `create_run_result_comment` | Run Result コメント作成 | import 完了後 (worker) |

### payload_json 設計 (artifact_bundle_import)

```json
{
  "bundle_id": "ab_abc123",
  "staging_object_key": "staging/runs/br_abc123/bundle.zip",
  "bundle_sha256": "sha256:...",
  "bundle_size_bytes": 12345678
}
```

### トランザクション内でのenqueue

Import API では BoardRun のステータス更新と job enqueue を同一トランザクションで行う:

```rust
use sqlx::PgPool;

pub async fn process_import_request(
    pool: &PgPool,
    board_run_id: Uuid,
    // ... other params
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    // 1. BoardRun status を 'importing' に更新
    sqlx::query(
        "UPDATE board_runs SET status = 'importing' WHERE id = $1 AND status IN ('created', 'uploading')"
    )
    .bind(board_run_id)
    .execute(&mut *tx)
    .await?;

    // 2. artifact_bundles レコード作成
    // ...

    // 3. github_jobs に import job を enqueue
    sqlx::query(
        r#"
        INSERT INTO github_jobs (
            id, installation_id, repository_id, board_project_id,
            board_run_id, type, payload_json, status,
            attempts, run_after, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, 'artifact_bundle_import', $6, 'pending', 0, NOW(), NOW(), NOW())
        ON CONFLICT (board_run_id, type) WHERE board_run_id IS NOT NULL
        DO NOTHING
        "#,
    )
    // ... bind params
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}
```

### Worker の pull パターン (FOR UPDATE SKIP LOCKED)

既存の `github_jobs` テーブルに対する worker pull パターン:

```rust
pub async fn pull_jobs(
    pool: &PgPool,
    limit: i32,
) -> Result<Vec<GithubJob>, sqlx::Error> {
    sqlx::query_as::<_, GithubJob>(
        r#"
        UPDATE github_jobs
        SET status = 'running', updated_at = NOW(), attempts = attempts + 1
        WHERE id IN (
            SELECT id
            FROM github_jobs
            WHERE status = 'pending' AND run_after <= NOW()
            ORDER BY run_after ASC
            FOR UPDATE SKIP LOCKED
            LIMIT $1
        )
        RETURNING *
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
}
```

`FOR UPDATE SKIP LOCKED` により、複数 worker が同時にポーリングしても同じジョブを重複取得しない。

## BoardFlow への示唆

- マイグレーションで `idx_github_jobs_board_run_id_type` 部分ユニークインデックスを追加する
- `crates/jobs/src/lib.rs` に enqueue ユーティリティ関数を実装する
- Import API handler は transaction 内で BoardRun ステータス変更 + bundle 作成 + job enqueue を行う
- `ON CONFLICT DO NOTHING` (冪等 insert) と `ON CONFLICT DO UPDATE` (upsert) を用途で使い分ける:
  - Import API の冪等性: 同一 `board_run_id + type` の再送では既存 job を返す → `DO UPDATE SET updated_at` + `RETURNING id`
  - Worker 側の job enqueue (後続ジョブ生成): 重複しても問題ないなら `DO NOTHING`

## 採用/不採用判断

**採用**: 既存の `github_jobs` テーブルを活用し、部分ユニークインデックス追加 + `ON CONFLICT` パターンで冪等enqueueを実装する。

## 制約とpitfall

- `ON CONFLICT` で部分インデックスを対象にする場合、`WHERE` 句がインデックス定義と完全一致する必要がある
- `board_run_id IS NULL` のジョブ (repository レベル) にはこの制約が適用されないため、別途 `board_project_id + type` や他のキーで冪等性を担保する必要がある
- `FOR UPDATE SKIP LOCKED` は PostgreSQL 9.5+ で利用可能 (既に前提)
- transaction が長引くと他の INSERT がブロックされるため、enqueue は軽量に保つ
- `run_after` を現在時刻にするとすぐ実行対象になる。遅延実行が必要な場合は未来時刻を設定

## 未解決の疑問

- `github_jobs` テーブルの最大 attempts 数 (5回? 設定可能?) — worker 実装時に決定
- failed job の再実行タイミング (exponential backoff の具体値) — worker 実装時に決定
- `board_run_id` が NULL のジョブ種別に対する冪等キーの設計 — Issue #5 の scope 外

## 参照URL

- https://kerkour.com/rust-job-queue-with-postgresql
- https://cetra3.github.io/blog/implementing-a-jobq-sqlx/
- https://www.postgresql.org/docs/current/sql-select.html#SQL-FOR-UPDATE-SHARE
- https://www.prisma.io/dataguide/postgresql/inserting-and-modifying-data/insert-on-conflict
