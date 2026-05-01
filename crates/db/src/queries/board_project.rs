use boardflow_domain::models::board_project::BoardProject;
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BoardProjectWithRepository {
    // BoardProject fields
    pub id: Uuid,
    pub repository_id: Uuid,
    pub project_path: String,
    pub project_dir: String,
    pub display_name: String,
    pub issue_number: Option<i32>,
    pub issue_node_id: Option<String>,
    pub issue_url: Option<String>,
    pub issue_sync_status: boardflow_domain::models::board_project::IssueSyncStatus,
    pub dashboard_comment_id: Option<i64>,
    pub recreate_issue_on_update: bool,
    pub latest_tree_hash: Option<String>,
    pub latest_completed_run_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // Repository fields (prefixed)
    pub repo_id: Uuid,
    pub github_repository_id: i64,
    pub repo_owner: String,
    pub repo_name: String,
    pub repo_installation_id: i64,
}

pub async fn find_by_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<Option<BoardProject>, sqlx::Error> {
    sqlx::query_as::<_, BoardProject>("SELECT * FROM board_projects WHERE id = $1")
        .bind(id)
        .fetch_optional(executor)
        .await
}

pub async fn find_by_id_with_repository(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<Option<BoardProjectWithRepository>, sqlx::Error> {
    sqlx::query_as::<_, BoardProjectWithRepository>(
        r#"SELECT
            bp.id, bp.repository_id, bp.project_path, bp.project_dir, bp.display_name,
            bp.issue_number, bp.issue_node_id, bp.issue_url, bp.issue_sync_status,
            bp.dashboard_comment_id, bp.recreate_issue_on_update, bp.latest_tree_hash,
            bp.latest_completed_run_id, bp.created_at, bp.updated_at,
            r.id AS repo_id, r.github_repository_id, r.owner AS repo_owner,
            r.name AS repo_name, r.installation_id AS repo_installation_id
        FROM board_projects bp
        JOIN repositories r ON r.id = bp.repository_id
        WHERE bp.id = $1"#,
    )
    .bind(id)
    .fetch_optional(executor)
    .await
}

pub async fn list_by_repository_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    repository_id: Uuid,
    limit: i64,
    cursor: Option<(DateTime<Utc>, Uuid)>,
) -> Result<Vec<BoardProject>, sqlx::Error> {
    match cursor {
        Some((ts, id)) => {
            sqlx::query_as::<_, BoardProject>(
                r#"SELECT * FROM board_projects
                WHERE repository_id = $1 AND (updated_at, id) < ($2, $3)
                ORDER BY updated_at DESC, id DESC
                LIMIT $4"#,
            )
            .bind(repository_id)
            .bind(ts)
            .bind(id)
            .bind(limit)
            .fetch_all(executor)
            .await
        }
        None => {
            sqlx::query_as::<_, BoardProject>(
                r#"SELECT * FROM board_projects
                WHERE repository_id = $1
                ORDER BY updated_at DESC, id DESC
                LIMIT $2"#,
            )
            .bind(repository_id)
            .bind(limit)
            .fetch_all(executor)
            .await
        }
    }
}

pub async fn upsert(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    repository_id: Uuid,
    project_path: &str,
    project_dir: &str,
    display_name: &str,
) -> Result<BoardProject, sqlx::Error> {
    let id = Uuid::now_v7();
    sqlx::query_as::<_, BoardProject>(
        "INSERT INTO board_projects (id, repository_id, project_path, project_dir, display_name, issue_sync_status, recreate_issue_on_update, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, 'pending', true, NOW(), NOW()) \
         ON CONFLICT (repository_id, project_path) DO UPDATE SET \
         project_dir = EXCLUDED.project_dir, display_name = EXCLUDED.display_name, updated_at = NOW() \
         RETURNING *",
    )
    .bind(id)
    .bind(repository_id)
    .bind(project_path)
    .bind(project_dir)
    .bind(display_name)
    .fetch_one(executor)
    .await
}

/// Update latest_completed_run_id and latest_tree_hash
pub async fn update_latest_completed_run(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    board_run_id: Uuid,
    tree_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE board_projects SET latest_completed_run_id = $2, latest_tree_hash = $3, updated_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .bind(board_run_id)
    .bind(tree_hash)
    .execute(executor)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BoardProjectWithLatestRunStatus {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub project_path: String,
    pub project_dir: String,
    pub display_name: String,
    pub issue_url: Option<String>,
    pub latest_tree_hash: Option<String>,
    pub latest_completed_run_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub latest_run_status: Option<String>,
}

pub async fn list_by_repository_id_with_status(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    repository_id: Uuid,
    limit: i64,
    cursor: Option<(DateTime<Utc>, Uuid)>,
) -> Result<Vec<BoardProjectWithLatestRunStatus>, sqlx::Error> {
    match cursor {
        Some((ts, id)) => {
            sqlx::query_as::<_, BoardProjectWithLatestRunStatus>(
                r#"SELECT bp.id, bp.repository_id, bp.project_path, bp.project_dir, bp.display_name,
                    bp.issue_url, bp.latest_tree_hash, bp.latest_completed_run_id,
                    bp.created_at, bp.updated_at,
                    (SELECT br.status FROM board_runs br WHERE br.board_project_id = bp.id
                     ORDER BY br.created_at DESC LIMIT 1) AS latest_run_status
                FROM board_projects bp
                WHERE bp.repository_id = $1 AND (bp.updated_at, bp.id) < ($2, $3)
                ORDER BY bp.updated_at DESC, bp.id DESC
                LIMIT $4"#,
            )
            .bind(repository_id)
            .bind(ts)
            .bind(id)
            .bind(limit)
            .fetch_all(executor)
            .await
        }
        None => {
            sqlx::query_as::<_, BoardProjectWithLatestRunStatus>(
                r#"SELECT bp.id, bp.repository_id, bp.project_path, bp.project_dir, bp.display_name,
                    bp.issue_url, bp.latest_tree_hash, bp.latest_completed_run_id,
                    bp.created_at, bp.updated_at,
                    (SELECT br.status FROM board_runs br WHERE br.board_project_id = bp.id
                     ORDER BY br.created_at DESC LIMIT 1) AS latest_run_status
                FROM board_projects bp
                WHERE bp.repository_id = $1
                ORDER BY bp.updated_at DESC, bp.id DESC
                LIMIT $2"#,
            )
            .bind(repository_id)
            .bind(limit)
            .fetch_all(executor)
            .await
        }
    }
}

pub async fn get_latest_run_status(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    board_project_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT br.status FROM board_runs br WHERE br.board_project_id = $1 ORDER BY br.created_at DESC LIMIT 1",
    )
    .bind(board_project_id)
    .fetch_optional(executor)
    .await
}

/// Update issue info after creating an issue
pub async fn update_issue_info(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    issue_number: i32,
    issue_node_id: &str,
    issue_url: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE board_projects SET issue_number = $2, issue_node_id = $3, issue_url = $4, issue_sync_status = 'synced', updated_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .bind(issue_number)
    .bind(issue_node_id)
    .bind(issue_url)
    .execute(executor)
    .await?;
    Ok(())
}

/// Update dashboard_comment_id after creating a dashboard comment
pub async fn update_dashboard_comment_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
    comment_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE board_projects SET dashboard_comment_id = $2, updated_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .bind(comment_id)
    .execute(executor)
    .await?;
    Ok(())
}

/// Clear dashboard_comment_id (e.g., when the comment is deleted or needs recreation)
pub async fn clear_dashboard_comment_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE board_projects SET dashboard_comment_id = NULL, updated_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .execute(executor)
    .await?;
    Ok(())
}

/// Find the repository associated with a board_project
pub async fn find_repository_by_board_project_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    board_project_id: Uuid,
) -> Result<Option<boardflow_domain::models::repository::Repository>, sqlx::Error> {
    sqlx::query_as::<_, boardflow_domain::models::repository::Repository>(
        r#"SELECT r.* FROM repositories r
        JOIN board_projects bp ON bp.repository_id = r.id
        WHERE bp.id = $1"#,
    )
    .bind(board_project_id)
    .fetch_optional(executor)
    .await
}
