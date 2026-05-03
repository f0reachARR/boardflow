use boardflow_domain::models::user::User;
use uuid::Uuid;

pub async fn find_by_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(executor)
        .await
}

pub async fn find_by_github_user_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    github_user_id: i64,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE github_user_id = $1")
        .bind(github_user_id)
        .fetch_optional(executor)
        .await
}

pub async fn find_by_github_access_token(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    github_access_token: &str,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE github_access_token = $1")
        .bind(github_access_token)
        .fetch_optional(executor)
        .await
}

pub async fn upsert(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    github_user_id: i64,
    github_login: &str,
    github_avatar_url: Option<&str>,
    github_access_token: &str,
) -> Result<User, sqlx::Error> {
    let id = Uuid::now_v7();
    sqlx::query_as::<_, User>(
        r#"INSERT INTO users (id, github_user_id, github_login, github_avatar_url, github_access_token, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
           ON CONFLICT (github_user_id) DO UPDATE SET
             github_login = EXCLUDED.github_login,
             github_avatar_url = EXCLUDED.github_avatar_url,
             github_access_token = EXCLUDED.github_access_token,
             updated_at = NOW()
           RETURNING *"#,
    )
    .bind(id)
    .bind(github_user_id)
    .bind(github_login)
    .bind(github_avatar_url)
    .bind(github_access_token)
    .fetch_one(executor)
    .await
}
