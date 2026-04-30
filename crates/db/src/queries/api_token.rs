use boardflow_domain::models::api_token::BoardflowApiToken;
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
