use boardflow_domain::models::board_run::BoardRun;
use uuid::Uuid;

/// Find a BoardRun by its ID
pub async fn find_by_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<Option<BoardRun>, sqlx::Error> {
    sqlx::query_as::<_, BoardRun>("SELECT * FROM board_runs WHERE id = $1")
        .bind(id)
        .fetch_optional(executor)
        .await
}

/// Find a BoardRun by its ID with FOR UPDATE lock
pub async fn find_by_id_for_update(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<Option<BoardRun>, sqlx::Error> {
    sqlx::query_as::<_, BoardRun>("SELECT * FROM board_runs WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(executor)
        .await
}

/// Find a BoardRun by idempotency key
pub async fn find_by_idempotency_key(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    board_project_id: Uuid,
    github_run_id: i64,
    github_run_attempt: i32,
) -> Result<Option<BoardRun>, sqlx::Error> {
    sqlx::query_as::<_, BoardRun>(
        "SELECT * FROM board_runs WHERE board_project_id = $1 AND github_run_id = $2 AND github_run_attempt = $3",
    )
    .bind(board_project_id)
    .bind(github_run_id)
    .bind(github_run_attempt)
    .fetch_optional(executor)
    .await
}

/// Insert a new BoardRun
pub async fn insert(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    board_project_id: Uuid,
    commit_sha: &str,
    branch: &str,
    ref_: &str,
    github_run_id: i64,
    github_run_attempt: i32,
    tree_hash: &str,
) -> Result<BoardRun, sqlx::Error> {
    sqlx::query_as::<_, BoardRun>(
        r#"INSERT INTO board_runs (id, board_project_id, commit_sha, branch, ref, github_run_id, github_run_attempt, tree_hash, status, erc_errors, erc_warnings, drc_errors, drc_warnings, review_status, diff_status, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'created', 0, 0, 0, 0, 'pending', 'pending', NOW())
        RETURNING *"#,
    )
    .bind(id)
    .bind(board_project_id)
    .bind(commit_sha)
    .bind(branch)
    .bind(ref_)
    .bind(github_run_id)
    .bind(github_run_attempt)
    .bind(tree_hash)
    .fetch_one(executor)
    .await
}

/// Update BoardRun status to 'failed'
pub async fn mark_failed(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<BoardRun, sqlx::Error> {
    sqlx::query_as::<_, BoardRun>(
        "UPDATE board_runs SET status = 'failed', completed_at = NOW() WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .fetch_one(executor)
    .await
}

/// Update BoardRun status to 'importing'
pub async fn mark_importing(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<BoardRun, sqlx::Error> {
    sqlx::query_as::<_, BoardRun>(
        "UPDATE board_runs SET status = 'importing' WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .fetch_one(executor)
    .await
}
