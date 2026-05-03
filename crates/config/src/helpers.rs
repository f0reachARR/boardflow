use std::str::FromStr;

use crate::error::ConfigError;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

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
}
