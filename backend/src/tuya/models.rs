#![allow(dead_code)]

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Token
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub result: TokenResult,
    pub success: bool,
    pub t: i64,
}

#[derive(Debug, Deserialize)]
pub struct TokenResult {
    pub access_token: String,
    pub expire_time: i64,
    pub refresh_token: String,
    pub uid: String,
}

// ---------------------------------------------------------------------------
// Device status
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DeviceStatusResponse {
    pub result: Vec<DeviceProperty>,
    pub success: bool,
    pub t: i64,
}

#[derive(Debug, Deserialize)]
pub struct DeviceProperty {
    pub code: String,
    pub value: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Send command
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SendCommandRequest {
    pub commands: Vec<Command>,
}

#[derive(Debug, Serialize)]
pub struct Command {
    pub code: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct SendCommandResponse {
    pub result: bool,
    pub success: bool,
    pub t: i64,
}
