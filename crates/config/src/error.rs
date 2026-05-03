use std::num::ParseIntError;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("required environment variable {0} is not set")]
    MissingEnvVar(String),
    #[error("invalid value for {var}: {reason}")]
    InvalidValue { var: String, reason: String },
}

impl From<ParseIntError> for ConfigError {
    fn from(e: ParseIntError) -> Self {
        ConfigError::InvalidValue {
            var: String::new(),
            reason: e.to_string(),
        }
    }
}
