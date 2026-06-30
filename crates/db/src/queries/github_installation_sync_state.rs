use chrono::{DateTime, Duration, Utc};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InstallationSyncState {
    pub installation_id: i64,
    pub webhook_seen_at: Option<DateTime<Utc>>,
    pub last_sync_started_at: Option<DateTime<Utc>>,
    pub last_sync_completed_at: Option<DateTime<Utc>>,
    pub last_sync_status: Option<String>,
    pub last_error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

pub async fn upsert_webhook_seen(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    installation_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO github_installation_sync_state (installation_id, webhook_seen_at, updated_at)
           VALUES ($1, NOW(), NOW())
           ON CONFLICT (installation_id) DO UPDATE SET
             webhook_seen_at = NOW(),
             updated_at = NOW()"#,
    )
    .bind(installation_id)
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn mark_sync_started(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    installation_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO github_installation_sync_state (installation_id, last_sync_started_at, updated_at)
           VALUES ($1, NOW(), NOW())
           ON CONFLICT (installation_id) DO UPDATE SET
             last_sync_started_at = NOW(),
             updated_at = NOW()"#,
    )
    .bind(installation_id)
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn mark_sync_completed(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    installation_id: i64,
    status: &str,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO github_installation_sync_state
             (installation_id, last_sync_completed_at, last_sync_status, last_error, updated_at)
           VALUES ($1, NOW(), $2, $3, NOW())
           ON CONFLICT (installation_id) DO UPDATE SET
             last_sync_completed_at = NOW(),
             last_sync_status = EXCLUDED.last_sync_status,
             last_error = EXCLUDED.last_error,
             updated_at = NOW()"#,
    )
    .bind(installation_id)
    .bind(status)
    .bind(error)
    .execute(executor)
    .await?;
    Ok(())
}

/// Installations that should be reconciled by the worker:
/// - never observed via webhook, or webhook is older than `stale_after`,
/// - AND not already synced more recently than `min_sync_interval`.
///
/// Failed syncs are eligible regardless of the throttle so they can recover.
pub async fn list_installations_needing_sync(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    stale_after: Duration,
    min_sync_interval: Duration,
    limit: i64,
) -> Result<Vec<InstallationSyncState>, sqlx::Error> {
    let webhook_cutoff = Utc::now() - stale_after;
    let sync_cutoff = Utc::now() - min_sync_interval;
    sqlx::query_as::<_, InstallationSyncState>(
        r#"SELECT * FROM github_installation_sync_state
           WHERE (webhook_seen_at IS NULL OR webhook_seen_at < $1)
             AND (
               last_sync_status = 'failed'
               OR last_sync_completed_at IS NULL
               OR last_sync_completed_at < $2
             )
             AND (last_sync_started_at IS NULL OR last_sync_started_at < $2)
           ORDER BY webhook_seen_at NULLS FIRST, installation_id
           LIMIT $3"#,
    )
    .bind(webhook_cutoff)
    .bind(sync_cutoff)
    .bind(limit)
    .fetch_all(executor)
    .await
}

pub async fn find_by_installation_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    installation_id: i64,
) -> Result<Option<InstallationSyncState>, sqlx::Error> {
    sqlx::query_as::<_, InstallationSyncState>(
        "SELECT * FROM github_installation_sync_state WHERE installation_id = $1",
    )
    .bind(installation_id)
    .fetch_optional(executor)
    .await
}
