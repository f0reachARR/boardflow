use boardflow_domain::models::snapshot::{BoardRunDiff, BoardRunDiffMetadata};
use uuid::Uuid;

pub async fn insert_diff_metadata(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    board_run_id: Uuid,
    file_hashes_json: Option<&serde_json::Value>,
    bom_summary_json: Option<&serde_json::Value>,
    checks_summary_json: Option<&serde_json::Value>,
    artifacts_summary_json: Option<&serde_json::Value>,
    previews_json: Option<&serde_json::Value>,
) -> Result<BoardRunDiffMetadata, sqlx::Error> {
    sqlx::query_as::<_, BoardRunDiffMetadata>(
        r#"INSERT INTO board_run_diff_metadata (id, board_run_id, file_hashes_json, bom_summary_json, checks_summary_json, artifacts_summary_json, previews_json, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
        RETURNING *"#,
    )
    .bind(id)
    .bind(board_run_id)
    .bind(file_hashes_json)
    .bind(bom_summary_json)
    .bind(checks_summary_json)
    .bind(artifacts_summary_json)
    .bind(previews_json)
    .fetch_one(executor)
    .await
}

pub async fn insert_diff(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    board_run_id: Uuid,
    base_board_run_id: Option<Uuid>,
    status: &str,
    summary_json: Option<&serde_json::Value>,
) -> Result<BoardRunDiff, sqlx::Error> {
    sqlx::query_as::<_, BoardRunDiff>(
        r#"INSERT INTO board_run_diffs (id, board_run_id, base_board_run_id, status, summary_json, created_at)
        VALUES ($1, $2, $3, $4, $5, NOW())
        RETURNING *"#,
    )
    .bind(id)
    .bind(board_run_id)
    .bind(base_board_run_id)
    .bind(status)
    .bind(summary_json)
    .fetch_one(executor)
    .await
}
