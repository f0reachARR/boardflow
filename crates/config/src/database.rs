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
