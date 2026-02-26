use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::db::models::SensorType;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SensorReadingDto {
    pub id: Uuid,
    pub device_id: String,
    pub sensor_type: SensorType,
    pub recorded_at: DateTime<Utc>,
    /// Encoded integer value.
    /// Numeric sensors: real_value * 100 (e.g. 2145 = 21.45 °C).
    /// Boolean sensors: 0 = false, 1 = true.
    pub value: i64,
}

impl From<crate::db::models::SensorReading> for SensorReadingDto {
    fn from(r: crate::db::models::SensorReading) -> Self {
        Self {
            id: r.id,
            device_id: r.device_id,
            sensor_type: r.sensor_type,
            recorded_at: r.recorded_at,
            value: r.value,
        }
    }
}
