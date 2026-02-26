use anyhow::Result;
use sqlx::PgPool;
use tracing::{info, warn};

use crate::{reading_cache::ReadingCache, tuya::TuyaClient};

pub struct SensorService {
    pool: PgPool,
    tuya: TuyaClient,
    cache: ReadingCache,
}

impl SensorService {
    pub fn new(pool: PgPool, tuya: TuyaClient, cache: ReadingCache) -> Self {
        Self { pool, tuya, cache }
    }

    /// Fetches the current status of `device_id` from Tuya, maps known
    /// data-point codes to typed columns, persists to DB, and updates the
    /// shared in-memory cache.
    pub async fn fetch_and_persist(&self, device_id: &str) -> Result<()> {
        info!(device_id = %device_id, "Fetching sensor reading");

        let status = self.tuya.get_device_status(device_id).await?;

        // Map DP codes → typed fields.
        // Adjust the code strings to match your actual Tuya device DPs.
        let mut temperature: Option<f64> = None;
        let mut humidity: Option<f64> = None;
        let mut door_open: Option<bool> = None;
        let mut power_consumption: Option<f64> = None;
        let mut relay_state: Option<bool> = None;
        let mut temperature_setpoint: Option<f64> = None;

        for dp in &status.result {
            match dp.code.as_str() {
                "temp_current" | "va_temperature" => {
                    temperature = dp.value.as_f64();
                }
                "humidity_value" | "va_humidity" => {
                    humidity = dp.value.as_f64();
                }
                "doorcontact_state" => {
                    door_open = dp.value.as_bool();
                }
                "cur_power" => {
                    power_consumption = dp.value.as_f64().map(|v| v / 10.0);
                }
                "switch_1" | "switch" => {
                    relay_state = dp.value.as_bool();
                }
                "temp_set" => {
                    temperature_setpoint = dp.value.as_f64();
                }
                code => {
                    warn!(device_id = %device_id, code = %code, "Unknown DP code — ignoring");
                }
            }
        }

        let reading = sqlx::query_as!(
            crate::db::models::SensorReading,
            r#"
            INSERT INTO sensor_readings
                (device_id, temperature, humidity, door_open,
                 power_consumption, relay_state, temperature_setpoint)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
            device_id,
            temperature,
            humidity,
            door_open,
            power_consumption,
            relay_state,
            temperature_setpoint,
        )
        .fetch_one(&self.pool)
        .await?;

        self.cache.update(reading).await;

        info!(device_id = %device_id, "Sensor reading persisted and cache updated");
        Ok(())
    }
}
