pub struct WorkerConfig {
    pub database_url: String,
    pub staging_bucket: String,
    pub artifacts_bucket: String,
    pub s3_endpoint: Option<String>,
    pub poll_interval_secs: u64,
}

impl WorkerConfig {
    pub fn from_env() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            staging_bucket: std::env::var("STAGING_BUCKET")
                .unwrap_or_else(|_| "boardflow-staging".into()),
            artifacts_bucket: std::env::var("ARTIFACTS_BUCKET")
                .unwrap_or_else(|_| "boardflow-artifacts".into()),
            s3_endpoint: std::env::var("S3_ENDPOINT").ok(),
            poll_interval_secs: std::env::var("POLL_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2),
        }
    }
}
