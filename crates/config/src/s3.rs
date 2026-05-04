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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;

    #[test]
    #[serial]
    fn from_env_uses_defaults_when_unset() {
        unsafe {
            env::remove_var("MINIO_ENDPOINT");
            env::remove_var("MINIO_ACCESS_KEY");
            env::remove_var("MINIO_SECRET_KEY");
            env::remove_var("MINIO_BUCKET_STAGING");
            env::remove_var("MINIO_BUCKET_FINAL");
        }

        let config = S3Config::from_env();
        assert_eq!(config.endpoint, None);
        assert_eq!(config.access_key, None);
        assert_eq!(config.secret_key, None);
        assert_eq!(config.staging_bucket, "boardflow-staging");
        assert_eq!(config.final_bucket, "boardflow-final");
    }

    #[test]
    #[serial]
    fn from_env_reads_custom_values() {
        unsafe {
            env::set_var("MINIO_ENDPOINT", "http://custom:9000");
            env::set_var("MINIO_ACCESS_KEY", "mykey");
            env::set_var("MINIO_SECRET_KEY", "mysecret");
            env::set_var("MINIO_BUCKET_STAGING", "my-staging");
            env::set_var("MINIO_BUCKET_FINAL", "my-final");
        }

        let config = S3Config::from_env();
        assert_eq!(config.endpoint, Some("http://custom:9000".to_string()));
        assert_eq!(config.access_key, Some("mykey".to_string()));
        assert_eq!(config.secret_key, Some("mysecret".to_string()));
        assert_eq!(config.staging_bucket, "my-staging");
        assert_eq!(config.final_bucket, "my-final");

        unsafe {
            env::remove_var("MINIO_ENDPOINT");
            env::remove_var("MINIO_ACCESS_KEY");
            env::remove_var("MINIO_SECRET_KEY");
            env::remove_var("MINIO_BUCKET_STAGING");
            env::remove_var("MINIO_BUCKET_FINAL");
        }
    }
}
