use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Generic response envelope
//
// Every Tuya Cloud API wraps its payload in the same outer object:
//
// Success:
//   { "success": true, "t": 1545447665981, "result": <T>, "tid": "..." }
//
// Failure:
//   { "success": false, "t": 1561348644346, "code": 2009, "msg": "...", "tid": "..." }
//
// `result` is absent on failure; `code` and `msg` are absent on success.
// `tid` is a server-side request trace ID — present in all observed responses.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TuyaResponse<T> {
    /// `true` on success, `false` on API-level failure.
    pub success: bool,

    /// 13-digit Unix timestamp in milliseconds returned by the server.
    pub t: i64,

    /// Server-side request trace ID — useful for support queries.
    pub tid: Option<String>,

    /// Present on success.
    pub result: Option<T>,

    /// Tuya error code — present on failure.
    pub code: Option<i32>,

    /// Human-readable error message — present on failure.
    pub msg: Option<String>,
}

impl<T> TuyaResponse<T> {
    /// Convert into `anyhow::Result<T>`, mapping API-level failures to errors.
    pub fn into_result(self) -> anyhow::Result<T> {
        if self.success {
            self.result
                .ok_or_else(|| anyhow!("Tuya response: success=true but result field is missing"))
        } else {
            Err(anyhow!(
                "Tuya API error: code={}, msg={}",
                self.code.unwrap_or(-1),
                self.msg.as_deref().unwrap_or("(no message)")
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// DpValue — typed replacement for serde_json::Value in device properties
//
// Tuya DP values are polymorphic: a single response can contain booleans,
// integers, and strings. Using #[serde(untagged)] makes serde try each
// variant in order. Bool MUST come before Integer — JSON true/false would
// otherwise be coerced to 1/0 by some deserializers.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum DpValue {
    Bool(bool),
    Integer(i64),
    Text(String),
}

impl DpValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            DpValue::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            DpValue::Integer(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            DpValue::Text(v) => Some(v.as_str()),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Token  —  GET /v1.0/token?grant_type=1
// ---------------------------------------------------------------------------

/// Full response type: `TuyaResponse<TokenResult>`.
pub type TokenResponse = TuyaResponse<TokenResult>;

/// Payload inside a successful token response.
///
/// Reference: <https://developer.tuya.com/en/docs/cloud/6c1636a9bd?id=Ka7kjumkoa53v>
#[derive(Debug, Deserialize)]
pub struct TokenResult {
    /// Short-lived bearer token used in subsequent API calls.
    pub access_token: String,

    /// Validity period in **seconds** (typically 7200).
    pub expire_time: i64,

    /// Token used to obtain a new `access_token` without re-authenticating.
    pub refresh_token: String,

    /// Tuya user ID associated with this token.
    pub uid: String,
}

// ---------------------------------------------------------------------------
// Device status  —  GET /v1.0/devices/{device_id}/status
// ---------------------------------------------------------------------------

/// Full response type: `TuyaResponse<Vec<DeviceProperty>>`.
pub type DeviceStatusResponse = TuyaResponse<Vec<DeviceProperty>>;

/// A single data-point (DP) from the v1 device status endpoint.
#[derive(Debug, Deserialize)]
pub struct DeviceProperty {
    /// DP code, e.g. `"temp_current"`, `"switch_1"`, `"cur_power"`.
    pub code: String,

    /// DP value — bool, integer, or string depending on the DP type.
    pub value: DpValue,
}

// ---------------------------------------------------------------------------
// Shadow properties  —  GET /v2.0/cloud/thing/{device_id}/shadow/properties
//
// Used for devices (e.g. weather stations) that do not support the v1
// /devices/{id}/status endpoint.
// ---------------------------------------------------------------------------

/// Full response type: `TuyaResponse<ShadowPropertiesResult>`.
pub type ShadowPropertiesResponse = TuyaResponse<ShadowPropertiesResult>;

#[derive(Debug, Deserialize)]
pub struct ShadowPropertiesResult {
    pub properties: Vec<ShadowProperty>,
}

/// A single property from the v2 shadow/properties endpoint.
///
/// Compared to `DeviceProperty`, shadow properties carry a per-property
/// last-updated timestamp and an explicit type tag.
#[derive(Debug, Deserialize)]
pub struct ShadowProperty {
    /// DP code, e.g. `"local_temp"`, `"sub1_hum"`.
    pub code: String,

    /// Tuya DP numeric ID.
    pub dp_id: u32,

    /// Unix timestamp in milliseconds when this property was last updated.
    pub time: i64,

    /// DP type string: `"value"` | `"bool"` | `"raw"` | `"enum"` | `"bitmap"`.
    #[serde(rename = "type")]
    pub dp_type: String,

    /// DP value.
    pub value: DpValue,

    /// User-defined display name (often empty).
    pub custom_name: Option<String>,
}

// ---------------------------------------------------------------------------
// Send commands  —  POST /v1.0/devices/{device_id}/commands
// ---------------------------------------------------------------------------

/// Full response type: `TuyaResponse<bool>`.
pub type SendCommandResponse = TuyaResponse<bool>;

/// Request body sent to the commands endpoint.
#[derive(Debug, Serialize)]
pub struct SendCommandRequest {
    pub commands: Vec<Command>,
}

/// A single command to send to a device DP.
#[derive(Debug, Serialize)]
pub struct Command {
    /// DP code to target, e.g. `"switch_1"`.
    pub code: String,

    /// New value — use `DpValue::Bool`, `DpValue::Integer`, or `DpValue::Text`.
    pub value: DpValue,
}

// ---------------------------------------------------------------------------
// Typed device status structs
//
// Each struct is built from a raw slice of DPs via TryFrom rather than being
// a direct deserialisation target. This keeps the wire types decoupled from
// the domain model and makes required-field validation explicit.
// ---------------------------------------------------------------------------

// --- Thermostat (bf99bff9 family) -----------------------------------------
//
// Observed DPs (device_status, v1 endpoint):
//   switch        bool    relay on/off
//   temp_set      i64     220 = 22.0 °C  (÷10)
//   temp_current  i64     189 = 18.9 °C  (÷10)
//   mode          String  "auto" | "manual" | ...
//   child_lock    bool
//   fault         i64     bitmask
//   upper_temp    i64     absolute max setpoint
//   temp_correction i64   calibration offset (can be negative)
//   frost         bool    frost-protection mode
//   sound         bool    key beep on/off

/// Typed view of a thermostat device status.
#[derive(Debug, Clone)]
pub struct ThermostatStatus {
    pub switch: bool,
    /// Raw value: 189 → 18.9 °C  (divide by 10 to get °C).
    pub temp_current: i64,
    /// Raw value: 220 → 22.0 °C  (divide by 10 to get °C).
    pub temp_set: i64,
    pub mode: String,
    pub child_lock: Option<bool>,
    /// Fault bitmask — 0 means no fault.
    pub fault: Option<i64>,
    pub upper_temp: Option<i64>,
    /// Calibration offset; can be negative.
    pub temp_correction: Option<i64>,
    pub frost: Option<bool>,
    pub sound: Option<bool>,
}

impl ThermostatStatus {
    /// Current temperature in °C.
    pub fn temp_current_celsius(&self) -> f64 {
        self.temp_current as f64 / 10.0
    }

    /// Target setpoint in °C.
    pub fn temp_set_celsius(&self) -> f64 {
        self.temp_set as f64 / 10.0
    }
}

impl TryFrom<&[DeviceProperty]> for ThermostatStatus {
    type Error = anyhow::Error;

    fn try_from(dps: &[DeviceProperty]) -> anyhow::Result<Self> {
        let get = |code: &str| dps.iter().find(|dp| dp.code == code);

        let switch = get("switch")
            .and_then(|dp| dp.value.as_bool())
            .with_context(|| "thermostat: missing required DP 'switch'")?;

        let temp_current = get("temp_current")
            .and_then(|dp| dp.value.as_i64())
            .with_context(|| "thermostat: missing required DP 'temp_current'")?;

        let temp_set = get("temp_set")
            .and_then(|dp| dp.value.as_i64())
            .with_context(|| "thermostat: missing required DP 'temp_set'")?;

        let mode = get("mode")
            .and_then(|dp| dp.value.as_str())
            .with_context(|| "thermostat: missing required DP 'mode'")?
            .to_owned();

        Ok(Self {
            switch,
            temp_current,
            temp_set,
            mode,
            child_lock: get("child_lock").and_then(|dp| dp.value.as_bool()),
            fault: get("fault").and_then(|dp| dp.value.as_i64()),
            upper_temp: get("upper_temp").and_then(|dp| dp.value.as_i64()),
            temp_correction: get("temp_correction").and_then(|dp| dp.value.as_i64()),
            frost: get("frost").and_then(|dp| dp.value.as_bool()),
            sound: get("sound").and_then(|dp| dp.value.as_bool()),
        })
    }
}

// --- Energy meter (bfb5747d family) ----------------------------------------
//
// Observed DPs (device_status, v1 endpoint):
//   switch                bool
//   total_forward_energy  i64     Wh accumulated
//   phase_a/b/c           String  Base64 binary blobs (3-phase data)
//   fault                 i64     bitmask
//   switch_prepayment     bool
//   balance_energy        i64     Wh remaining (prepayment)
//   charge_energy         i64     Wh charged
//   leakage_current       i64     mA
//   reverse_energy_total  i64     Wh
//   temp_current          i64     °C  (scale ×1 on this device)
//   countdown_1           i64     seconds
//   alarm_set_1/2         String  Base64
//   cycle_time            String  Base64
//   random_time           String  Base64 or empty
//   energy_reset          String  empty string

/// Typed view of an energy meter device status.
#[derive(Debug, Clone)]
pub struct EnergyMeterStatus {
    pub switch: bool,
    /// Accumulated forward energy in Wh.
    pub total_forward_energy: i64,
    /// Base64-encoded 3-phase A data blob.
    pub phase_a: String,
    /// Base64-encoded 3-phase B data blob.
    pub phase_b: String,
    /// Base64-encoded 3-phase C data blob.
    pub phase_c: String,
    /// Fault bitmask — 0 means no fault.
    pub fault: Option<i64>,
    pub switch_prepayment: Option<bool>,
    pub balance_energy: Option<i64>,
    pub charge_energy: Option<i64>,
    /// Leakage current in mA.
    pub leakage_current: Option<i64>,
    pub reverse_energy_total: Option<i64>,
    /// Device temperature in °C (scale ×1, not ×10).
    pub temp_current: Option<i64>,
    pub countdown_1: Option<i64>,
    pub alarm_set_1: Option<String>,
    pub alarm_set_2: Option<String>,
    pub cycle_time: Option<String>,
    pub random_time: Option<String>,
    pub energy_reset: Option<String>,
}

impl TryFrom<&[DeviceProperty]> for EnergyMeterStatus {
    type Error = anyhow::Error;

    fn try_from(dps: &[DeviceProperty]) -> anyhow::Result<Self> {
        let get = |code: &str| dps.iter().find(|dp| dp.code == code);

        let switch = get("switch")
            .and_then(|dp| dp.value.as_bool())
            .with_context(|| "energy_meter: missing required DP 'switch'")?;

        let total_forward_energy = get("total_forward_energy")
            .and_then(|dp| dp.value.as_i64())
            .with_context(|| "energy_meter: missing required DP 'total_forward_energy'")?;

        let phase_a = get("phase_a")
            .and_then(|dp| dp.value.as_str())
            .with_context(|| "energy_meter: missing required DP 'phase_a'")?
            .to_owned();

        let phase_b = get("phase_b")
            .and_then(|dp| dp.value.as_str())
            .with_context(|| "energy_meter: missing required DP 'phase_b'")?
            .to_owned();

        let phase_c = get("phase_c")
            .and_then(|dp| dp.value.as_str())
            .with_context(|| "energy_meter: missing required DP 'phase_c'")?
            .to_owned();

        Ok(Self {
            switch,
            total_forward_energy,
            phase_a,
            phase_b,
            phase_c,
            fault: get("fault").and_then(|dp| dp.value.as_i64()),
            switch_prepayment: get("switch_prepayment").and_then(|dp| dp.value.as_bool()),
            balance_energy: get("balance_energy").and_then(|dp| dp.value.as_i64()),
            charge_energy: get("charge_energy").and_then(|dp| dp.value.as_i64()),
            leakage_current: get("leakage_current").and_then(|dp| dp.value.as_i64()),
            reverse_energy_total: get("reverse_energy_total").and_then(|dp| dp.value.as_i64()),
            temp_current: get("temp_current").and_then(|dp| dp.value.as_i64()),
            countdown_1: get("countdown_1").and_then(|dp| dp.value.as_i64()),
            alarm_set_1: get("alarm_set_1")
                .and_then(|dp| dp.value.as_str())
                .map(str::to_owned),
            alarm_set_2: get("alarm_set_2")
                .and_then(|dp| dp.value.as_str())
                .map(str::to_owned),
            cycle_time: get("cycle_time")
                .and_then(|dp| dp.value.as_str())
                .map(str::to_owned),
            random_time: get("random_time")
                .and_then(|dp| dp.value.as_str())
                .map(str::to_owned),
            energy_reset: get("energy_reset")
                .and_then(|dp| dp.value.as_str())
                .map(str::to_owned),
        })
    }
}

// --- Weather station (bf13e057 family) -------------------------------------
//
// Observed properties (shadow/properties, v2 endpoint):
//   local_temp   value  i64  208 = 20.8 °C  (÷10)
//   local_hum    value  i64   51 = 51 %
//   sub1_temp    value  i64  218 = 21.8 °C  (÷10)
//   sub1_hum     value  i64   45 = 45 %
//   sub2_temp    value  i64  173 = 17.3 °C  (÷10)
//   sub2_hum     value  i64   49 = 49 %
//   sub3_temp    value  i64   15 =  1.5 °C  (÷10)
//   sub3_hum     value  i64   75 = 75 %
//   temp_unit_convert  enum  "c" | "f"
//   (other DPs contain binary blobs or display settings — not mapped)

/// Typed view of a weather station shadow properties response.
#[derive(Debug, Clone)]
pub struct WeatherStationStatus {
    /// Raw value: 208 → 20.8 °C  (divide by 10 to get °C).
    pub local_temp: i64,
    /// Raw value: 51 → 51 %.
    pub local_hum: i64,
    /// Sub-sensor 1 temperature (raw ÷ 10 = °C).
    pub sub1_temp: Option<i64>,
    pub sub1_hum: Option<i64>,
    /// Sub-sensor 2 temperature (raw ÷ 10 = °C).
    pub sub2_temp: Option<i64>,
    pub sub2_hum: Option<i64>,
    /// Sub-sensor 3 temperature (raw ÷ 10 = °C).
    pub sub3_temp: Option<i64>,
    pub sub3_hum: Option<i64>,
    /// Temperature unit: `"c"` (Celsius) or `"f"` (Fahrenheit).
    pub temp_unit: Option<String>,
}

impl WeatherStationStatus {
    /// Local temperature in °C.
    pub fn local_temp_celsius(&self) -> f64 {
        self.local_temp as f64 / 10.0
    }

    /// Local relative humidity in percent.
    pub fn local_hum_pct(&self) -> f64 {
        self.local_hum as f64
    }
}

impl TryFrom<&[ShadowProperty]> for WeatherStationStatus {
    type Error = anyhow::Error;

    fn try_from(props: &[ShadowProperty]) -> anyhow::Result<Self> {
        let get = |code: &str| props.iter().find(|p| p.code == code);

        let local_temp = get("local_temp")
            .and_then(|p| p.value.as_i64())
            .with_context(|| "weather_station: missing required property 'local_temp'")?;

        let local_hum = get("local_hum")
            .and_then(|p| p.value.as_i64())
            .with_context(|| "weather_station: missing required property 'local_hum'")?;

        Ok(Self {
            local_temp,
            local_hum,
            sub1_temp: get("sub1_temp").and_then(|p| p.value.as_i64()),
            sub1_hum: get("sub1_hum").and_then(|p| p.value.as_i64()),
            sub2_temp: get("sub2_temp").and_then(|p| p.value.as_i64()),
            sub2_hum: get("sub2_hum").and_then(|p| p.value.as_i64()),
            sub3_temp: get("sub3_temp").and_then(|p| p.value.as_i64()),
            sub3_hum: get("sub3_hum").and_then(|p| p.value.as_i64()),
            temp_unit: get("temp_unit_convert")
                .and_then(|p| p.value.as_str())
                .map(str::to_owned),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- DpValue ------------------------------------------------------------

    #[test]
    fn dpvalue_bool_true_deserializes() {
        let v: DpValue = serde_json::from_str("true").unwrap();
        assert_eq!(v.as_bool(), Some(true));
        assert_eq!(v.as_i64(), None);
    }

    #[test]
    fn dpvalue_bool_false_deserializes() {
        let v: DpValue = serde_json::from_str("false").unwrap();
        assert_eq!(v.as_bool(), Some(false));
    }

    #[test]
    fn dpvalue_integer_deserializes() {
        let v: DpValue = serde_json::from_str("189").unwrap();
        assert_eq!(v.as_i64(), Some(189));
        assert_eq!(v.as_bool(), None);
    }

    #[test]
    fn dpvalue_negative_integer_deserializes() {
        let v: DpValue = serde_json::from_str("-22").unwrap();
        assert_eq!(v.as_i64(), Some(-22));
    }

    #[test]
    fn dpvalue_string_deserializes() {
        let v: DpValue = serde_json::from_str(r#""auto""#).unwrap();
        assert_eq!(v.as_str(), Some("auto"));
        assert_eq!(v.as_i64(), None);
        assert_eq!(v.as_bool(), None);
    }

    #[test]
    fn dpvalue_empty_string_deserializes() {
        let v: DpValue = serde_json::from_str(r#""""#).unwrap();
        assert_eq!(v.as_str(), Some(""));
    }

    // --- ThermostatStatus ---------------------------------------------------

    fn thermostat_dps() -> Vec<DeviceProperty> {
        serde_json::from_str(
            r#"[
            {"code":"switch","value":true},
            {"code":"temp_set","value":220},
            {"code":"temp_current","value":189},
            {"code":"mode","value":"auto"},
            {"code":"child_lock","value":false},
            {"code":"fault","value":0},
            {"code":"upper_temp","value":60},
            {"code":"temp_correction","value":-22},
            {"code":"frost","value":false},
            {"code":"sound","value":true}
        ]"#,
        )
        .unwrap()
    }

    #[test]
    fn thermostat_try_from_full_dps() {
        let dps = thermostat_dps();
        let s = ThermostatStatus::try_from(dps.as_slice()).unwrap();
        assert!(s.switch);
        assert_eq!(s.temp_current, 189);
        assert_eq!(s.temp_set, 220);
        assert_eq!(s.mode, "auto");
        assert_eq!(s.child_lock, Some(false));
        assert_eq!(s.fault, Some(0));
        assert_eq!(s.upper_temp, Some(60));
        assert_eq!(s.temp_correction, Some(-22));
        assert_eq!(s.frost, Some(false));
        assert_eq!(s.sound, Some(true));
    }

    #[test]
    fn thermostat_celsius_helpers() {
        let dps = thermostat_dps();
        let s = ThermostatStatus::try_from(dps.as_slice()).unwrap();
        assert!((s.temp_current_celsius() - 18.9).abs() < f64::EPSILON);
        assert!((s.temp_set_celsius() - 22.0).abs() < f64::EPSILON);
    }

    #[test]
    fn thermostat_missing_required_dp_errors() {
        // Missing 'switch'
        let dps: Vec<DeviceProperty> = serde_json::from_str(
            r#"[
            {"code":"temp_set","value":220},
            {"code":"temp_current","value":189},
            {"code":"mode","value":"auto"}
        ]"#,
        )
        .unwrap();
        let err = ThermostatStatus::try_from(dps.as_slice()).unwrap_err();
        assert!(err.to_string().contains("switch"));
    }

    // --- EnergyMeterStatus --------------------------------------------------

    fn energy_meter_dps() -> Vec<DeviceProperty> {
        serde_json::from_str(
            r#"[
            {"code":"switch","value":true},
            {"code":"total_forward_energy","value":531309},
            {"code":"phase_a","value":"CPAAAAAAAAA="},
            {"code":"phase_b","value":"COkAABUAAAE="},
            {"code":"phase_c","value":"COMAACEAAAY="},
            {"code":"fault","value":0},
            {"code":"switch_prepayment","value":false},
            {"code":"energy_reset","value":""},
            {"code":"balance_energy","value":0},
            {"code":"charge_energy","value":0},
            {"code":"leakage_current","value":0},
            {"code":"alarm_set_1","value":"BQEAVQQAAB4="},
            {"code":"alarm_set_2","value":"AQEDIAMBARMEAQCvAgAAFAUAAAA="},
            {"code":"temp_current","value":16},
            {"code":"countdown_1","value":0},
            {"code":"reverse_energy_total","value":0}
        ]"#,
        )
        .unwrap()
    }

    #[test]
    fn energy_meter_try_from_full_dps() {
        let dps = energy_meter_dps();
        let s = EnergyMeterStatus::try_from(dps.as_slice()).unwrap();
        assert!(s.switch);
        assert_eq!(s.total_forward_energy, 531309);
        assert_eq!(s.phase_a, "CPAAAAAAAAA=");
        assert_eq!(s.temp_current, Some(16));
        assert_eq!(s.fault, Some(0));
        assert_eq!(s.leakage_current, Some(0));
    }

    #[test]
    fn energy_meter_missing_phase_errors() {
        let dps: Vec<DeviceProperty> = serde_json::from_str(
            r#"[
            {"code":"switch","value":true},
            {"code":"total_forward_energy","value":100},
            {"code":"phase_a","value":"abc="}
        ]"#,
        )
        .unwrap();
        // Missing phase_b, phase_c
        let err = EnergyMeterStatus::try_from(dps.as_slice()).unwrap_err();
        assert!(err.to_string().contains("phase_b"));
    }

    // --- WeatherStationStatus -----------------------------------------------

    fn weather_props() -> Vec<ShadowProperty> {
        serde_json::from_str(r#"[
            {"code":"local_temp","dp_id":131,"time":1772132505450,"type":"value","value":208,"custom_name":""},
            {"code":"local_hum","dp_id":132,"time":1772132505460,"type":"value","value":51,"custom_name":""},
            {"code":"sub1_temp","dp_id":133,"time":1772132399469,"type":"value","value":218,"custom_name":""},
            {"code":"sub1_hum","dp_id":134,"time":1772132399480,"type":"value","value":45,"custom_name":""},
            {"code":"sub2_temp","dp_id":135,"time":1772132450733,"type":"value","value":173,"custom_name":""},
            {"code":"sub2_hum","dp_id":136,"time":1772132450744,"type":"value","value":49,"custom_name":""},
            {"code":"sub3_temp","dp_id":137,"time":1772130249524,"type":"value","value":15,"custom_name":""},
            {"code":"sub3_hum","dp_id":138,"time":1772132282997,"type":"value","value":75,"custom_name":""},
            {"code":"temp_unit_convert","dp_id":105,"time":1770157204538,"type":"enum","value":"c","custom_name":""}
        ]"#).unwrap()
    }

    #[test]
    fn weather_station_try_from_full_props() {
        let props = weather_props();
        let s = WeatherStationStatus::try_from(props.as_slice()).unwrap();
        assert_eq!(s.local_temp, 208);
        assert_eq!(s.local_hum, 51);
        assert_eq!(s.sub1_temp, Some(218));
        assert_eq!(s.sub1_hum, Some(45));
        assert_eq!(s.sub2_temp, Some(173));
        assert_eq!(s.sub2_hum, Some(49));
        assert_eq!(s.sub3_temp, Some(15));
        assert_eq!(s.sub3_hum, Some(75));
        assert_eq!(s.temp_unit.as_deref(), Some("c"));
    }

    #[test]
    fn weather_station_celsius_helpers() {
        let props = weather_props();
        let s = WeatherStationStatus::try_from(props.as_slice()).unwrap();
        assert!((s.local_temp_celsius() - 20.8).abs() < f64::EPSILON);
        assert!((s.local_hum_pct() - 51.0).abs() < f64::EPSILON);
    }

    #[test]
    fn weather_station_missing_local_temp_errors() {
        let props: Vec<ShadowProperty> = serde_json::from_str(
            r#"[
            {"code":"local_hum","dp_id":132,"time":0,"type":"value","value":51,"custom_name":""}
        ]"#,
        )
        .unwrap();
        let err = WeatherStationStatus::try_from(props.as_slice()).unwrap_err();
        assert!(err.to_string().contains("local_temp"));
    }

    #[test]
    fn weather_station_optional_sub_sensors_absent() {
        let props: Vec<ShadowProperty> = serde_json::from_str(
            r#"[
            {"code":"local_temp","dp_id":131,"time":0,"type":"value","value":200,"custom_name":""},
            {"code":"local_hum","dp_id":132,"time":0,"type":"value","value":60,"custom_name":""}
        ]"#,
        )
        .unwrap();
        let s = WeatherStationStatus::try_from(props.as_slice()).unwrap();
        assert_eq!(s.sub1_temp, None);
        assert_eq!(s.sub3_hum, None);
    }
}
