use boardflow_domain::models::board_run::BoardRun;
use chrono::{DateTime, Utc};
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

/// Update BoardRun status to 'completed' with check summaries
pub async fn mark_completed(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    erc_status: Option<&str>,
    erc_errors: i32,
    erc_warnings: i32,
    drc_status: Option<&str>,
    drc_errors: i32,
    drc_warnings: i32,
) -> Result<BoardRun, sqlx::Error> {
    sqlx::query_as::<_, BoardRun>(
        r#"UPDATE board_runs SET status = 'completed', erc_status = $2, erc_errors = $3, erc_warnings = $4,
        drc_status = $5, drc_errors = $6, drc_warnings = $7, completed_at = NOW()
        WHERE id = $1 RETURNING *"#,
    )
    .bind(id)
    .bind(erc_status)
    .bind(erc_errors)
    .bind(erc_warnings)
    .bind(drc_status)
    .bind(drc_errors)
    .bind(drc_warnings)
    .fetch_one(executor)
    .await
}

/// List BoardRuns by board_project_id with cursor pagination
pub async fn list_by_board_project(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    board_project_id: Uuid,
    limit: i64,
    cursor: Option<(DateTime<Utc>, Uuid)>,
) -> Result<Vec<BoardRun>, sqlx::Error> {
    match cursor {
        Some((ts, id)) => {
            sqlx::query_as::<_, BoardRun>(
                r#"SELECT * FROM board_runs
                WHERE board_project_id = $1 AND (created_at, id) < ($2, $3)
                ORDER BY created_at DESC, id DESC
                LIMIT $4"#,
            )
            .bind(board_project_id)
            .bind(ts)
            .bind(id)
            .bind(limit)
            .fetch_all(executor)
            .await
        }
        None => {
            sqlx::query_as::<_, BoardRun>(
                r#"SELECT * FROM board_runs
                WHERE board_project_id = $1
                ORDER BY created_at DESC, id DESC
                LIMIT $2"#,
            )
            .bind(board_project_id)
            .bind(limit)
            .fetch_all(executor)
            .await
        }
    }
}

/// Find the repository associated with a board_run (via board_project)
pub async fn find_repository_by_board_run_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    board_run_id: Uuid,
) -> Result<Option<boardflow_domain::models::repository::Repository>, sqlx::Error> {
    sqlx::query_as::<_, boardflow_domain::models::repository::Repository>(
        r#"SELECT r.* FROM repositories r
        JOIN board_projects bp ON bp.repository_id = r.id
        JOIN board_runs br ON br.board_project_id = bp.id
        WHERE br.id = $1"#,
    )
    .bind(board_run_id)
    .fetch_optional(executor)
    .await
}

/// Find the previous completed run for a board_project (excluding the current run)
pub async fn find_previous_completed(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    board_project_id: Uuid,
    current_run_id: Uuid,
) -> Result<Option<BoardRun>, sqlx::Error> {
    sqlx::query_as::<_, BoardRun>(
        r#"SELECT * FROM board_runs
        WHERE board_project_id = $1 AND id != $2 AND status = 'completed'
        ORDER BY completed_at DESC
        LIMIT 1"#,
    )
    .bind(board_project_id)
    .bind(current_run_id)
    .fetch_optional(executor)
    .await
}

/// Sweep BoardRuns that have been in non-terminal status for more than 12 hours.
/// Returns the IDs of the runs that were marked as timed out.
pub async fn sweep_timed_out(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query_scalar::<_, Uuid>(
        r#"UPDATE board_runs
        SET status = 'timed_out', timed_out_at = NOW()
        WHERE status IN ('created', 'uploading', 'importing')
        AND created_at < NOW() - interval '12 hours'
        RETURNING id"#,
    )
    .fetch_all(executor)
    .await?;
    Ok(rows)
}
