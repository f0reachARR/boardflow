use chrono::{Duration, Utc};
use serde_json::Value;
use uuid::Uuid;

pub async fn get_valid_cache(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    user_id: Uuid,
    cache_type: &str,
) -> Result<Option<Value>, sqlx::Error> {
    let row = sqlx::query_scalar::<_, Value>(
        "SELECT value_json FROM github_api_cache WHERE user_id = $1 AND cache_type = $2 AND expires_at > NOW()",
    )
    .bind(user_id)
    .bind(cache_type)
    .fetch_optional(executor)
    .await?;
    Ok(row)
}

pub async fn get_stale_cache(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    user_id: Uuid,
    cache_type: &str,
    max_stale_duration: Duration,
) -> Result<Option<Value>, sqlx::Error> {
    let cutoff = Utc::now() - max_stale_duration;
    let row = sqlx::query_scalar::<_, Value>(
        "SELECT value_json FROM github_api_cache WHERE user_id = $1 AND cache_type = $2 AND expires_at > $3",
    )
    .bind(user_id)
    .bind(cache_type)
    .bind(cutoff)
    .fetch_optional(executor)
    .await?;
    Ok(row)
}

pub async fn upsert_cache(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    user_id: Uuid,
    cache_type: &str,
    value_json: &Value,
    ttl_seconds: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO github_api_cache (user_id, cache_type, value_json, expires_at, created_at, updated_at)
           VALUES ($1, $2, $3, NOW() + make_interval(secs => $4), NOW(), NOW())
           ON CONFLICT (user_id, cache_type) DO UPDATE SET
             value_json = EXCLUDED.value_json,
             expires_at = EXCLUDED.expires_at,
             updated_at = NOW()"#,
    )
    .bind(user_id)
    .bind(cache_type)
    .bind(value_json)
    .bind(ttl_seconds as f64)
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn delete_cache_by_user(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM github_api_cache WHERE user_id = $1")
        .bind(user_id)
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn delete_cache(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    user_id: Uuid,
    cache_type: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM github_api_cache WHERE user_id = $1 AND cache_type = $2")
        .bind(user_id)
        .bind(cache_type)
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn cleanup_expired_cache(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
) -> Result<u64, sqlx::Error> {
    let result =
        sqlx::query("DELETE FROM github_api_cache WHERE expires_at < NOW() - INTERVAL '1 hour'")
            .execute(executor)
            .await?;
    Ok(result.rows_affected())
}
