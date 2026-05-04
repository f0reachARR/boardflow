use std::env;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use boardflow_config::{AppConfig, ConfigError, WorkerConfig};
use serial_test::serial;

fn test_dir(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = env::temp_dir().join(format!("boardflow-config-it-{name}-{unique}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn clear_env() {
    for key in [
        "DATABASE_URL",
        "BOARDFLOW_ARTIFACT_SECRET",
        "API_HOST",
        "API_PORT",
        "RUST_LOG",
        "REDIS_URL",
        "MINIO_ENDPOINT",
        "MINIO_ACCESS_KEY",
        "MINIO_SECRET_KEY",
        "MINIO_BUCKET_STAGING",
        "MINIO_BUCKET_FINAL",
        "GITHUB_CLIENT_ID",
        "GITHUB_CLIENT_SECRET",
        "BOARDFLOW_SESSION_SECRET",
        "BOARDFLOW_APP_DOMAIN",
        "BOARDFLOW_ARTIFACT_BASE_URL",
        "GITHUB_WEBHOOK_SECRET",
        "POLL_INTERVAL_SECS",
        "TIMEOUT_SWEEP_INTERVAL_SECS",
        "CACHE_CLEANUP_INTERVAL_SECS",
        "GITHUB_APP_ID",
        "GITHUB_PRIVATE_KEY_PEM",
        "APP_BASE_URL",
    ] {
        unsafe { env::remove_var(key) };
    }
}

#[test]
#[serial]
fn app_config_reads_values_from_dotenv() {
    clear_env();
    let original_dir = env::current_dir().unwrap();
    let root_dir = test_dir("app-config");
    let nested_dir = root_dir.join("crates/api");
    fs::create_dir_all(&nested_dir).unwrap();
    fs::write(
        root_dir.join(".env"),
        "DATABASE_URL=postgres://dotenv:dotenv@localhost/app\nBOARDFLOW_ARTIFACT_SECRET=dotenv-secret\nAPI_HOST=127.0.0.1\nAPI_PORT=4011\nRUST_LOG=debug\nMINIO_BUCKET_STAGING=dotenv-staging\nMINIO_BUCKET_FINAL=dotenv-final\n",
    )
    .unwrap();
    env::set_current_dir(&nested_dir).unwrap();

    let config = AppConfig::from_env().unwrap();

    env::set_current_dir(original_dir).unwrap();
    fs::remove_dir_all(root_dir).unwrap();
    clear_env();

    assert_eq!(
        config.db.database_url,
        "postgres://dotenv:dotenv@localhost/app"
    );
    assert_eq!(config.artifact_secret, "dotenv-secret");
    assert_eq!(config.api_host, "127.0.0.1");
    assert_eq!(config.api_port, 4011);
    assert_eq!(config.rust_log, "debug");
    assert_eq!(config.s3.staging_bucket, "dotenv-staging");
    assert_eq!(config.s3.final_bucket, "dotenv-final");
}

#[test]
#[serial]
fn worker_config_reads_values_from_dotenv() {
    clear_env();
    let original_dir = env::current_dir().unwrap();
    let root_dir = test_dir("worker-config");
    let nested_dir = root_dir.join("crates/worker");
    fs::create_dir_all(&nested_dir).unwrap();
    fs::write(
        root_dir.join(".env"),
        "DATABASE_URL=postgres://dotenv:dotenv@localhost/worker\nBOARDFLOW_APP_DOMAIN=https://boardflow.test\nPOLL_INTERVAL_SECS=9\nTIMEOUT_SWEEP_INTERVAL_SECS=45\nGITHUB_APP_ID=123\nGITHUB_PRIVATE_KEY_PEM=test-pem\n",
    )
    .unwrap();
    env::set_current_dir(&nested_dir).unwrap();

    let config = WorkerConfig::from_env().unwrap();

    env::set_current_dir(original_dir).unwrap();
    fs::remove_dir_all(root_dir).unwrap();
    clear_env();

    assert_eq!(
        config.db.database_url,
        "postgres://dotenv:dotenv@localhost/worker"
    );
    assert_eq!(config.app_domain, "https://boardflow.test");
    assert_eq!(config.poll_interval_secs, 9);
    assert_eq!(config.timeout_sweep_interval_secs, 45);
    assert_eq!(config.github_app_id, Some(123));
    assert_eq!(config.github_private_key_pem.as_deref(), Some("test-pem"));
}

#[test]
#[serial]
fn malformed_dotenv_fails_fast_during_config_load() {
    clear_env();
    let original_dir = env::current_dir().unwrap();
    let root_dir = test_dir("malformed-config");
    let nested_dir = root_dir.join("crates/api");
    fs::create_dir_all(&nested_dir).unwrap();
    fs::write(root_dir.join(".env"), "BROKEN LINE").unwrap();
    env::set_current_dir(&nested_dir).unwrap();

    let result = AppConfig::from_env();

    env::set_current_dir(original_dir).unwrap();
    fs::remove_dir_all(root_dir).unwrap();
    clear_env();

    assert!(matches!(result, Err(ConfigError::Dotenv(_))));
}
