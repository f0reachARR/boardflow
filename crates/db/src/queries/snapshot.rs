use boardflow_domain::models::snapshot::BoardProjectSnapshot;
use uuid::Uuid;

pub async fn insert(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    board_project_id: Uuid,
    board_run_id: Uuid,
    tree_hash: &str,
    commit_sha: &str,
    file_hashes_json: &serde_json::Value,
) -> Result<BoardProjectSnapshot, sqlx::Error> {
    sqlx::query_as::<_, BoardProjectSnapshot>(
        r#"INSERT INTO board_project_snapshots (id, board_project_id, board_run_id, tree_hash, commit_sha, file_hashes_json, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, NOW())
        RETURNING *"#,
    )
    .bind(id)
    .bind(board_project_id)
    .bind(board_run_id)
    .bind(tree_hash)
    .bind(commit_sha)
    .bind(file_hashes_json)
    .fetch_one(executor)
    .await
}
