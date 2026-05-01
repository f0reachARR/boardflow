use std::fmt;
use std::num::ParseIntError;

#[derive(Debug)]
pub enum ConfigError {
    MissingEnvVar(String),
    InvalidPort(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::MissingEnvVar(var) => write!(f, "required environment variable {var} is not set"),
            ConfigError::InvalidPort(val) => write!(f, "invalid API_PORT value: {val}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::env::VarError> for ConfigError {
    fn from(_: std::env::VarError) -> Self {
        ConfigError::MissingEnvVar("DATABASE_URL".to_string())
    }
}

#[derive(Debug)]
pub struct AppConfig {
    pub database_url: String,
    pub redis_url: Option<String>,
    pub minio_endpoint: Option<String>,
    pub minio_access_key: Option<String>,
    pub minio_secret_key: Option<String>,
    pub minio_bucket_staging: String,
    pub minio_bucket_final: String,
    pub api_host: String,
    pub api_port: u16,
    pub rust_log: String,
    pub github_client_id: Option<String>,
    pub github_client_secret: Option<String>,
    pub session_secret: Option<String>,
    pub artifact_secret: Option<String>,
    pub app_domain: String,
    pub artifact_base_url: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| ConfigError::MissingEnvVar("DATABASE_URL".to_string()))?;

        let api_port = match std::env::var("API_PORT") {
            Ok(val) => val
                .parse::<u16>()
                .map_err(|_: ParseIntError| ConfigError::InvalidPort(val))?,
            Err(_) => 3000,
        };

        Ok(Self {
            database_url,
            redis_url: std::env::var("REDIS_URL").ok(),
            minio_endpoint: std::env::var("MINIO_ENDPOINT").ok(),
            minio_access_key: std::env::var("MINIO_ACCESS_KEY").ok(),
            minio_secret_key: std::env::var("MINIO_SECRET_KEY").ok(),
            minio_bucket_staging: std::env::var("MINIO_BUCKET_STAGING").unwrap_or_else(|_| "boardflow-staging".to_string()),
            minio_bucket_final: std::env::var("MINIO_BUCKET_FINAL").unwrap_or_else(|_| "boardflow-final".to_string()),
            api_host: std::env::var("API_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            api_port,
            rust_log: std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            github_client_id: std::env::var("GITHUB_CLIENT_ID").ok(),
            github_client_secret: std::env::var("GITHUB_CLIENT_SECRET").ok(),
            session_secret: std::env::var("BOARDFLOW_SESSION_SECRET").ok(),
            artifact_secret: std::env::var("BOARDFLOW_ARTIFACT_SECRET").ok(),
            app_domain: std::env::var("BOARDFLOW_APP_DOMAIN").unwrap_or_else(|_| "http://localhost:3000".to_string()),
            artifact_base_url: std::env::var("BOARDFLOW_ARTIFACT_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string()),
        })
    }
}
