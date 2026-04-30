use boardflow_domain::models::repository::Repository;
use uuid::Uuid;

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
