use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub database_url: String,
    pub storage_dir: String,
    pub assets_dir: String,
    pub log_level: String,
    pub request_timeout_secs: u64,
    pub secure: bool,
    pub secure_key: String,
    pub invite_code: String,
    pub user_limit: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            database_url: "sqlite:storage/reader.db?mode=rwc".to_string(),
            storage_dir: "storage".to_string(),
            assets_dir: "storage/assets".to_string(),
            log_level: "info".to_string(),
            request_timeout_secs: 15,
            secure: false,
            secure_key: "".to_string(),
            invite_code: "".to_string(),
            user_limit: 50,
        }
    }
}
