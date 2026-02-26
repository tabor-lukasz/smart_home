use axum::{
    extract::{Path, State},
    Json,
};
use sqlx::PgPool;
use utoipa::OpenApi;

use super::{dto::SensorReadingDto, errors::AppError};
use crate::db::models::SensorReading;

/// Fetch the latest reading for every known device (one row per device).
#[utoipa::path(
    get,
    path = "/sensors/latest",
    responses(
        (status = 200, description = "Latest sensor readings", body = Vec<SensorReadingDto>),
        (status = 500, description = "Internal server error"),
    ),
    tag = "sensors"
)]
pub async fn get_latest_readings(
    State(pool): State<PgPool>,
) -> Result<Json<Vec<SensorReadingDto>>, AppError> {
    let rows = sqlx::query_as!(
        SensorReading,
        r#"
        SELECT DISTINCT ON (device_id)
            id, device_id, recorded_at,
            temperature, humidity, door_open,
            power_consumption, relay_state, temperature_setpoint
        FROM sensor_readings
        ORDER BY device_id, recorded_at DESC
        "#
    )
    .fetch_all(&pool)
    .await?;

    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

/// Fetch the N most recent readings for a specific device.
#[utoipa::path(
    get,
    path = "/sensors/{device_id}",
    params(
        ("device_id" = String, Path, description = "Tuya device ID"),
    ),
    responses(
        (status = 200, description = "Device sensor readings", body = Vec<SensorReadingDto>),
        (status = 500, description = "Internal server error"),
    ),
    tag = "sensors"
)]
pub async fn get_device_readings(
    State(pool): State<PgPool>,
    Path(device_id): Path<String>,
) -> Result<Json<Vec<SensorReadingDto>>, AppError> {
    let rows = sqlx::query_as!(
        SensorReading,
        r#"
        SELECT id, device_id, recorded_at,
               temperature, humidity, door_open,
               power_consumption, relay_state, temperature_setpoint
        FROM sensor_readings
        WHERE device_id = $1
        ORDER BY recorded_at DESC
        LIMIT 100
        "#,
        device_id
    )
    .fetch_all(&pool)
    .await?;

    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

// ---------------------------------------------------------------------------
// OpenAPI spec struct (used in api/mod.rs)
// ---------------------------------------------------------------------------

#[derive(OpenApi)]
#[openapi(
    paths(get_latest_readings, get_device_readings),
    components(schemas(SensorReadingDto)),
    tags((name = "sensors", description = "Sensor reading endpoints")),
    info(
        title = "Smart Home Backend API",
        version = "0.1.0",
        description = "REST API for smart home sensor data"
    )
)]
pub struct ApiDoc;
