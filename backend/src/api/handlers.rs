use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::OpenApi;

use super::{dto::SensorReadingDto, errors::AppError};
use crate::db::models::{SensorReading, SensorType};

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TimeRangeParams {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Fetch the latest reading for every known `(device_id, sensor_type)` pair.
#[utoipa::path(
    get,
    path = "/sensors/latest",
    responses(
        (status = 200, description = "Latest reading per (device_id, sensor_type)", body = Vec<SensorReadingDto>),
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
        SELECT DISTINCT ON (device_id, sensor_type)
            id,
            device_id,
            sensor_type AS "sensor_type: SensorType",
            recorded_at,
            value
        FROM sensor_readings
        ORDER BY device_id, sensor_type, recorded_at DESC
        "#
    )
    .fetch_all(&pool)
    .await?;

    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

/// Fetch time-series readings for a specific device and sensor type.
/// Optimised for scatter charts. Optionally filter by time range with
/// `?from=<RFC3339>&to=<RFC3339>`. Results are ordered by `recorded_at ASC`.
#[utoipa::path(
    get,
    path = "/sensors/{device_id}/{sensor_type}",
    params(
        ("device_id" = String, Path, description = "Tuya device ID"),
        ("sensor_type" = SensorType, Path, description = "Sensor type"),
        ("from" = Option<DateTime<Utc>>, Query, description = "Start of time range (RFC3339)"),
        ("to"   = Option<DateTime<Utc>>, Query, description = "End of time range (RFC3339)"),
    ),
    responses(
        (status = 200, description = "Sensor readings", body = Vec<SensorReadingDto>),
        (status = 500, description = "Internal server error"),
    ),
    tag = "sensors"
)]
pub async fn get_sensor_readings(
    State(pool): State<PgPool>,
    Path((device_id, sensor_type)): Path<(String, SensorType)>,
    Query(params): Query<TimeRangeParams>,
) -> Result<Json<Vec<SensorReadingDto>>, AppError> {
    let rows = sqlx::query_as!(
        SensorReading,
        r#"
        SELECT id,
               device_id,
               sensor_type AS "sensor_type: SensorType",
               recorded_at,
               value
        FROM sensor_readings
        WHERE device_id   = $1
          AND sensor_type = $2
          AND ($3::timestamptz IS NULL OR recorded_at >= $3)
          AND ($4::timestamptz IS NULL OR recorded_at <= $4)
        ORDER BY recorded_at ASC
        "#,
        device_id,
        sensor_type as SensorType,
        params.from,
        params.to,
    )
    .fetch_all(&pool)
    .await?;

    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

/// Fetch the single latest reading for a specific device and sensor type.
#[utoipa::path(
    get,
    path = "/sensors/{device_id}/{sensor_type}/latest",
    params(
        ("device_id"   = String,     Path, description = "Tuya device ID"),
        ("sensor_type" = SensorType, Path, description = "Sensor type"),
    ),
    responses(
        (status = 200, description = "Latest sensor reading", body = SensorReadingDto),
        (status = 404, description = "No reading found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "sensors"
)]
pub async fn get_sensor_latest(
    State(pool): State<PgPool>,
    Path((device_id, sensor_type)): Path<(String, SensorType)>,
) -> Result<Json<Option<SensorReadingDto>>, AppError> {
    let row = sqlx::query_as!(
        SensorReading,
        r#"
        SELECT id,
               device_id,
               sensor_type AS "sensor_type: SensorType",
               recorded_at,
               value
        FROM sensor_readings
        WHERE device_id   = $1
          AND sensor_type = $2
        ORDER BY recorded_at DESC
        LIMIT 1
        "#,
        device_id,
        sensor_type as SensorType,
    )
    .fetch_optional(&pool)
    .await?;

    Ok(Json(row.map(Into::into)))
}

// ---------------------------------------------------------------------------
// Health check
// ---------------------------------------------------------------------------

/// Returns `200 OK` with `{"status":"ok"}` when the server is running.
#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Service is healthy"),
    ),
    tag = "system"
)]
pub async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "status": "ok" }))
}

// ---------------------------------------------------------------------------
// OpenAPI spec
// ---------------------------------------------------------------------------

#[derive(OpenApi)]
#[openapi(
    paths(get_latest_readings, get_sensor_readings, get_sensor_latest, health),
    components(schemas(SensorReadingDto, SensorType)),
    tags(
        (name = "sensors", description = "Sensor reading endpoints"),
        (name = "system",  description = "System endpoints"),
    ),
    info(
        title = "Smart Home Backend API",
        version = "0.1.0",
        description = "REST API for smart home sensor data"
    )
)]
pub struct ApiDoc;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use axum_test::TestServer;
    use serde_json::Value;
    use sqlx::PgPool;

    use crate::api::router;

    fn test_server(pool: PgPool) -> TestServer {
        TestServer::new(router(pool)).unwrap()
    }

    async fn insert_reading(pool: &PgPool, device_id: &str, sensor_type: &str, value: i64) {
        sqlx::query(
            "INSERT INTO sensor_readings (device_id, sensor_type, value) \
             VALUES ($1, $2::sensor_type, $3)",
        )
        .bind(device_id)
        .bind(sensor_type)
        .bind(value)
        .execute(pool)
        .await
        .unwrap();
    }

    // -----------------------------------------------------------------------
    // GET /sensors/latest
    // -----------------------------------------------------------------------

    #[sqlx::test(migrations = "./migrations")]
    async fn latest_empty_returns_empty_array(pool: PgPool) {
        let server = test_server(pool);
        let resp = server.get("/sensors/latest").await;
        resp.assert_status_ok();
        let body: Value = resp.json();
        assert_eq!(body, serde_json::json!([]));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn latest_returns_most_recent_per_device_and_type(pool: PgPool) {
        insert_reading(&pool, "dev1", "temperature", 2000).await;
        insert_reading(&pool, "dev1", "temperature", 2500).await;
        insert_reading(&pool, "dev1", "humidity", 6000).await;

        let server = test_server(pool);
        let resp = server.get("/sensors/latest").await;
        resp.assert_status_ok();

        let body: Vec<Value> = resp.json();
        assert_eq!(body.len(), 2);

        let temp = body.iter().find(|r| r["sensor_type"] == "temperature").unwrap();
        assert_eq!(temp["device_id"], "dev1");
        assert_eq!(temp["value"], 2500);

        let hum = body.iter().find(|r| r["sensor_type"] == "humidity").unwrap();
        assert_eq!(hum["value"], 6000);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn latest_returns_one_entry_per_device_type_combination(pool: PgPool) {
        insert_reading(&pool, "dev1", "temperature", 2000).await;
        insert_reading(&pool, "dev2", "temperature", 3000).await;

        let server = test_server(pool);
        let resp = server.get("/sensors/latest").await;
        resp.assert_status_ok();

        let body: Vec<Value> = resp.json();
        assert_eq!(body.len(), 2);
    }

    // -----------------------------------------------------------------------
    // GET /sensors/{device_id}/{sensor_type}
    // -----------------------------------------------------------------------

    #[sqlx::test(migrations = "./migrations")]
    async fn sensor_readings_empty_for_unknown_device(pool: PgPool) {
        let server = test_server(pool);
        let resp = server.get("/sensors/unknown/temperature").await;
        resp.assert_status_ok();
        let body: Value = resp.json();
        assert_eq!(body, serde_json::json!([]));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn sensor_readings_returns_all_in_asc_order(pool: PgPool) {
        insert_reading(&pool, "dev1", "temperature", 2000).await;
        insert_reading(&pool, "dev1", "temperature", 2100).await;
        insert_reading(&pool, "dev1", "temperature", 2200).await;

        let server = test_server(pool);
        let resp = server.get("/sensors/dev1/temperature").await;
        resp.assert_status_ok();

        let body: Vec<Value> = resp.json();
        assert_eq!(body.len(), 3);
        assert!(
            body[0]["recorded_at"].as_str().unwrap()
                <= body[1]["recorded_at"].as_str().unwrap()
        );
        assert!(
            body[1]["recorded_at"].as_str().unwrap()
                <= body[2]["recorded_at"].as_str().unwrap()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn sensor_readings_filters_by_sensor_type(pool: PgPool) {
        insert_reading(&pool, "dev1", "temperature", 2000).await;
        insert_reading(&pool, "dev1", "humidity", 6000).await;

        let server = test_server(pool);
        let resp = server.get("/sensors/dev1/humidity").await;
        resp.assert_status_ok();

        let body: Vec<Value> = resp.json();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0]["sensor_type"], "humidity");
        assert_eq!(body[0]["value"], 6000);
    }

    // -----------------------------------------------------------------------
    // GET /sensors/{device_id}/{sensor_type}/latest
    // -----------------------------------------------------------------------

    #[sqlx::test(migrations = "./migrations")]
    async fn sensor_latest_returns_null_when_no_readings(pool: PgPool) {
        let server = test_server(pool);
        let resp = server.get("/sensors/unknown/temperature/latest").await;
        resp.assert_status_ok();
        let body: Value = resp.json();
        assert!(body.is_null());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn sensor_latest_returns_most_recent(pool: PgPool) {
        insert_reading(&pool, "dev1", "temperature", 2000).await;
        insert_reading(&pool, "dev1", "temperature", 2999).await;

        let server = test_server(pool);
        let resp = server.get("/sensors/dev1/temperature/latest").await;
        resp.assert_status_ok();

        let body: Value = resp.json();
        assert_eq!(body["value"], 2999);
        assert_eq!(body["device_id"], "dev1");
        assert_eq!(body["sensor_type"], "temperature");
    }

    // -----------------------------------------------------------------------
    // GET /health
    // -----------------------------------------------------------------------

    #[sqlx::test(migrations = "./migrations")]
    async fn health_returns_ok(pool: PgPool) {
        let server = test_server(pool);
        let resp = server.get("/health").await;
        resp.assert_status_ok();
        let body: Value = resp.json();
        assert_eq!(body["status"], "ok");
    }

    // -----------------------------------------------------------------------
    // GET /api-docs/openapi.json
    // -----------------------------------------------------------------------

    #[sqlx::test(migrations = "./migrations")]
    async fn openapi_spec_is_served(pool: PgPool) {
        let server = test_server(pool);
        let resp = server.get("/api-docs/openapi.json").await;
        resp.assert_status_ok();
        let body: Value = resp.json();
        assert_eq!(body["info"]["title"], "Smart Home Backend API");
    }
}
