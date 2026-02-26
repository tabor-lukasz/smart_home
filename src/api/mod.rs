pub mod dto;
pub mod errors;
pub mod handlers;

use axum::{routing::get, Router};
use sqlx::PgPool;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

use handlers::ApiDoc;

pub fn router(pool: PgPool) -> Router {
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .route("/sensors/latest", get(handlers::get_latest_readings))
        .route("/sensors/{device_id}", get(handlers::get_device_readings))
        .with_state(pool)
        .split_for_parts();

    router.route(
        "/api-docs/openapi.json",
        get(move || async move { axum::Json(api) }),
    )
}
