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
