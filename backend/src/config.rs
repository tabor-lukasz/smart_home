use std::{collections::HashMap, str::FromStr};

use anyhow::{Context, Result};

// ---------------------------------------------------------------------------
// DeviceType
// ---------------------------------------------------------------------------

/// Known Tuya device categories used to select the correct polling endpoint
/// and DP mapping in `SensorService`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceType {
    Thermostat,
    EnergyMeter,
    WeatherStation,
}

impl FromStr for DeviceType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "thermostat" => Ok(Self::Thermostat),
            "energy_meter" => Ok(Self::EnergyMeter),
            "weather_station" => Ok(Self::WeatherStation),
            other => Err(anyhow::anyhow!("unknown device type: {other:?}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub tuya_client_id: String,
    pub tuya_client_secret: String,
    pub tuya_base_url: String,
    pub server_host: String,
    pub server_port: u16,
    /// Maps device_id → DeviceType,
    /// Format: `"id1:type1,id2:type2"` (e.g. `"abc:thermostat,def:energy_meter"`).
    pub device_ids: HashMap<String, DeviceType>,
    /// Sensor polling interval in seconds.
    pub poll_interval_secs: u64,
    /// Control loop interval in seconds.
    pub control_interval_secs: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            tuya_client_id: required("TUYA_CLIENT_ID")?,
            tuya_client_secret: required("TUYA_CLIENT_SECRET")?,
            tuya_base_url: required("TUYA_BASE_URL")?,
            server_host: optional("SERVER_HOST", "0.0.0.0"),
            server_port: optional("SERVER_PORT", "8080")
                .parse()
                .context("SERVER_PORT must be a valid port number")?,
            device_ids: parse_device_ids(&optional("TUYA_DEVICE_IDS", ""))?,
            poll_interval_secs: optional("POLL_INTERVAL_SECS", "60")
                .parse()
                .context("POLL_INTERVAL_SECS must be a positive integer")?,
            control_interval_secs: optional("CONTROL_INTERVAL_SECS", "60")
                .parse()
                .context("CONTROL_INTERVAL_SECS must be a positive integer")?,
        })
    }
}

/// Parse `"id1:type1,id2:type2"` into a `HashMap<String, DeviceType>`.
///
/// Returns an error immediately if any entry is malformed or contains an
/// unrecognised device type string.
fn parse_device_ids(raw: &str) -> Result<HashMap<String, DeviceType>> {
    raw.split(',')
        .filter(|s| !s.is_empty())
        .map(|entry| {
            let (id, kind) = entry.split_once(':').with_context(|| {
                format!("TUYA_DEVICE_IDS entry must be 'device_id:device_type', got: {entry:?}")
            })?;
            let kind = kind.trim().parse::<DeviceType>().with_context(|| {
                format!("unknown device type in TUYA_DEVICE_IDS entry {entry:?}")
            })?;
            Ok((id.trim().to_owned(), kind))
        })
        .collect()
}

fn required(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("missing required env var: {key}"))
}

fn optional(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_device_ids_empty() {
        let m = parse_device_ids("").unwrap();
        assert!(m.is_empty());
    }

    #[test]
    fn parse_device_ids_all_known() {
        let m = parse_device_ids("aaa:thermostat,bbb:energy_meter,ccc:weather_station").unwrap();
        assert_eq!(m["aaa"], DeviceType::Thermostat);
        assert_eq!(m["bbb"], DeviceType::EnergyMeter);
        assert_eq!(m["ccc"], DeviceType::WeatherStation);
    }

    #[test]
    fn parse_device_ids_unknown_type_errors() {
        let err = parse_device_ids("aaa:fridge").unwrap_err();
        assert!(err.to_string().contains("unknown device type"));
    }

    #[test]
    fn parse_device_ids_missing_colon_errors() {
        let err = parse_device_ids("aaa").unwrap_err();
        assert!(err.to_string().contains("device_id:device_type"));
    }

    #[test]
    fn device_type_from_str_roundtrip() {
        assert_eq!(
            "thermostat".parse::<DeviceType>().unwrap(),
            DeviceType::Thermostat
        );
        assert_eq!(
            "energy_meter".parse::<DeviceType>().unwrap(),
            DeviceType::EnergyMeter
        );
        assert_eq!(
            "weather_station".parse::<DeviceType>().unwrap(),
            DeviceType::WeatherStation
        );
    }
}
