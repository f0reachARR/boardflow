pub mod config;
pub mod routes;

use axum::{Json, Router};
use sqlx::PgPool;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn create_app(pool: PgPool) -> Router {
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(routes::health::healthz))
        .split_for_parts();

    router
        .route(
            "/api/v1/openapi.json",
            axum::routing::get({
                let api = api.clone();
                move || async move { Json(api) }
            }),
        )
        .with_state(pool)
}

#[derive(OpenApi)]
#[openapi(info(title = "BoardFlow API", version = "0.1.0"))]
struct ApiDoc;
