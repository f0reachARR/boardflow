use boardflow_api::config::AppConfig;

/// 環境変数テストは process-global な状態を変更するため、
/// 単一テスト内で順次実行する。
#[test]
fn test_app_config_from_env() {
    // 1. DATABASE_URL が未設定の場合はエラーを返す
    unsafe {
        std::env::remove_var("DATABASE_URL");
        std::env::remove_var("API_HOST");
        std::env::remove_var("API_PORT");
        std::env::remove_var("RUST_LOG");
    }
    let result = AppConfig::from_env();
    assert!(result.is_err(), "DATABASE_URL未設定時はエラーを返すべき");

    // 2. DATABASE_URL のみ設定した場合、他のフィールドはデフォルト値
    unsafe {
        std::env::set_var("DATABASE_URL", "postgres://test:test@localhost/test");
    }
    let config = AppConfig::from_env().unwrap();
    assert_eq!(config.database_url, "postgres://test:test@localhost/test");
    assert_eq!(config.api_host, "0.0.0.0");
    assert_eq!(config.api_port, 3000);
    assert_eq!(config.rust_log, "info");

    // 3. 全フィールドをカスタム値で設定
    unsafe {
        std::env::set_var("DATABASE_URL", "postgres://custom@localhost/db");
        std::env::set_var("API_HOST", "127.0.0.1");
        std::env::set_var("API_PORT", "8080");
        std::env::set_var("RUST_LOG", "debug");
    }
    let config = AppConfig::from_env().unwrap();
    assert_eq!(config.database_url, "postgres://custom@localhost/db");
    assert_eq!(config.api_host, "127.0.0.1");
    assert_eq!(config.api_port, 8080);
    assert_eq!(config.rust_log, "debug");

    // 4. 無効なポート番号はデフォルト値3000にフォールバック
    unsafe {
        std::env::set_var("API_PORT", "not_a_number");
    }
    let config = AppConfig::from_env().unwrap();
    assert_eq!(config.api_port, 3000);
}
