use anyhow::Result;
use sqlx::PgPool;
use tracing::{info, warn};

use crate::{
    db::models::{SensorReading, SensorType},
    reading_cache::ReadingCache,
    tuya::TuyaClient,
};

pub struct SensorService {
    pool: PgPool,
    tuya: TuyaClient,
    cache: ReadingCache,
}

impl SensorService {
    pub fn new(pool: PgPool, tuya: TuyaClient, cache: ReadingCache) -> Self {
        Self { pool, tuya, cache }
    }

    /// Fetches the current status of `device_id` from Tuya, maps each known
    /// data-point code to a `(SensorType, i64)` pair, inserts one row per DP,
    /// and updates the shared in-memory cache.
    pub async fn fetch_and_persist(&self, device_id: &str) -> Result<()> {
        info!(device_id = %device_id, "Fetching sensor readings");

        let status = self.tuya.get_device_status(device_id).await?;

        // Map Tuya DP codes → (SensorType, encoded i64 value).
        // Adjust code strings to match your actual device DPs.
        let mut readings: Vec<(SensorType, i64)> = Vec::new();

        for dp in &status.result {
            let mapped = match dp.code.as_str() {
                "temp_current" | "va_temperature" => {
                    dp.value.as_f64().map(|v| (SensorType::Temperature, encode_f64(v)))
                }
                "humidity_value" | "va_humidity" => {
                    dp.value.as_f64().map(|v| (SensorType::Humidity, encode_f64(v)))
                }
                "doorcontact_state" => {
                    dp.value.as_bool().map(|v| (SensorType::DoorOpen, encode_bool(v)))
                }
                "cur_power" => {
                    // Tuya reports cur_power in units of 0.1 W; divide by 10 to get W.
                    dp.value.as_f64().map(|v| (SensorType::PowerConsumption, encode_f64(v / 10.0)))
                }
                "switch_1" | "switch" => {
                    dp.value.as_bool().map(|v| (SensorType::RelayState, encode_bool(v)))
                }
                "temp_set" => {
                    dp.value.as_f64().map(|v| (SensorType::TemperatureSetpoint, encode_f64(v)))
                }
                code => {
                    warn!(device_id = %device_id, code = %code, "Unknown DP code — ignoring");
                    None
                }
            };

            if let Some(pair) = mapped {
                readings.push(pair);
            }
        }

        for (sensor_type, value) in readings {
            let reading = sqlx::query_as!(
                SensorReading,
                r#"
                INSERT INTO sensor_readings (device_id, sensor_type, value)
                VALUES ($1, $2, $3)
                ON CONFLICT (device_id, sensor_type, recorded_at) DO NOTHING
                RETURNING id, device_id, sensor_type AS "sensor_type: SensorType",
                          recorded_at, value
                "#,
                device_id,
                sensor_type as SensorType,
                value,
            )
            .fetch_optional(&self.pool)
            .await?;

            if let Some(reading) = reading {
                self.cache.update(reading).await;
            }
        }

        info!(device_id = %device_id, "Sensor readings persisted and cache updated");
        Ok(())
    }
}

/// Encode a floating-point reading as an integer (multiply by 100, round).
#[inline]
pub(crate) fn encode_f64(v: f64) -> i64 {
    (v * 100.0).round() as i64
}

/// Encode a boolean reading as an integer (false → 0, true → 1).
#[inline]
pub(crate) fn encode_bool(v: bool) -> i64 {
    v as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_f64_positive() {
        assert_eq!(encode_f64(21.45), 2145);
    }

    #[test]
    fn encode_f64_negative() {
        assert_eq!(encode_f64(-5.5), -550);
    }

    #[test]
    fn encode_f64_zero() {
        assert_eq!(encode_f64(0.0), 0);
    }

    #[test]
    fn encode_f64_rounds() {
        assert_eq!(encode_f64(21.455), 2146);
        assert_eq!(encode_f64(21.454), 2145);
    }

    #[test]
    fn encode_f64_large_value() {
        assert_eq!(encode_f64(12345.0 / 10.0), 123450);
    }

    #[test]
    fn encode_bool_true_is_one() {
        assert_eq!(encode_bool(true), 1);
    }

    #[test]
    fn encode_bool_false_is_zero() {
        assert_eq!(encode_bool(false), 0);
    }
}
