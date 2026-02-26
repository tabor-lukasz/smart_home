use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SensorReadingDto {
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

impl From<crate::db::models::SensorReading> for SensorReadingDto {
    fn from(r: crate::db::models::SensorReading) -> Self {
        Self {
            id: r.id,
            device_id: r.device_id,
            recorded_at: r.recorded_at,
            temperature: r.temperature,
            humidity: r.humidity,
            door_open: r.door_open,
            power_consumption: r.power_consumption,
            relay_state: r.relay_state,
            temperature_setpoint: r.temperature_setpoint,
        }
    }
}
