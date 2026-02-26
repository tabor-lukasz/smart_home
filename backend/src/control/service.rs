use std::time::Duration;

use tokio::time;
use tracing::{error, info};

use crate::{
    db::models::SensorType,
    reading_cache::ReadingCache,
    tuya::TuyaClient,
};

pub struct ControlService {
    // Intentionally retained for when control logic sends commands to Tuya.
    #[allow(dead_code)]
    tuya: TuyaClient,
    cache: ReadingCache,
    interval: Duration,
}

impl ControlService {
    pub fn new(tuya: TuyaClient, cache: ReadingCache, interval_secs: u64) -> Self {
        Self {
            tuya,
            cache,
            interval: Duration::from_secs(interval_secs),
        }
    }

    /// Runs the control loop indefinitely.
    /// Spawn this via `tokio::spawn`.
    pub async fn run(self) {
        info!(interval_secs = self.interval.as_secs(), "Control loop started");
        let mut ticker = time::interval(self.interval);

        loop {
            ticker.tick().await;
            if let Err(e) = self.run_once().await {
                error!(error = %e, "Control loop iteration failed");
            }
        }
    }

    async fn run_once(&self) -> anyhow::Result<()> {
        let readings = self.cache.all().await;

        if readings.is_empty() {
            info!("No sensor readings in cache yet; skipping control iteration");
            return Ok(());
        }

        // Group by device_id for convenient per-device logic.
        let mut by_device: std::collections::HashMap<&str, Vec<_>> = std::collections::HashMap::new();
        for r in &readings {
            by_device.entry(r.device_id.as_str()).or_default().push(r);
        }

        for (device_id, device_readings) in &by_device {
            // Look up individual sensor values by type.
            let temperature = device_readings
                .iter()
                .find(|r| r.sensor_type == SensorType::Temperature)
                .map(|r| r.value as f64 / 100.0);

            let relay_state = device_readings
                .iter()
                .find(|r| r.sensor_type == SensorType::RelayState)
                .map(|r| r.value != 0);

            info!(
                device_id = %device_id,
                temperature = ?temperature,
                relay_state = ?relay_state,
                "Control iteration — latest readings"
            );

            // Control logic goes here.
            // self.tuya.send_commands(device_id, vec![...]).await?;
            // Example: thermostat — compare temperature vs setpoint and toggle relay.
            //   let setpoint = device_readings.iter()
            //       .find(|r| r.sensor_type == SensorType::TemperatureSetpoint)
            //       .map(|r| r.value as f64 / 100.0);
            //   if let (Some(temp), Some(sp)) = (temperature, setpoint) {
            //       let should_heat = temp < sp;
            //       self.tuya.send_commands(device_id, vec![
            //           Command { code: "switch_1".into(), value: should_heat.into() },
            //       ]).await?;
            //   }
        }

        Ok(())
    }
}
