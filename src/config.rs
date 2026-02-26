use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub tuya_client_id: String,
    pub tuya_client_secret: String,
    pub tuya_base_url: String,
    pub server_host: String,
    pub server_port: u16,
    /// Comma-separated list of device IDs to poll
    pub tuya_device_ids: Vec<String>,
    /// Sensor polling interval in seconds
    pub poll_interval_secs: u64,
    /// Control loop interval in seconds
    pub control_interval_secs: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            tuya_client_id: required("TUYA_CLIENT_ID")?,
            tuya_client_secret: required("TUYA_CLIENT_SECRET")?,
            tuya_base_url: required("TUYA_BASE_URL")?,
            server_host: optional("SERVER_HOST", "0.0.0.0"),
            server_port: optional("SERVER_PORT", "8080")
                .parse()
                .context("SERVER_PORT must be a valid port number")?,
            tuya_device_ids: optional("TUYA_DEVICE_IDS", "")
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.trim().to_owned())
                .collect(),
            poll_interval_secs: optional("POLL_INTERVAL_SECS", "60")
                .parse()
                .context("POLL_INTERVAL_SECS must be a positive integer")?,
            control_interval_secs: optional("CONTROL_INTERVAL_SECS", "60")
                .parse()
                .context("CONTROL_INTERVAL_SECS must be a positive integer")?,
        })
    }
}

fn required(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("missing required env var: {key}"))
}

fn optional(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}
