/// Application settings — loaded from environment variables.
/// Equivalent of Django's settings.py or Node's config.
///
/// All configuration lives here. No env::var() calls anywhere else in the codebase.
pub struct Settings {
    pub database_url: String,
    pub psp_url: String,
    pub server_host: String,
    pub server_port: u16,
    pub max_db_connections: u32,
    pub psp_timeout_secs: u64,
    pub token_expiry_hours: u64,
    pub webhook_max_attempts: i32,
    pub webhook_poll_interval_secs: u64,
}

impl Settings {
    /// Load settings from environment variables with sensible defaults.
    pub fn from_env() -> Self {
        Self {
            database_url: env_or("DATABASE_URL", "postgres://postgres:postgres@localhost:5432/invoices"),
            psp_url: env_or("PSP_URL", "http://localhost:8081"),
            server_host: env_or("SERVER_HOST", "0.0.0.0"),
            server_port: env_or("SERVER_PORT", "8080").parse().unwrap_or(8080),
            max_db_connections: env_or("MAX_DB_CONNECTIONS", "10").parse().unwrap_or(10),
            psp_timeout_secs: env_or("PSP_TIMEOUT_SECS", "10").parse().unwrap_or(10),
            token_expiry_hours: env_or("TOKEN_EXPIRY_HOURS", "24").parse().unwrap_or(24),
            webhook_max_attempts: env_or("WEBHOOK_MAX_ATTEMPTS", "5").parse().unwrap_or(5),
            webhook_poll_interval_secs: env_or("WEBHOOK_POLL_INTERVAL_SECS", "1").parse().unwrap_or(1),
        }
    }

    pub fn server_addr(&self) -> String {
        format!("{}:{}", self.server_host, self.server_port)
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}
