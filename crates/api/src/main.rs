use boardflow_api::config::AppConfig;
use boardflow_config::{DatabaseConfig, load_dotenv};
use tracing_subscriber::EnvFilter;

enum Command {
    Serve,
    MigrateUp,
    MigrateInfo,
}

fn parse_command() -> Result<Command, String> {
    let args: Vec<String> = std::env::args().collect();
    match args.iter().skip(1).map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] | ["serve"] => Ok(Command::Serve),
        ["migrate", "up"] => Ok(Command::MigrateUp),
        ["migrate", "info"] => Ok(Command::MigrateInfo),
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "Usage: boardflow-api [serve | migrate <up|info>]".to_string()
}

fn init_tracing(rust_log: &str) {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(rust_log))
        .json()
        .init();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let command = parse_command().map_err(|msg| -> Box<dyn std::error::Error> { msg.into() })?;

    match command {
        Command::Serve => run_serve().await,
        Command::MigrateUp => run_migrate_up().await,
        Command::MigrateInfo => run_migrate_info().await,
    }
}

async fn run_serve() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::from_env()?;
    init_tracing(&config.rust_log);

    tracing::info!("Starting BoardFlow API server");

    let pool = boardflow_db::create_pool(&config.db.database_url).await?;
    tracing::info!("Database connection established");

    let s3_client = if let Some(endpoint) = &config.s3.endpoint {
        let creds = aws_sdk_s3::config::Credentials::new(
            config.s3.access_key.as_deref().unwrap_or("minioadmin"),
            config.s3.secret_key.as_deref().unwrap_or("minioadmin"),
            None,
            None,
            "env",
        );
        let s3_config = aws_sdk_s3::Config::builder()
            .endpoint_url(endpoint)
            .region(aws_sdk_s3::config::Region::new("us-east-1"))
            .credentials_provider(creds)
            .force_path_style(true)
            .behavior_version_latest()
            .build();
        Some(aws_sdk_s3::Client::from_conf(s3_config))
    } else {
        None
    };

    let app = boardflow_api::create_app_with_config(
        pool,
        s3_client,
        Some(boardflow_api::routes::auth::OAuthConfig {
            client_id: config.github_client_id.unwrap_or_default(),
            client_secret: config.github_client_secret.unwrap_or_default(),
        }),
        Some(config.artifact_secret.into_bytes()),
        None, // access_checker - not from config
        Some(config.s3.final_bucket),
        Some(config.s3.staging_bucket),
        Some(config.app_domain),
        Some(config.artifact_base_url),
        config.github_webhook_secret,
        config.github_app_id,
    );

    let addr = format!("{}:{}", config.api_host, config.api_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn migrate_pool() -> Result<sqlx::PgPool, Box<dyn std::error::Error>> {
    load_dotenv()?;
    let db = DatabaseConfig::from_env()?;
    let rust_log = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    init_tracing(&rust_log);
    Ok(boardflow_db::create_pool(&db.database_url).await?)
}

async fn run_migrate_up() -> Result<(), Box<dyn std::error::Error>> {
    let pool = migrate_pool().await?;
    tracing::info!("Running database migrations");
    boardflow_db::run_migrations(&pool).await?;
    tracing::info!("Migrations complete");
    Ok(())
}

async fn run_migrate_info() -> Result<(), Box<dyn std::error::Error>> {
    let pool = migrate_pool().await?;
    let entries = boardflow_db::migration_status(&pool).await?;
    println!("{:<20} {:<24} DESCRIPTION", "VERSION", "APPLIED_AT");
    for entry in entries {
        let applied = entry
            .applied_at
            .map(|t| t.format("%Y-%m-%d %H:%M:%SZ").to_string())
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{:<20} {:<24} {}",
            entry.version, applied, entry.description
        );
    }
    Ok(())
}
