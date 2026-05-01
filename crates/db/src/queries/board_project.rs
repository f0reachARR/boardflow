use boardflow_domain::models::board_project::BoardProject;
use uuid::Uuid;

pub async fn find_by_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<Option<BoardProject>, sqlx::Error> {
    sqlx::query_as::<_, BoardProject>("SELECT * FROM board_projects WHERE id = $1")
        .bind(id)
        .fetch_optional(executor)
        .await
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
