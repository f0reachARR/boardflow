use boardflow_api::config::AppConfig;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::from_env()?;

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(&config.rust_log))
        .json()
        .init();

    tracing::info!("Starting BoardFlow API server");

    let pool = boardflow_db::create_pool(&config.database_url).await?;
    tracing::info!("Database connection established");

    let app = boardflow_api::create_app(pool);

    let addr = format!("{}:{}", config.api_host, config.api_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
