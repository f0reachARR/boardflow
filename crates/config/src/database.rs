use crate::error::ConfigError;
use crate::helpers::required_env;

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub database_url: String,
}

impl DatabaseConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            database_url: required_env("DATABASE_URL")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn from_env_succeeds_when_database_url_set() {
        unsafe { env::set_var("DATABASE_URL", "postgres://test:test@localhost/test") };
        let config = DatabaseConfig::from_env().unwrap();
        assert_eq!(config.database_url, "postgres://test:test@localhost/test");
        unsafe { env::remove_var("DATABASE_URL") };
    }

    #[test]
    fn from_env_fails_when_database_url_missing() {
        unsafe { env::remove_var("DATABASE_URL") };
        let result = DatabaseConfig::from_env();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::MissingEnvVar(_)));
    }
}
