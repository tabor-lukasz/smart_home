use std::collections::HashMap;

use anyhow::Result;
use sqlx::PgPool;
use tracing::{info, warn};

use crate::{
    config::DeviceType,
    db::models::{SensorReading, SensorType},
    reading_cache::ReadingCache,
    tuya::{
        models::{EnergyMeterStatus, ThermostatStatus, WeatherStationStatus},
        TuyaClient,
    },
};

pub struct SensorService {
    pool: PgPool,
    tuya: TuyaClient,
    cache: ReadingCache,
    device_ids: HashMap<String, DeviceType>,
}

impl SensorService {
    pub fn new(
        pool: PgPool,
        tuya: TuyaClient,
        cache: ReadingCache,
        device_ids: HashMap<String, DeviceType>,
    ) -> Self {
        Self { pool, tuya, cache, device_ids }
    }

    /// Returns the set of device IDs this service is configured to poll.
    pub fn device_ids(&self) -> impl Iterator<Item = &String> {
        self.device_ids.keys()
    }

    /// Fetches the current status of `device_id` from Tuya using the endpoint
    /// appropriate for its device type, maps each DP to a `(SensorType, i64)`
    /// pair, inserts one row per DP, and updates the shared in-memory cache.
    ///
    /// # Value encoding
    ///
    /// All values are stored as `round(real_value * 100)` per DB convention:
    ///
    /// | Device         | DP            | Raw | Real     | Stored |
    /// |----------------|---------------|-----|----------|--------|
    /// | Thermostat     | temp_current  | 189 | 18.9 °C  | 1890   |
    /// | Thermostat     | temp_set      | 220 | 22.0 °C  | 2200   |
    /// | EnergyMeter    | temp_current  |  16 | 16.0 °C  | 1600   |
    /// | WeatherStation | local_temp    | 208 | 20.8 °C  | 2080   |
    /// | WeatherStation | local_hum     |  51 | 51 %     | 5100   |
    pub async fn fetch_and_persist(&self, device_id: &str) -> Result<()> {
        info!(device_id = %device_id, "Fetching sensor readings");

        let readings = match self.device_ids.get(device_id) {
            Some(DeviceType::Thermostat) => {
                let dps = self.tuya.get_device_status(device_id).await?;
                let s = ThermostatStatus::try_from(dps.as_slice())?;
                // Raw ÷ 10 = °C → stored as °C × 100 = raw × 10
                vec![
                    (SensorType::Temperature, s.temp_current * 10),
                    (SensorType::TemperatureSetpoint, s.temp_set * 10),
                    (SensorType::RelayState, encode_bool(s.switch)),
                ]
            }

            Some(DeviceType::EnergyMeter) => {
                let dps = self.tuya.get_device_status(device_id).await?;
                let s = EnergyMeterStatus::try_from(dps.as_slice())?;
                let mut r = vec![(SensorType::RelayState, encode_bool(s.switch))];
                // Raw is already in °C (×1) → stored as °C × 100
                if let Some(t) = s.temp_current {
                    r.push((SensorType::Temperature, t * 100));
                }
                r
            }

            Some(DeviceType::WeatherStation) => {
                let props = self.tuya.get_weather_station_status(device_id).await?;
                let s = WeatherStationStatus::try_from(props.as_slice())?;
                // Raw ÷ 10 = °C → stored as °C × 100 = raw × 10
                // Raw humidity is already % → stored as % × 100
                let mut r = vec![
                    (SensorType::Temperature, s.local_temp * 10),
                    (SensorType::Humidity, s.local_hum * 100),
                ];
                if let Some(v) = s.sub1_temp { r.push((SensorType::Sub1Temperature, v * 10)); }
                if let Some(v) = s.sub1_hum  { r.push((SensorType::Sub1Humidity,    v * 100)); }
                if let Some(v) = s.sub2_temp { r.push((SensorType::Sub2Temperature, v * 10)); }
                if let Some(v) = s.sub2_hum  { r.push((SensorType::Sub2Humidity,    v * 100)); }
                if let Some(v) = s.sub3_temp { r.push((SensorType::Sub3Temperature, v * 10)); }
                if let Some(v) = s.sub3_hum  { r.push((SensorType::Sub3Humidity,    v * 100)); }
                r
            }

            None => {
                warn!(
                    device_id = %device_id,
                    "No device type configured for this device — skipping DP mapping. \
                     Add it to TUYA_DEVICE_IDS."
                );
                vec![]
            }
        };

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

/// Encode a boolean reading as an integer (`false` → 0, `true` → 1).
#[inline]
pub(crate) fn encode_bool(v: bool) -> i64 {
    v as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_bool_true_is_one() {
        assert_eq!(encode_bool(true), 1);
    }

    #[test]
    fn encode_bool_false_is_zero() {
        assert_eq!(encode_bool(false), 0);
    }
}
