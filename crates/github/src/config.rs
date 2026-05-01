use secrecy::SecretString;

pub struct GitHubAppConfig {
    pub app_id: u64,
    pub private_key_pem: SecretString,
}
