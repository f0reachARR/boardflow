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
