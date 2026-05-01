use boardflow_domain::models::github_job::GithubJob;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("job processing error: {0}")]
    Processing(String),
}

/// Dequeue a single pending job using CTE + FOR UPDATE SKIP LOCKED
pub async fn dequeue(pool: &PgPool, job_type: &str) -> Result<Option<GithubJob>, JobError> {
    let job = sqlx::query_as::<_, GithubJob>(
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
    .fetch_optional(pool)
    .await?;
    Ok(job)
}

/// Mark job as completed
pub async fn ack(pool: &PgPool, job_id: Uuid) -> Result<(), JobError> {
    sqlx::query("UPDATE github_jobs SET status = 'completed', updated_at = NOW() WHERE id = $1")
        .bind(job_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Mark job as failed with retry (exponential backoff: 10s * 3^attempts)
const MAX_ATTEMPTS: i32 = 5;
const BASE_BACKOFF_SECS: i64 = 10;

pub async fn nack(
    pool: &PgPool,
    job_id: Uuid,
    error_message: &str,
    attempts: i32,
) -> Result<(), JobError> {
    if attempts >= MAX_ATTEMPTS {
        sqlx::query(
            "UPDATE github_jobs SET status = 'failed', last_error = $2, updated_at = NOW() WHERE id = $1",
        )
        .bind(job_id)
        .bind(error_message)
        .execute(pool)
        .await?;
    } else {
        let backoff_secs = BASE_BACKOFF_SECS * 3_i64.pow(attempts as u32);
        sqlx::query(
            "UPDATE github_jobs SET status = 'pending', last_error = $2, run_after = NOW() + make_interval(secs => $3), updated_at = NOW() WHERE id = $1",
        )
        .bind(job_id)
        .bind(error_message)
        .bind(backoff_secs as f64)
        .execute(pool)
        .await?;
    }
    Ok(())
}

