#![allow(dead_code)]

pub mod models;

use anyhow::{Context, Result};
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

use crate::config::Config;

use self::models::{
    Command, DeviceStatusResponse, SendCommandRequest, SendCommandResponse, TokenResponse,
};

#[derive(Debug, Clone)]
pub struct TuyaClient {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    http: Client,
    base_url: String,
    client_id: String,
    client_secret: String,
    token: Mutex<Option<CachedToken>>,
}

#[derive(Debug)]
struct CachedToken {
    access_token: String,
    /// Unix timestamp (seconds) when this token expires
    expires_at: i64,
}

impl TuyaClient {
    pub fn new(config: &Config) -> Self {
        Self {
            inner: Arc::new(Inner {
                http: Client::new(),
                base_url: config.tuya_base_url.clone(),
                client_id: config.tuya_client_id.clone(),
                client_secret: config.tuya_client_secret.clone(),
                token: Mutex::new(None),
            }),
        }
    }

    /// Returns a valid access token, refreshing it if necessary.
    async fn access_token(&self) -> Result<String> {
        let mut guard = self.inner.token.lock().await;
        let now = chrono::Utc::now().timestamp();

        if let Some(ref cached) = *guard {
            // Refresh 60 s before expiry
            if cached.expires_at > now + 60 {
                return Ok(cached.access_token.clone());
            }
        }

        info!("Fetching new Tuya access token");
        let token = self.fetch_token().await?;
        let expires_at = now + token.result.expire_time;
        let access_token = token.result.access_token.clone();

        *guard = Some(CachedToken {
            access_token: token.result.access_token,
            expires_at,
        });

        Ok(access_token)
    }

    async fn fetch_token(&self) -> Result<TokenResponse> {
        // TODO: Tuya requires HMAC-SHA256 signed requests.
        // Replace this placeholder URL with the correct signed token endpoint.
        let url = format!("{}/v1.0/token?grant_type=1", self.inner.base_url);
        debug!(url = %url, "Requesting Tuya token");

        let resp = self
            .inner
            .http
            .get(&url)
            .header("client_id", &self.inner.client_id)
            .header("secret", &self.inner.client_secret)
            .send()
            .await
            .context("Tuya token request failed")?
            .error_for_status()
            .context("Tuya token endpoint returned error status")?
            .json::<TokenResponse>()
            .await
            .context("Failed to deserialize Tuya token response")?;

        Ok(resp)
    }

    /// Fetch all data-point (DP) properties for a device.
    pub async fn get_device_status(&self, device_id: &str) -> Result<DeviceStatusResponse> {
        let token = self.access_token().await?;
        let url = format!("{}/v1.0/devices/{}/status", self.inner.base_url, device_id);
        debug!(device_id = %device_id, url = %url, "Fetching device status");

        let resp = self
            .inner
            .http
            .get(&url)
            .header("client_id", &self.inner.client_id)
            .header("access_token", &token)
            .send()
            .await
            .context("Tuya get_device_status request failed")?
            .error_for_status()
            .context("Tuya device status endpoint returned error status")?
            .json::<DeviceStatusResponse>()
            .await
            .context("Failed to deserialize Tuya device status response")?;

        Ok(resp)
    }

    /// Send one or more commands to a device.
    pub async fn send_commands(
        &self,
        device_id: &str,
        commands: Vec<Command>,
    ) -> Result<SendCommandResponse> {
        let token = self.access_token().await?;
        let url = format!("{}/v1.0/devices/{}/commands", self.inner.base_url, device_id);
        debug!(device_id = %device_id, "Sending commands to device");

        let body = SendCommandRequest { commands };

        let resp = self
            .inner
            .http
            .post(&url)
            .header("client_id", &self.inner.client_id)
            .header("access_token", &token)
            .json(&body)
            .send()
            .await
            .context("Tuya send_commands request failed")?
            .error_for_status()
            .context("Tuya commands endpoint returned error status")?
            .json::<SendCommandResponse>()
            .await
            .context("Failed to deserialize Tuya send_commands response")?;

        Ok(resp)
    }
}
