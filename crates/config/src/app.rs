use crate::{
    ConfigError, DatabaseConfig, S3Config, load_dotenv, optional_env, optional_env_or,
    parse_env_or, required_env,
};

#[derive(Debug)]
pub struct AppConfig {
    pub db: DatabaseConfig,
    pub s3: S3Config,
    pub redis_url: Option<String>,
    pub api_host: String,
    pub api_port: u16,
    pub rust_log: String,
    pub github_client_id: Option<String>,
    pub github_client_secret: Option<String>,
    pub session_secret: Option<String>,
    pub artifact_secret: String,
    pub app_domain: String,
    pub artifact_base_url: String,
    pub github_webhook_secret: Option<String>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        load_dotenv()?;

        Ok(Self {
            db: DatabaseConfig::from_env()?,
            s3: S3Config::from_env(),
            redis_url: optional_env("REDIS_URL"),
            api_host: optional_env_or("API_HOST", "0.0.0.0"),
            api_port: parse_env_or("API_PORT", 3000u16)?,
            rust_log: optional_env_or("RUST_LOG", "info"),
            github_client_id: optional_env("GITHUB_CLIENT_ID"),
            github_client_secret: optional_env("GITHUB_CLIENT_SECRET"),
            session_secret: optional_env("BOARDFLOW_SESSION_SECRET"),
            artifact_secret: required_env("BOARDFLOW_ARTIFACT_SECRET")?,
            app_domain: optional_env_or("BOARDFLOW_APP_DOMAIN", "http://localhost:3000"),
            artifact_base_url: optional_env_or(
                "BOARDFLOW_ARTIFACT_BASE_URL",
                "http://localhost:8080",
            ),
            github_webhook_secret: optional_env("GITHUB_WEBHOOK_SECRET"),
        })
    }
}
