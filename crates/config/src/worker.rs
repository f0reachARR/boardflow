use crate::{
    ConfigError, DatabaseConfig, S3Config, load_dotenv, optional_env, optional_env_or, parse_env_or,
};

pub struct WorkerConfig {
    pub db: DatabaseConfig,
    pub s3: S3Config,
    pub poll_interval_secs: u64,
    pub timeout_sweep_interval_secs: u64,
    pub cache_cleanup_interval_secs: u64,
    pub github_app_id: Option<u64>,
    pub github_private_key_pem: Option<String>,
    pub app_domain: String,
}

impl WorkerConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        load_dotenv()?;

        let github_app_id = match optional_env("GITHUB_APP_ID") {
            Some(val) => Some(val.parse::<u64>().map_err(|e| ConfigError::InvalidValue {
                var: "GITHUB_APP_ID".to_string(),
                reason: e.to_string(),
            })?),
            None => None,
        };

        let cache_cleanup_interval_secs =
            parse_env_or("CACHE_CLEANUP_INTERVAL_SECS", 3600u64)?;
        if cache_cleanup_interval_secs == 0 {
            return Err(ConfigError::InvalidValue {
                var: "CACHE_CLEANUP_INTERVAL_SECS".to_string(),
                reason: "must be greater than 0".to_string(),
            });
        }

        Ok(Self {
            db: DatabaseConfig::from_env()?,
            s3: S3Config::from_env(),
            poll_interval_secs: parse_env_or("POLL_INTERVAL_SECS", 2u64)?,
            timeout_sweep_interval_secs: parse_env_or("TIMEOUT_SWEEP_INTERVAL_SECS", 60u64)?,
            cache_cleanup_interval_secs,
            github_app_id,
            github_private_key_pem: optional_env("GITHUB_PRIVATE_KEY_PEM"),
            app_domain: std::env::var("BOARDFLOW_APP_DOMAIN").unwrap_or_else(|_| {
                optional_env_or("APP_BASE_URL", "https://boardflow.example.com")
            }),
        })
    }
}
