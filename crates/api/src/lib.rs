pub mod config;
pub mod error;
pub mod extractors;
pub mod middleware;
pub mod routes;

use axum::{Extension, Json, Router};
use sqlx::PgPool;
use utoipa::openapi::security::{Http, HttpAuthScheme, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn create_app(pool: PgPool, s3_client: Option<aws_sdk_s3::Client>) -> Router {
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(routes::health::healthz))
        .routes(routes!(routes::plan::plan_run))
        .routes(routes!(routes::board_run::create_board_run))
        .routes(routes!(routes::board_run::fail_board_run))
        .routes(routes!(routes::board_run::import_artifact_bundle))
        .split_for_parts();

    router
        .route(
            "/api/v1/openapi.json",
            axum::routing::get({
                let api = api.clone();
                move || async move { Json(api) }
            }),
        )
        .layer(Extension(s3_client))
        .layer(axum::middleware::from_fn(
            middleware::request_id::request_id_middleware,
        ))
        .with_state(pool)
}

#[derive(OpenApi)]
#[openapi(
    info(title = "BoardFlow API", version = "0.1.0"),
    modifiers(&SecurityAddon)
)]
struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
            );
        }
    }
}
