/// Saves raw Tuya API response bytes to `responses/{endpoint}/{timestamp}_{suffix}.json`
/// for offline analysis.
///
/// Errors are logged and swallowed — saving is best-effort and must never
/// interrupt normal application flow.
use tokio::fs;
use tracing::warn;

/// Write `bytes` to `responses/{endpoint}/{timestamp}_{suffix}.json`.
///
/// - `endpoint`: used as the sub-directory name, e.g. `"token"` or `"device_status"`.
/// - `suffix`: appended after the timestamp, e.g. a device ID. Pass `""` to omit.
/// - `bytes`: the raw HTTP response body as received from Tuya.
pub async fn save(endpoint: &str, suffix: &str, bytes: &[u8]) {
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let filename = if suffix.is_empty() {
        format!("{ts}.json")
    } else {
        format!("{ts}_{suffix}.json")
    };

    let dir = format!("responses/{endpoint}");
    let path = format!("{dir}/{filename}");

    if let Err(e) = fs::create_dir_all(&dir).await {
        warn!(path = %path, error = %e, "response_store: failed to create directory");
        return;
    }

    // Pretty-print the JSON if valid; fall back to raw bytes otherwise.
    let content = match serde_json::from_slice::<serde_json::Value>(bytes) {
        Ok(v) => match serde_json::to_vec_pretty(&v) {
            Ok(pretty) => pretty,
            Err(_) => bytes.to_vec(),
        },
        Err(_) => bytes.to_vec(),
    };

    if let Err(e) = fs::write(&path, &content).await {
        warn!(path = %path, error = %e, "response_store: failed to write response file");
    } else {
        tracing::debug!(path = %path, bytes = content.len(), "response_store: saved");
    }
}
