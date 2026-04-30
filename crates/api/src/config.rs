pub struct AppConfig {
    pub database_url: String,
    pub api_host: String,
    pub api_port: u16,
    pub rust_log: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, std::env::VarError> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")?,
            api_host: std::env::var("API_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            api_port: std::env::var("API_PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .unwrap_or(3000),
            rust_log: std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        })
    }
}
