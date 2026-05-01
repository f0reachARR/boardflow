use boardflow_domain::models::run_check::RunCheck;
use uuid::Uuid;

pub async fn list_by_board_run(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    board_run_id: Uuid,
) -> Result<Vec<RunCheck>, sqlx::Error> {
    sqlx::query_as::<_, RunCheck>(
        "SELECT * FROM run_checks WHERE board_run_id = $1 ORDER BY check_kind, created_at",
    )
    .bind(board_run_id)
    .fetch_all(executor)
    .await
}

pub async fn insert(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    board_run_id: Uuid,
    check_kind: &str,
    status: &str,
    error_count: i32,
    warning_count: i32,
    notice_count: i32,
    tool_name: Option<&str>,
    tool_version: Option<&str>,
    raw_summary_json: Option<&serde_json::Value>,
) -> Result<RunCheck, sqlx::Error> {
    sqlx::query_as::<_, RunCheck>(
        r#"INSERT INTO run_checks (id, board_run_id, check_kind, status, error_count, warning_count, notice_count, tool_name, tool_version, raw_summary_json, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
        RETURNING *"#,
    )
    .bind(id)
    .bind(board_run_id)
    .bind(check_kind)
    .bind(status)
    .bind(error_count)
    .bind(warning_count)
    .bind(notice_count)
    .bind(tool_name)
    .bind(tool_version)
    .bind(raw_summary_json)
    .fetch_one(executor)
    .await
}
