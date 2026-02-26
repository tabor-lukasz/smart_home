use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct SensorReading {
    pub id: Uuid,
    pub device_id: String,
    pub recorded_at: DateTime<Utc>,
    /// Degrees Celsius
    pub temperature: Option<f64>,
    /// Relative humidity percentage
    pub humidity: Option<f64>,
    pub door_open: Option<bool>,
    /// Watts
    pub power_consumption: Option<f64>,
    pub relay_state: Option<bool>,
    /// Degrees Celsius
    pub temperature_setpoint: Option<f64>,
}
