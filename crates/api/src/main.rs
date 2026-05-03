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
    );

    let addr = format!("{}:{}", config.api_host, config.api_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
