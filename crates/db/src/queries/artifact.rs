use boardflow_domain::models::artifact::{Artifact, ArtifactType};
use uuid::Uuid;

pub async fn find_by_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<Option<Artifact>, sqlx::Error> {
    sqlx::query_as::<_, Artifact>("SELECT * FROM artifacts WHERE id = $1")
        .bind(id)
        .fetch_optional(executor)
        .await
}

pub async fn list_by_board_run(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    board_run_id: Uuid,
) -> Result<Vec<Artifact>, sqlx::Error> {
    sqlx::query_as::<_, Artifact>(
        "SELECT * FROM artifacts WHERE board_run_id = $1 ORDER BY type, created_at",
    )
    .bind(board_run_id)
    .fetch_all(executor)
    .await
}

pub async fn insert(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    board_run_id: Uuid,
    artifact_type: ArtifactType,
    status: &str,
    filename: Option<&str>,
    source_path: Option<&str>,
    logical_name: Option<&str>,
    content_type: Option<&str>,
    storage_key: Option<&str>,
    sha256: Option<&str>,
    size_bytes: Option<i64>,
    status_reason: Option<&str>,
    source_bundle_id: Option<Uuid>,
) -> Result<Artifact, sqlx::Error> {
    sqlx::query_as::<_, Artifact>(
        r#"INSERT INTO artifacts (id, board_run_id, type, status, filename, source_path, logical_name, content_type, storage_key, sha256, size_bytes, status_reason, source_bundle_id, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, NOW())
        RETURNING *"#,
    )
    .bind(id)
    .bind(board_run_id)
    .bind(artifact_type)
    .bind(status)
    .bind(filename)
    .bind(source_path)
    .bind(logical_name)
    .bind(content_type)
    .bind(storage_key)
    .bind(sha256)
    .bind(size_bytes)
    .bind(status_reason)
    .bind(source_bundle_id)
    .fetch_one(executor)
    .await
}
