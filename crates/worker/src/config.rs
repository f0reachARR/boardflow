pub struct WorkerConfig {
    pub database_url: String,
    pub staging_bucket: String,
    pub artifacts_bucket: String,
    pub s3_endpoint: Option<String>,
    pub s3_access_key: Option<String>,
    pub s3_secret_key: Option<String>,
    pub poll_interval_secs: u64,
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
        }
    }
}
