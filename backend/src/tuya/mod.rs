pub mod models;

use anyhow::{Context, Result};
use hmac::{Hmac, Mac};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use tracing::{debug, info};
use uuid::Uuid;

use crate::{config::Config, response_store};

use self::models::{
    Command, DeviceProperty, DeviceStatusResponse, SendCommandRequest, SendCommandResponse,
    ShadowPropertiesResponse, ShadowProperty, TokenResponse, TokenResult,
};

type HmacSha256 = Hmac<Sha256>;

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
        let expires_at = now + token.expire_time;
        let access_token = token.access_token.clone();

        *guard = Some(CachedToken {
            access_token: token.access_token,
            expires_at,
        });

        Ok(access_token)
    }

    async fn fetch_token(&self) -> Result<TokenResult> {
        let path = "/v1.0/token?grant_type=1";
        let url = format!("{}{}", self.inner.base_url, path);
        debug!(url = %url, "Requesting Tuya token");

        let headers = build_signed_headers(
            "GET",
            path,
            &[],
            &self.inner.client_id,
            &self.inner.client_secret,
            None,
        );

        let bytes = self
            .inner
            .http
            .get(&url)
            .headers(to_header_map(headers)?)
            .send()
            .await
            .context("Tuya token request failed")?
            .error_for_status()
            .context("Tuya token endpoint returned error status")?
            .bytes()
            .await
            .context("Failed to read Tuya token response body")?;

        response_store::save("token", "", &bytes).await;

        let resp = serde_json::from_slice::<TokenResponse>(&bytes)
            .context("Failed to deserialize Tuya token response")?
            .into_result()
            .context("Tuya token API call failed")?;

        Ok(resp)
    }

    /// Fetch all data-point (DP) properties for a device.
    pub async fn get_device_status(&self, device_id: &str) -> Result<Vec<DeviceProperty>> {
        let token = self.access_token().await?;
        let path = format!("/v1.0/devices/{}/status", device_id);
        let url = format!("{}{}", self.inner.base_url, path);
        debug!(device_id = %device_id, url = %url, "Fetching device status");

        let headers = build_signed_headers(
            "GET",
            &path,
            &[],
            &self.inner.client_id,
            &self.inner.client_secret,
            Some(&token),
        );

        let bytes = self
            .inner
            .http
            .get(&url)
            .headers(to_header_map(headers)?)
            .send()
            .await
            .context("Tuya get_device_status request failed")?
            .error_for_status()
            .context("Tuya device status endpoint returned error status")?
            .bytes()
            .await
            .context("Failed to read Tuya device status response body")?;

        response_store::save("device_status", device_id, &bytes).await;

        let resp = serde_json::from_slice::<DeviceStatusResponse>(&bytes)
            .context("Failed to deserialize Tuya device status response")?
            .into_result()
            .context("Tuya device status API call failed")?;

        Ok(resp)
    }

    /// Send one or more commands to a device.
    pub async fn send_commands(
        &self,
        device_id: &str,
        commands: Vec<Command>,
    ) -> Result<bool> {
        let token = self.access_token().await?;
        let path = format!("/v1.0/devices/{}/commands", device_id);
        let url = format!("{}{}", self.inner.base_url, path);
        debug!(device_id = %device_id, "Sending commands to device");

        let body = SendCommandRequest { commands };
        let body_bytes = serde_json::to_vec(&body).context("Failed to serialize command body")?;

        let headers = build_signed_headers(
            "POST",
            &path,
            &body_bytes,
            &self.inner.client_id,
            &self.inner.client_secret,
            Some(&token),
        );

        let bytes = self
            .inner
            .http
            .post(&url)
            .headers(to_header_map(headers)?)
            .json(&body)
            .send()
            .await
            .context("Tuya send_commands request failed")?
            .error_for_status()
            .context("Tuya commands endpoint returned error status")?
            .bytes()
            .await
            .context("Failed to read Tuya send_commands response body")?;

        response_store::save("send_commands", device_id, &bytes).await;

        let resp = serde_json::from_slice::<SendCommandResponse>(&bytes)
            .context("Failed to deserialize Tuya send_commands response")?
            .into_result()
            .context("Tuya send_commands API call failed")?;

        Ok(resp)
    }

    /// Fetch shadow properties for a device using the v2 IoT Core endpoint.
    ///
    /// Used for devices (e.g. weather stations) that return error 2003 on the
    /// standard v1 `/devices/{id}/status` endpoint.
    pub async fn get_weather_station_status(
        &self,
        device_id: &str,
    ) -> Result<Vec<ShadowProperty>> {
        let token = self.access_token().await?;
        let path = format!("/v2.0/cloud/thing/{}/shadow/properties", device_id);
        let url = format!("{}{}", self.inner.base_url, path);
        debug!(device_id = %device_id, url = %url, "Fetching weather station shadow properties");

        let headers = build_signed_headers(
            "GET",
            &path,
            &[],
            &self.inner.client_id,
            &self.inner.client_secret,
            Some(&token),
        );

        let bytes = self
            .inner
            .http
            .get(&url)
            .headers(to_header_map(headers)?)
            .send()
            .await
            .context("Tuya get_weather_station_status request failed")?
            .error_for_status()
            .context("Tuya shadow properties endpoint returned error status")?
            .bytes()
            .await
            .context("Failed to read Tuya shadow properties response body")?;

        response_store::save("weather_station", device_id, &bytes).await;

        let resp = serde_json::from_slice::<ShadowPropertiesResponse>(&bytes)
            .context("Failed to deserialize Tuya shadow properties response")?
            .into_result()
            .context("Tuya shadow properties API call failed")?;

        Ok(resp.properties)
    }
}

// ---------------------------------------------------------------------------
// Signing helpers
// ---------------------------------------------------------------------------

/// Deterministic signing inputs used by tests.
#[derive(Debug)]
pub(crate) struct SigningContext<'a> {
    pub method: &'a str,
    pub path_and_query: &'a str,
    pub body_bytes: &'a [u8],
    pub access_token: Option<&'a str>,
    /// 13-digit Unix timestamp in milliseconds
    pub t: &'a str,
    pub nonce: &'a str,
}

/// Build the Tuya-required signed request headers.
///
/// - `access_token`: `None` for token-fetch calls; `Some(token)` for all others.
/// - `body_bytes`: raw request body (empty slice for GET requests).
///
/// Tuya signing specification:
/// <https://developer.tuya.com/en/docs/iot/singnature?id=Ka43a5mtx1gsc>
pub(crate) fn build_signed_headers(
    method: &str,
    path_and_query: &str,
    body_bytes: &[u8],
    client_id: &str,
    secret: &str,
    access_token: Option<&str>,
) -> HashMap<String, String> {
    let t = chrono::Utc::now().timestamp_millis().to_string();
    let nonce = Uuid::new_v4().to_string();
    let ctx = SigningContext {
        method,
        path_and_query,
        body_bytes,
        access_token,
        t: &t,
        nonce: &nonce,
    };
    build_signed_headers_inner(client_id, secret, &ctx)
}

/// Inner implementation that accepts an explicit `SigningContext` so that
/// unit tests can inject deterministic timestamp and nonce values.
pub(crate) fn build_signed_headers_inner(
    client_id: &str,
    secret: &str,
    ctx: &SigningContext<'_>,
) -> HashMap<String, String> {
    let SigningContext { method, path_and_query, body_bytes, access_token, t, nonce } = ctx;
    // 1. SHA-256 of the request body (empty body → well-known hash).
    let content_sha256 = {
        let mut hasher = Sha256::new();
        hasher.update(body_bytes);
        hex::encode(hasher.finalize())
    };

    // 2. Build stringToSign.
    //    Format: HTTPMethod\nContent-SHA256\nHeaders\nUrl
    //    We send no custom Signature-Headers, so the Headers segment is empty.
    let string_to_sign = format!("{}\n{}\n\n{}", method, content_sha256, path_and_query);

    // 3. Build the string to HMAC.
    //    Token calls:   client_id + t + nonce + stringToSign
    //    Other calls:   client_id + access_token + t + nonce + stringToSign
    let str_to_hmac = match access_token {
        None => format!("{}{}{}{}", client_id, t, nonce, string_to_sign),
        Some(token) => format!("{}{}{}{}{}", client_id, token, t, nonce, string_to_sign),
    };

    // 4. HMAC-SHA256, uppercase hex.
    let sign = {
        let mut mac =
            HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
        mac.update(str_to_hmac.as_bytes());
        hex::encode(mac.finalize().into_bytes()).to_uppercase()
    };

    // 5. Assemble headers.
    let mut headers = HashMap::new();
    headers.insert("client_id".to_owned(), client_id.to_owned());
    headers.insert("t".to_owned(), t.to_string());
    headers.insert("nonce".to_owned(), nonce.to_string());
    headers.insert("sign_method".to_owned(), "HMAC-SHA256".to_owned());
    headers.insert("sign".to_owned(), sign);
    if let Some(token) = access_token {
        headers.insert("access_token".to_owned(), token.to_string());
    }

    headers
}

/// Convert our string `HashMap` into a `reqwest::header::HeaderMap`.
fn to_header_map(map: HashMap<String, String>) -> Result<reqwest::header::HeaderMap> {
    let mut header_map = reqwest::header::HeaderMap::new();
    for (k, v) in map {
        let name = reqwest::header::HeaderName::from_bytes(k.as_bytes())
            .with_context(|| format!("invalid header name: {k}"))?;
        let value = reqwest::header::HeaderValue::from_str(&v)
            .with_context(|| format!("invalid header value for {k}"))?;
        header_map.insert(name, value);
    }
    Ok(header_map)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Known-good values derived from the Tuya signing documentation example:
    // https://developer.tuya.com/en/docs/iot/singnature?id=Ka43a5mtx1gsc
    //
    // Parameters from the docs:
    //   client_id    = 1KAD46OrT9HafiKdsXeg
    //   secret       = 4OHBOnWOqaEC1mWXOpVL3yV50s0qGSRC
    //   t            = 1588925778000
    //   nonce        = 5138cc3a9033d69856923fd07b491173
    //   access_token = 3f4eda2bdec17232f67c0b188af3eec1   (None for token call)
    //   method       = GET
    //   path         = /v1.0/token?grant_type=1
    //   body         = (empty)
    //
    // The docs example also includes custom Signature-Headers (area_id, call_id).
    // Our implementation intentionally omits custom headers (empty Headers segment),
    // so we derive the expected sign ourselves using the same algorithm but without
    // those extra headers, and cross-check with the HMAC computation.

    const CLIENT_ID: &str = "1KAD46OrT9HafiKdsXeg";
    const SECRET: &str = "4OHBOnWOqaEC1mWXOpVL3yV50s0qGSRC";
    const T: &str = "1588925778000";
    const NONCE: &str = "5138cc3a9033d69856923fd07b491173";
    const ACCESS_TOKEN: &str = "3f4eda2bdec17232f67c0b188af3eec1";

    /// SHA-256 of empty body — this is a well-known constant.
    const EMPTY_BODY_SHA256: &str =
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn hmac_sign(s: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(SECRET.as_bytes())
            .expect("HMAC accepts any key length");
        mac.update(s.as_bytes());
        hex::encode(mac.finalize().into_bytes()).to_uppercase()
    }

    #[test]
    fn empty_body_sha256_is_well_known() {
        let mut hasher = Sha256::new();
        hasher.update(b"");
        let result = hex::encode(hasher.finalize());
        assert_eq!(result, EMPTY_BODY_SHA256);
    }

    #[test]
    fn token_call_sign_matches_manual_computation() {
        let path = "/v1.0/token?grant_type=1";
        let string_to_sign = format!("GET\n{EMPTY_BODY_SHA256}\n\n{path}");
        let str_to_hmac = format!("{CLIENT_ID}{T}{NONCE}{string_to_sign}");
        let expected_sign = hmac_sign(&str_to_hmac);

        let ctx = SigningContext {
            method: "GET",
            path_and_query: path,
            body_bytes: &[],
            access_token: None,
            t: T,
            nonce: NONCE,
        };
        let headers = build_signed_headers_inner(CLIENT_ID, SECRET, &ctx);

        assert_eq!(headers["sign"], expected_sign);
        assert_eq!(headers["client_id"], CLIENT_ID);
        assert_eq!(headers["t"], T);
        assert_eq!(headers["nonce"], NONCE);
        assert_eq!(headers["sign_method"], "HMAC-SHA256");
        assert!(!headers.contains_key("access_token"));
    }

    #[test]
    fn service_call_sign_matches_manual_computation() {
        let path = "/v1.0/devices/abc123/status";
        let string_to_sign = format!("GET\n{EMPTY_BODY_SHA256}\n\n{path}");
        let str_to_hmac = format!("{CLIENT_ID}{ACCESS_TOKEN}{T}{NONCE}{string_to_sign}");
        let expected_sign = hmac_sign(&str_to_hmac);

        let ctx = SigningContext {
            method: "GET",
            path_and_query: path,
            body_bytes: &[],
            access_token: Some(ACCESS_TOKEN),
            t: T,
            nonce: NONCE,
        };
        let headers = build_signed_headers_inner(CLIENT_ID, SECRET, &ctx);

        assert_eq!(headers["sign"], expected_sign);
        assert_eq!(headers["access_token"], ACCESS_TOKEN);
        assert!(
            !headers.contains_key("secret"),
            "secret must never appear in outgoing headers"
        );
    }

    #[test]
    fn post_body_produces_different_content_sha256() {
        let body = br#"{"commands":[{"code":"switch_1","value":true}]}"#;
        let path = "/v1.0/devices/abc123/commands";

        let ctx_empty = SigningContext {
            method: "POST",
            path_and_query: path,
            body_bytes: &[],
            access_token: Some(ACCESS_TOKEN),
            t: T,
            nonce: NONCE,
        };
        let ctx_with_body = SigningContext {
            method: "POST",
            path_and_query: path,
            body_bytes: body,
            access_token: Some(ACCESS_TOKEN),
            t: T,
            nonce: NONCE,
        };
        let headers_empty = build_signed_headers_inner(CLIENT_ID, SECRET, &ctx_empty);
        let headers_with_body = build_signed_headers_inner(CLIENT_ID, SECRET, &ctx_with_body);

        // Different body → different sign
        assert_ne!(
            headers_empty["sign"],
            headers_with_body["sign"],
            "body content must affect the signature"
        );
    }

    #[test]
    fn sign_is_uppercase_hex() {
        let ctx = SigningContext {
            method: "GET",
            path_and_query: "/v1.0/token?grant_type=1",
            body_bytes: &[],
            access_token: None,
            t: T,
            nonce: NONCE,
        };
        let headers = build_signed_headers_inner(CLIENT_ID, SECRET, &ctx);
        let sign = &headers["sign"];
        assert_eq!(sign.to_uppercase(), *sign, "sign must be uppercase");
        assert_eq!(sign.len(), 64, "HMAC-SHA256 hex is always 64 chars");
    }

    #[test]
    fn to_header_map_converts_correctly() {
        let mut map = HashMap::new();
        map.insert("client_id".to_owned(), "abc".to_owned());
        map.insert("sign".to_owned(), "DEF123".to_owned());

        let hm = to_header_map(map).expect("should convert");
        assert_eq!(hm["client_id"], "abc");
        assert_eq!(hm["sign"], "DEF123");
    }
}
