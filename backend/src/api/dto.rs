use std::collections::BTreeMap;

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

/// Request body for `POST /sensors/readings`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SensorReadingsRequest {
    /// Device IDs to query.
    pub device_ids: Vec<String>,
    /// Sensor types to include.
    pub sensor_types: Vec<SensorType>,
    /// Start of time range (RFC3339, inclusive). Optional.
    pub from: Option<DateTime<Utc>>,
    /// End of time range (RFC3339, inclusive). Optional.
    pub to: Option<DateTime<Utc>>,
}

/// Response for `POST /sensors/readings`.
///
/// Outer key: `device_id`. Inner key: `sensor_type` (snake_case string).
/// Values are ordered by `recorded_at ASC`.
pub type SensorReadingsResponse = BTreeMap<String, BTreeMap<String, Vec<SensorReadingDto>>>;

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
