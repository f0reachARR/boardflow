#![allow(clippy::too_many_arguments)]

pub mod queries;

use std::collections::HashMap;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

pub async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
}

pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}

#[derive(Debug, Clone)]
pub struct MigrationEntry {
    pub version: i64,
    pub description: String,
    pub applied_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn migration_status(pool: &PgPool) -> Result<Vec<MigrationEntry>, sqlx::Error> {
    let migrator = sqlx::migrate!("./migrations");

    let applied: HashMap<i64, chrono::DateTime<chrono::Utc>> = match sqlx::query_as::<
        _,
        (i64, chrono::DateTime<chrono::Utc>),
    >(
        "SELECT version, installed_on FROM _sqlx_migrations WHERE success",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows.into_iter().collect(),
        Err(sqlx::Error::Database(err)) if err.code().as_deref() == Some("42P01") => {
            HashMap::new()
        }
        Err(err) => return Err(err),
    };

    let mut entries: Vec<MigrationEntry> = migrator
        .iter()
        .filter(|m| !m.migration_type.is_down_migration())
        .map(|m| MigrationEntry {
            version: m.version,
            description: m.description.to_string(),
            applied_at: applied.get(&m.version).copied(),
        })
        .collect();
    entries.sort_by_key(|e| e.version);
    Ok(entries)
}
