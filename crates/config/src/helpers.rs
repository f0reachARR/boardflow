use std::io::ErrorKind;
use std::str::FromStr;

use crate::error::ConfigError;

/// Load environment variables from a `.env` file if present.
pub fn load_dotenv() -> Result<(), ConfigError> {
    handle_dotenv_result(dotenvy::dotenv().map(|_| ()))
}

/// Read a required environment variable.
pub fn required_env(name: &str) -> Result<String, ConfigError> {
    std::env::var(name).map_err(|_| ConfigError::MissingEnvVar(name.to_string()))
}

/// Read an optional environment variable.
pub fn optional_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Read an optional environment variable with a default value.
pub fn optional_env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

/// Parse an environment variable with a default value.
pub fn parse_env_or<T: FromStr>(name: &str, default: T) -> Result<T, ConfigError>
where
    T::Err: std::fmt::Display,
{
    match std::env::var(name) {
        Ok(val) => val.parse::<T>().map_err(|e| ConfigError::InvalidValue {
            var: name.to_string(),
            reason: e.to_string(),
        }),
        Err(_) => Ok(default),
    }
}

fn handle_dotenv_result(result: Result<(), dotenvy::Error>) -> Result<(), ConfigError> {
    match result {
        Ok(()) => Ok(()),
        Err(dotenvy::Error::Io(err)) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(ConfigError::Dotenv(err.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    use serial_test::serial;

    fn with_test_dir(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = env::temp_dir().join(format!("boardflow-config-{name}-{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn clear_test_env() {
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
            "GITHUB_APP_ID",
            "GITHUB_PRIVATE_KEY_PEM",
            "APP_BASE_URL",
        ] {
            unsafe { env::remove_var(key) };
        }
    }

    fn write_env_file(dir: &Path, contents: &str) {
        fs::write(dir.join(".env"), contents).unwrap();
    }

    #[test]
    fn required_env_returns_error_when_missing() {
        unsafe { env::remove_var("__TEST_REQUIRED_MISSING") };
        let result = required_env("__TEST_REQUIRED_MISSING");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::MissingEnvVar(_)));
    }

    #[test]
    fn required_env_returns_value_when_set() {
        unsafe { env::set_var("__TEST_REQUIRED_SET", "hello") };
        let result = required_env("__TEST_REQUIRED_SET");
        assert_eq!(result.unwrap(), "hello");
        unsafe { env::remove_var("__TEST_REQUIRED_SET") };
    }

    #[test]
    fn optional_env_returns_none_when_missing() {
        unsafe { env::remove_var("__TEST_OPTIONAL_MISSING") };
        assert_eq!(optional_env("__TEST_OPTIONAL_MISSING"), None);
    }

    #[test]
    fn optional_env_returns_some_when_set() {
        unsafe { env::set_var("__TEST_OPTIONAL_SET", "world") };
        assert_eq!(
            optional_env("__TEST_OPTIONAL_SET"),
            Some("world".to_string())
        );
        unsafe { env::remove_var("__TEST_OPTIONAL_SET") };
    }

    #[test]
    fn optional_env_or_returns_default_when_missing() {
        unsafe { env::remove_var("__TEST_OPT_OR_MISSING") };
        assert_eq!(
            optional_env_or("__TEST_OPT_OR_MISSING", "default_val"),
            "default_val"
        );
    }

    #[test]
    fn optional_env_or_returns_value_when_set() {
        unsafe { env::set_var("__TEST_OPT_OR_SET", "custom") };
        assert_eq!(
            optional_env_or("__TEST_OPT_OR_SET", "default_val"),
            "custom"
        );
        unsafe { env::remove_var("__TEST_OPT_OR_SET") };
    }

    #[test]
    fn parse_env_or_returns_default_when_missing() {
        unsafe { env::remove_var("__TEST_PARSE_MISSING") };
        let result = parse_env_or("__TEST_PARSE_MISSING", 42u16);
        assert_eq!(result.unwrap(), 42u16);
    }

    #[test]
    fn parse_env_or_parses_value_when_set() {
        unsafe { env::set_var("__TEST_PARSE_SET", "99") };
        let result = parse_env_or("__TEST_PARSE_SET", 42u16);
        assert_eq!(result.unwrap(), 99u16);
        unsafe { env::remove_var("__TEST_PARSE_SET") };
    }

    #[test]
    fn parse_env_or_returns_invalid_value_on_parse_failure() {
        unsafe { env::set_var("__TEST_PARSE_BAD", "not_a_number") };
        let result = parse_env_or::<u16>("__TEST_PARSE_BAD", 42u16);
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::InvalidValue { var, .. } => assert_eq!(var, "__TEST_PARSE_BAD"),
            other => panic!("expected InvalidValue, got: {:?}", other),
        }
        unsafe { env::remove_var("__TEST_PARSE_BAD") };
    }

    #[test]
    #[serial]
    fn load_dotenv_succeeds_when_file_is_missing() {
        clear_test_env();
        let original_dir = env::current_dir().unwrap();
        let dir = with_test_dir("missing");
        env::set_current_dir(&dir).unwrap();

        let result = load_dotenv();

        env::set_current_dir(original_dir).unwrap();
        fs::remove_dir_all(dir).unwrap();

        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn load_dotenv_returns_error_for_malformed_file() {
        clear_test_env();
        let original_dir = env::current_dir().unwrap();
        let dir = with_test_dir("malformed");
        write_env_file(&dir, "MALFORMED LINE");
        env::set_current_dir(&dir).unwrap();

        let result = load_dotenv();

        env::set_current_dir(original_dir).unwrap();
        fs::remove_dir_all(dir).unwrap();

        assert!(matches!(result, Err(ConfigError::Dotenv(_))));
    }

    #[test]
    #[serial]
    fn load_dotenv_populates_process_env_from_parent_search() {
        clear_test_env();
        let original_dir = env::current_dir().unwrap();
        let root_dir = with_test_dir("search-root");
        let nested_dir = root_dir.join("crates/api");
        fs::create_dir_all(&nested_dir).unwrap();
        write_env_file(
            &root_dir,
            "DATABASE_URL=postgres://dotenv:test@localhost/dotenv\nBOARDFLOW_ARTIFACT_SECRET=dotenv-secret\nAPI_PORT=4010\nPOLL_INTERVAL_SECS=7\n",
        );
        env::set_current_dir(&nested_dir).unwrap();

        load_dotenv().unwrap();

        assert_eq!(
            env::var("DATABASE_URL").unwrap(),
            "postgres://dotenv:test@localhost/dotenv"
        );
        assert_eq!(
            env::var("BOARDFLOW_ARTIFACT_SECRET").unwrap(),
            "dotenv-secret"
        );
        assert_eq!(env::var("API_PORT").unwrap(), "4010");
        assert_eq!(env::var("POLL_INTERVAL_SECS").unwrap(), "7");

        env::set_current_dir(original_dir).unwrap();
        fs::remove_dir_all(root_dir).unwrap();
        clear_test_env();
    }
}
