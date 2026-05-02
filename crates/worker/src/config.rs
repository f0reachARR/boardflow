pub struct WorkerConfig {
    pub database_url: String,
    pub staging_bucket: String,
    pub artifacts_bucket: String,
    pub s3_endpoint: Option<String>,
    pub s3_access_key: Option<String>,
    pub s3_secret_key: Option<String>,
    pub poll_interval_secs: u64,
    pub timeout_sweep_interval_secs: u64,
    pub github_app_id: Option<u64>,
    pub github_private_key_pem: Option<String>,
    pub app_base_url: String,
}

impl WorkerConfig {
    pub fn from_env() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            staging_bucket: std::env::var("MINIO_BUCKET_STAGING")
                .unwrap_or_else(|_| "boardflow-staging".into()),
            artifacts_bucket: std::env::var("MINIO_BUCKET_FINAL")
                .unwrap_or_else(|_| "boardflow-artifacts".into()),
            s3_endpoint: std::env::var("MINIO_ENDPOINT").ok(),
            s3_access_key: std::env::var("MINIO_ACCESS_KEY").ok(),
            s3_secret_key: std::env::var("MINIO_SECRET_KEY").ok(),
            poll_interval_secs: std::env::var("POLL_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2),
            timeout_sweep_interval_secs: std::env::var("TIMEOUT_SWEEP_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
            github_app_id: std::env::var("GITHUB_APP_ID")
                .ok()
                .and_then(|v| v.parse().ok()),
            github_private_key_pem: std::env::var("GITHUB_PRIVATE_KEY_PEM").ok(),
            app_base_url: std::env::var("APP_BASE_URL")
                .unwrap_or_else(|_| "https://boardflow.example.com".into()),
        }
    }
}
