use boardflow_github::{GitHubAppClient, OctocrabGitHubAppClient};
use boardflow_github::GitHubAppConfig;
use secrecy::SecretString;

use boardflow_worker::config::WorkerConfig;
use boardflow_worker::dispatcher;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let config = WorkerConfig::from_env();
    tracing::info!("BoardFlow worker starting");

    let pool = boardflow_db::create_pool(&config.database_url)
        .await
        .expect("failed to connect to database");

    let s3_config = {
        let mut builder = aws_sdk_s3::config::Builder::new()
            .region(aws_sdk_s3::config::Region::new("us-east-1"));

        if let Some(ref endpoint) = config.s3_endpoint {
            builder = builder.endpoint_url(endpoint).force_path_style(true);
        }

        if let (Some(access_key), Some(secret_key)) =
            (&config.s3_access_key, &config.s3_secret_key)
        {
            builder = builder.credentials_provider(aws_sdk_s3::config::Credentials::new(
                access_key,
                secret_key,
                None,
                None,
                "env",
            ));
        }

        builder.build()
    };
    let s3_client = aws_sdk_s3::Client::from_conf(s3_config);

    // Initialize GitHub App client if configured
    let github_client: Option<Box<dyn GitHubAppClient>> =
        match (&config.github_app_id, &config.github_private_key_pem) {
            (Some(app_id), Some(pem)) => {
                let gh_config = GitHubAppConfig {
                    app_id: *app_id,
                    private_key_pem: SecretString::from(pem.clone()),
                };
                match OctocrabGitHubAppClient::new(&gh_config) {
                    Ok(client) => {
                        tracing::info!("GitHub App client initialized");
                        Some(Box::new(client))
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to initialize GitHub App client");
                        None
                    }
                }
            }
            _ => {
                tracing::warn!("GitHub App credentials not configured, GitHub API jobs will be deferred");
                None
            }
        };

    tracing::info!("BoardFlow worker started, polling for jobs");

    let mut sweep_interval = tokio::time::interval(std::time::Duration::from_secs(config.timeout_sweep_interval_secs));
    sweep_interval.tick().await; // 初回tickを消化

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                tracing::info!("Shutdown signal received, stopping worker");
                break;
            }
            _ = dispatcher::poll_and_dispatch(
                &pool,
                &s3_client,
                &config,
                github_client.as_deref(),
            ) => {}
            _ = sweep_interval.tick() => {
                dispatcher::sweep_timed_out_runs(&pool).await;
            }
        }
    }

    tracing::info!("BoardFlow worker stopped");
}
