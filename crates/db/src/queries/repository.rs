use boardflow_domain::models::repository::Repository;
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct RepositoryWithStats {
    pub id: Uuid,
    pub github_repository_id: i64,
    pub owner: String,
    pub name: String,
    pub installation_id: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub board_project_count: i64,
    pub latest_run_status: Option<String>,
}

pub async fn find_by_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<Option<Repository>, sqlx::Error> {
    sqlx::query_as::<_, Repository>("SELECT * FROM repositories WHERE id = $1")
        .bind(id)
        .fetch_optional(executor)
        .await
}

pub async fn find_by_github_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    github_repository_id: i64,
) -> Result<Option<Repository>, sqlx::Error> {
    sqlx::query_as::<_, Repository>(
        "SELECT * FROM repositories WHERE github_repository_id = $1",
    )
    .bind(github_repository_id)
    .fetch_optional(executor)
    .await
}

pub async fn list_with_stats(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    limit: i64,
    cursor: Option<(DateTime<Utc>, i64)>,
    accessible_repo_ids: Option<&[i64]>,
) -> Result<Vec<RepositoryWithStats>, sqlx::Error> {
    match (cursor, accessible_repo_ids) {
        (Some((ts, gid)), Some(ids)) => {
            sqlx::query_as::<_, RepositoryWithStats>(
                r#"SELECT r.*,
                    (SELECT COUNT(*) FROM board_projects bp WHERE bp.repository_id = r.id) AS board_project_count,
                    (SELECT br.status FROM board_runs br
                     JOIN board_projects bp ON bp.id = br.board_project_id
                     WHERE bp.repository_id = r.id
                     ORDER BY br.created_at DESC LIMIT 1) AS latest_run_status
                FROM repositories r
                WHERE r.github_repository_id = ANY($1)
                  AND (r.updated_at, r.github_repository_id) < ($2, $3)
                ORDER BY r.updated_at DESC, r.github_repository_id DESC
                LIMIT $4"#,
            )
            .bind(ids)
            .bind(ts)
            .bind(gid)
            .bind(limit)
            .fetch_all(executor)
            .await
        }
        (Some((ts, gid)), None) => {
            sqlx::query_as::<_, RepositoryWithStats>(
                r#"SELECT r.*,
                    (SELECT COUNT(*) FROM board_projects bp WHERE bp.repository_id = r.id) AS board_project_count,
                    (SELECT br.status FROM board_runs br
                     JOIN board_projects bp ON bp.id = br.board_project_id
                     WHERE bp.repository_id = r.id
                     ORDER BY br.created_at DESC LIMIT 1) AS latest_run_status
                FROM repositories r
                WHERE (r.updated_at, r.github_repository_id) < ($1, $2)
                ORDER BY r.updated_at DESC, r.github_repository_id DESC
                LIMIT $3"#,
            )
            .bind(ts)
            .bind(gid)
            .bind(limit)
            .fetch_all(executor)
            .await
        }
        (None, Some(ids)) => {
            sqlx::query_as::<_, RepositoryWithStats>(
                r#"SELECT r.*,
                    (SELECT COUNT(*) FROM board_projects bp WHERE bp.repository_id = r.id) AS board_project_count,
                    (SELECT br.status FROM board_runs br
                     JOIN board_projects bp ON bp.id = br.board_project_id
                     WHERE bp.repository_id = r.id
                     ORDER BY br.created_at DESC LIMIT 1) AS latest_run_status
                FROM repositories r
                WHERE r.github_repository_id = ANY($1)
                ORDER BY r.updated_at DESC, r.github_repository_id DESC
                LIMIT $2"#,
            )
            .bind(ids)
            .bind(limit)
            .fetch_all(executor)
            .await
        }
        (None, None) => {
            sqlx::query_as::<_, RepositoryWithStats>(
                r#"SELECT r.*,
                    (SELECT COUNT(*) FROM board_projects bp WHERE bp.repository_id = r.id) AS board_project_count,
                    (SELECT br.status FROM board_runs br
                     JOIN board_projects bp ON bp.id = br.board_project_id
                     WHERE bp.repository_id = r.id
                     ORDER BY br.created_at DESC LIMIT 1) AS latest_run_status
                FROM repositories r
                ORDER BY r.updated_at DESC, r.github_repository_id DESC
                LIMIT $1"#,
            )
            .bind(limit)
            .fetch_all(executor)
            .await
        }
    }
}

pub async fn upsert(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    github_repository_id: i64,
    owner: &str,
    name: &str,
    installation_id: i64,
) -> Result<Repository, sqlx::Error> {
    let id = Uuid::now_v7();
    sqlx::query_as::<_, Repository>(
        "INSERT INTO repositories (id, github_repository_id, owner, name, installation_id, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, NOW(), NOW()) \
         ON CONFLICT (github_repository_id) DO UPDATE SET \
         owner = EXCLUDED.owner, name = EXCLUDED.name, installation_id = EXCLUDED.installation_id, updated_at = NOW() \
         RETURNING *",
    )
    .bind(id)
    .bind(github_repository_id)
    .bind(owner)
    .bind(name)
    .bind(installation_id)
    .fetch_one(executor)
    .await
}
