use boardflow_api::config::AppConfig;
use boardflow_config::ConfigError;
use serial_test::serial;

/// 環境変数テストは process-global な状態を変更するため、
/// 単一テスト内で順次実行する。
#[test]
#[serial]
fn test_app_config_from_env() {
    // 1. DATABASE_URL が未設定の場合はエラーを返す
    unsafe {
        std::env::remove_var("DATABASE_URL");
        std::env::remove_var("BOARDFLOW_ARTIFACT_SECRET");
        std::env::remove_var("API_HOST");
        std::env::remove_var("API_PORT");
        std::env::remove_var("RUST_LOG");
        std::env::remove_var("REDIS_URL");
        std::env::remove_var("MINIO_ENDPOINT");
        std::env::remove_var("MINIO_ACCESS_KEY");
        std::env::remove_var("MINIO_SECRET_KEY");
        std::env::remove_var("MINIO_BUCKET_STAGING");
        std::env::remove_var("MINIO_BUCKET_FINAL");
    }
    let result = AppConfig::from_env();
    assert!(result.is_err(), "DATABASE_URL未設定時はエラーを返すべき");
    assert!(
        matches!(result.unwrap_err(), ConfigError::MissingEnvVar(ref var) if var == "DATABASE_URL")
    );

    // 2. DATABASE_URL + BOARDFLOW_ARTIFACT_SECRET のみ設定した場合、他のフィールドはデフォルト値
    unsafe {
        std::env::set_var("DATABASE_URL", "postgres://test:test@localhost/test");
        std::env::set_var("BOARDFLOW_ARTIFACT_SECRET", "test-secret");
    }
    let config = AppConfig::from_env().unwrap();
    assert_eq!(config.db.database_url, "postgres://test:test@localhost/test");
    assert_eq!(config.api_host, "0.0.0.0");
    assert_eq!(config.api_port, 3000);
    assert_eq!(config.rust_log, "info");
    assert_eq!(config.redis_url, None);
    assert_eq!(config.s3.endpoint, None);
    assert_eq!(config.s3.access_key, None);
    assert_eq!(config.s3.secret_key, None);
    assert_eq!(config.s3.staging_bucket, "boardflow-staging");
    assert_eq!(config.s3.final_bucket, "boardflow-final");

    // 3. 全フィールドをカスタム値で設定
    unsafe {
        std::env::set_var("DATABASE_URL", "postgres://custom@localhost/db");
        std::env::set_var("API_HOST", "127.0.0.1");
        std::env::set_var("API_PORT", "8080");
        std::env::set_var("RUST_LOG", "debug");
        std::env::set_var("REDIS_URL", "redis://localhost:6379");
        std::env::set_var("MINIO_ENDPOINT", "http://localhost:9000");
        std::env::set_var("MINIO_ACCESS_KEY", "minioadmin");
        std::env::set_var("MINIO_SECRET_KEY", "minioadmin");
        std::env::set_var("MINIO_BUCKET_STAGING", "custom-staging");
        std::env::set_var("MINIO_BUCKET_FINAL", "custom-final");
    }
    let config = AppConfig::from_env().unwrap();
    assert_eq!(config.db.database_url, "postgres://custom@localhost/db");
    assert_eq!(config.api_host, "127.0.0.1");
    assert_eq!(config.api_port, 8080);
    assert_eq!(config.rust_log, "debug");
    assert_eq!(config.redis_url.as_deref(), Some("redis://localhost:6379"));
    assert_eq!(
        config.s3.endpoint.as_deref(),
        Some("http://localhost:9000")
    );
    assert_eq!(config.s3.access_key.as_deref(), Some("minioadmin"));
    assert_eq!(config.s3.secret_key.as_deref(), Some("minioadmin"));
    assert_eq!(config.s3.staging_bucket, "custom-staging");
    assert_eq!(config.s3.final_bucket, "custom-final");

    // 4. 無効なポート番号はエラーを返す
    unsafe {
        std::env::set_var("API_PORT", "not_a_number");
    }
    let result = AppConfig::from_env();
    assert!(result.is_err(), "無効なポート番号はエラーを返すべき");
    assert!(matches!(result.unwrap_err(), ConfigError::InvalidValue { .. }));

    // 5. ポート番号が範囲外(65536以上)の場合もエラー
    unsafe {
        std::env::set_var("API_PORT", "70000");
    }
    let result = AppConfig::from_env();
    assert!(result.is_err(), "範囲外のポート番号はエラーを返すべき");
    assert!(matches!(result.unwrap_err(), ConfigError::InvalidValue { .. }));

    // cleanup
    unsafe {
        std::env::remove_var("REDIS_URL");
        std::env::remove_var("MINIO_ENDPOINT");
        std::env::remove_var("MINIO_ACCESS_KEY");
        std::env::remove_var("MINIO_SECRET_KEY");
        std::env::remove_var("MINIO_BUCKET_STAGING");
        std::env::remove_var("MINIO_BUCKET_FINAL");
        std::env::remove_var("BOARDFLOW_ARTIFACT_SECRET");
        std::env::set_var("API_PORT", "3000");
    }
}
