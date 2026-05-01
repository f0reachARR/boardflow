use boardflow_domain::models::session::Session;
use chrono::{DateTime, Utc};
use uuid::Uuid;

pub async fn find_by_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<Option<Session>, sqlx::Error> {
    sqlx::query_as::<_, Session>("SELECT * FROM sessions WHERE id = $1")
        .bind(id)
        .fetch_optional(executor)
        .await
}

pub async fn create(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    user_id: Uuid,
    expires_at: DateTime<Utc>,
) -> Result<Session, sqlx::Error> {
    let id = Uuid::now_v7();
    sqlx::query_as::<_, Session>(
        "INSERT INTO sessions (id, user_id, expires_at, created_at) VALUES ($1, $2, $3, NOW()) RETURNING *",
    )
    .bind(id)
    .bind(user_id)
    .bind(expires_at)
    .fetch_one(executor)
    .await
}

pub async fn delete_by_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(id)
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn delete_expired(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM sessions WHERE expires_at < NOW()")
        .execute(executor)
        .await?;
    Ok(result.rows_affected())
}
