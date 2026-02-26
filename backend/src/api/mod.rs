pub mod dto;
pub mod errors;
pub mod handlers;

use axum::{routing::{get, post}, Router};
use sqlx::PgPool;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

use handlers::ApiDoc;

pub fn router(pool: PgPool) -> Router {
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .route("/sensors/latest", get(handlers::get_latest_readings))
        .route("/sensors/readings", post(handlers::get_readings_multi))
        .route(
            "/sensors/{device_id}/{sensor_type}",
            get(handlers::get_sensor_readings),
        )
        .route(
            "/sensors/{device_id}/{sensor_type}/latest",
            get(handlers::get_sensor_latest),
        )
        .with_state(pool)
        .split_for_parts();

    router
        .route("/health", get(handlers::health))
        .route(
            "/api-docs/openapi.json",
            get(move || async move { axum::Json(api) }),
        )
}
