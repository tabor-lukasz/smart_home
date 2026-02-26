use std::time::Duration;

use tokio::time;
use tracing::{error, info};

use crate::{reading_cache::ReadingCache, tuya::TuyaClient};

pub struct ControlService {
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

        for reading in &readings {
            info!(
                device_id = %reading.device_id,
                temperature = ?reading.temperature,
                relay_state = ?reading.relay_state,
                "Control iteration — latest reading"
            );

            // TODO: Implement real control logic here.
            // Example pattern:
            //   if let Some(temp) = reading.temperature {
            //       if let Some(setpoint) = reading.temperature_setpoint {
            //           if temp > setpoint {
            //               self.tuya.send_commands(&reading.device_id, vec![
            //                   Command { code: "switch_1".into(), value: true.into() },
            //               ]).await?;
            //           }
            //       }
            //   }
            let _ = &self.tuya; // will be used by real control logic
        }

        Ok(())
    }
}
