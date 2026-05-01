use boardflow_domain::models::github_job::GithubJob;
use uuid::Uuid;

/// Enqueue an import job (idempotent via partial unique index)
pub async fn enqueue_import(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    installation_id: i64,
    repository_id: Uuid,
    board_project_id: Uuid,
    board_run_id: Uuid,
    payload: &serde_json::Value,
) -> Result<GithubJob, sqlx::Error> {
    sqlx::query_as::<_, GithubJob>(
        r#"INSERT INTO github_jobs (id, installation_id, repository_id, board_project_id, board_run_id, type, payload_json, status, attempts, run_after, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, 'artifact_bundle_import', $6, 'pending', 0, NOW(), NOW(), NOW())
        ON CONFLICT (board_run_id, type) WHERE board_run_id IS NOT NULL
        DO UPDATE SET updated_at = NOW()
        RETURNING *"#,
    )
    .bind(id)
    .bind(installation_id)
    .bind(repository_id)
    .bind(board_project_id)
    .bind(board_run_id)
    .bind(payload)
    .fetch_one(executor)
    .await
}

/// Dequeue a single pending job (CTE + FOR UPDATE SKIP LOCKED)
pub async fn dequeue(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    job_type: &str,
) -> Result<Option<GithubJob>, sqlx::Error> {
    sqlx::query_as::<_, GithubJob>(
        r#"WITH next_job AS (
            SELECT id FROM github_jobs
            WHERE type = $1 AND status = 'pending' AND run_after <= NOW()
            ORDER BY run_after, created_at
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE github_jobs SET status = 'running', attempts = attempts + 1, updated_at = NOW()
        FROM next_job
        WHERE github_jobs.id = next_job.id
        RETURNING github_jobs.*"#,
    )
    .bind(job_type)
    .fetch_optional(executor)
    .await
}

/// Mark job as completed
pub async fn mark_completed(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE github_jobs SET status = 'completed', updated_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await?;
    Ok(())
}

/// Mark job as failed (terminal)
pub async fn mark_failed(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    error_message: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE github_jobs SET status = 'failed', last_error = $2, updated_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .bind(error_message)
    .execute(executor)
    .await?;
    Ok(())
}

/// Reschedule job for retry with exponential backoff
pub async fn reschedule(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    error_message: &str,
    backoff_secs: f64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE github_jobs SET status = 'pending', last_error = $2, run_after = NOW() + make_interval(secs => $3), updated_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .bind(error_message)
    .bind(backoff_secs)
    .execute(executor)
    .await?;
    Ok(())
}

/// Enqueue a generic job (idempotent via ON CONFLICT DO NOTHING)
pub async fn enqueue(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    installation_id: i64,
    repository_id: Uuid,
    board_project_id: Option<Uuid>,
    board_run_id: Option<Uuid>,
    job_type: &str,
    payload: &serde_json::Value,
) -> Result<GithubJob, sqlx::Error> {
    sqlx::query_as::<_, GithubJob>(
        r#"INSERT INTO github_jobs (id, installation_id, repository_id, board_project_id, board_run_id, type, payload_json, status, attempts, run_after, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', 0, NOW(), NOW(), NOW())
        ON CONFLICT DO NOTHING
        RETURNING *"#,
    )
    .bind(id)
    .bind(installation_id)
    .bind(repository_id)
    .bind(board_project_id)
    .bind(board_run_id)
    .bind(job_type)
    .bind(payload)
    .fetch_one(executor)
    .await
}
