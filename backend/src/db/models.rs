use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

/// Mirrors the `sensor_type` Postgres enum.
///
/// Value encoding convention (stored as `BIGINT`):
/// - Numeric readings: `round(real_value * 100.0) as i64`
///   e.g. 21.45 °C → 2145, 60.5 % → 6050, 1234.56 W → 123456
/// - Boolean readings: `false` → 0, `true` → 1
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[sqlx(type_name = "sensor_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum SensorType {
    Temperature,
    Humidity,
    DoorOpen,
    PowerConsumption,
    RelayState,
    TemperatureSetpoint,
}

impl fmt::Display for SensorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SensorType::Temperature => "temperature",
            SensorType::Humidity => "humidity",
            SensorType::DoorOpen => "door_open",
            SensorType::PowerConsumption => "power_consumption",
            SensorType::RelayState => "relay_state",
            SensorType::TemperatureSetpoint => "temperature_setpoint",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct SensorReading {
    pub id: Uuid,
    pub device_id: String,
    pub sensor_type: SensorType,
    pub recorded_at: DateTime<Utc>,
    /// Encoded integer value — see `SensorType` for convention.
    pub value: i64,
}
