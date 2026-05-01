use boardflow_domain::models::api_token::BoardflowApiToken;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn find_by_hash(
    pool: &PgPool,
    token_hash: &str,
) -> Result<Option<BoardflowApiToken>, sqlx::Error> {
    sqlx::query_as::<_, BoardflowApiToken>(
        "SELECT id, installation_id, repository_id, name, token_hash, created_at, last_used_at, revoked_at FROM boardflow_api_tokens WHERE token_hash = $1",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

pub async fn update_last_used_at(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE boardflow_api_tokens SET last_used_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn create(
    pool: &PgPool,
    id: Uuid,
    installation_id: i64,
    repository_id: Uuid,
    name: &str,
    token_hash: &str,
) -> Result<BoardflowApiToken, sqlx::Error> {
    sqlx::query_as::<_, BoardflowApiToken>(
        "INSERT INTO boardflow_api_tokens (id, installation_id, repository_id, name, token_hash, created_at) \
         VALUES ($1, $2, $3, $4, $5, NOW()) \
         RETURNING id, installation_id, repository_id, name, token_hash, created_at, last_used_at, revoked_at",
    )
    .bind(id)
    .bind(installation_id)
    .bind(repository_id)
    .bind(name)
    .bind(token_hash)
    .fetch_one(pool)
    .await
}

pub async fn list_by_repository_id(
    pool: &PgPool,
    repository_id: Uuid,
    limit: i64,
    cursor: Option<(DateTime<Utc>, Uuid)>,
) -> Result<Vec<BoardflowApiToken>, sqlx::Error> {
    match cursor {
        None => {
            sqlx::query_as::<_, BoardflowApiToken>(
                "SELECT id, installation_id, repository_id, name, token_hash, created_at, last_used_at, revoked_at \
                 FROM boardflow_api_tokens \
                 WHERE repository_id = $1 \
                 ORDER BY created_at DESC, id DESC \
                 LIMIT $2",
            )
            .bind(repository_id)
            .bind(limit)
            .fetch_all(pool)
            .await
        }
        Some((cursor_ts, cursor_id)) => {
            sqlx::query_as::<_, BoardflowApiToken>(
                "SELECT id, installation_id, repository_id, name, token_hash, created_at, last_used_at, revoked_at \
                 FROM boardflow_api_tokens \
                 WHERE repository_id = $1 \
                   AND (created_at, id) < ($2, $3) \
                 ORDER BY created_at DESC, id DESC \
                 LIMIT $4",
            )
            .bind(repository_id)
            .bind(cursor_ts)
            .bind(cursor_id)
            .bind(limit)
            .fetch_all(pool)
            .await
        }
    }
}

pub async fn revoke(
    pool: &PgPool,
    token_id: Uuid,
    repository_id: Uuid,
) -> Result<Option<BoardflowApiToken>, sqlx::Error> {
    sqlx::query_as::<_, BoardflowApiToken>(
        "UPDATE boardflow_api_tokens \
         SET revoked_at = COALESCE(revoked_at, NOW()) \
         WHERE id = $1 AND repository_id = $2 \
         RETURNING id, installation_id, repository_id, name, token_hash, created_at, last_used_at, revoked_at",
    )
    .bind(token_id)
    .bind(repository_id)
    .fetch_optional(pool)
    .await
}
