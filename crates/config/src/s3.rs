use crate::helpers::{optional_env, optional_env_or};

#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub staging_bucket: String,
    pub final_bucket: String,
}

impl S3Config {
    pub fn from_env() -> Self {
        Self {
            endpoint: optional_env("MINIO_ENDPOINT"),
            access_key: optional_env("MINIO_ACCESS_KEY"),
            secret_key: optional_env("MINIO_SECRET_KEY"),
            staging_bucket: optional_env_or("MINIO_BUCKET_STAGING", "boardflow-staging"),
            final_bucket: optional_env_or("MINIO_BUCKET_FINAL", "boardflow-final"),
        }
    }
}
