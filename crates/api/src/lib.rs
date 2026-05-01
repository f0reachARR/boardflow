pub mod artifact_token;
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

use routes::auth::OAuthConfig;

pub fn create_app(pool: PgPool, s3_client: Option<aws_sdk_s3::Client>) -> Router {
    create_app_with_config(pool, s3_client, None, None)
}

pub fn create_app_with_config(
    pool: PgPool,
    s3_client: Option<aws_sdk_s3::Client>,
    oauth_config: Option<OAuthConfig>,
    artifact_secret: Option<Vec<u8>>,
) -> Router {
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(routes::health::healthz))
        .routes(routes!(routes::plan::plan_run))
        .routes(routes!(routes::board_run::create_board_run))
        .routes(routes!(routes::board_run::fail_board_run))
        .routes(routes!(routes::board_run::import_artifact_bundle))
        .routes(routes!(routes::read::list_repositories))
        .routes(routes!(routes::read::get_repository))
        .routes(routes!(routes::read::list_board_projects))
        .routes(routes!(routes::read::get_board_project))
        .routes(routes!(routes::read::list_board_runs))
        .routes(routes!(routes::read::get_board_run))
        .routes(routes!(routes::read::list_artifacts))
        .routes(routes!(routes::read::get_viewer_sources))
        .routes(routes!(routes::auth::login))
        .routes(routes!(routes::auth::callback))
        .routes(routes!(routes::auth::logout))
        .routes(routes!(routes::auth::me))
        .split_for_parts();

    let oauth = oauth_config.unwrap_or_else(|| OAuthConfig {
        client_id: std::env::var("GITHUB_CLIENT_ID").unwrap_or_default(),
        client_secret: std::env::var("GITHUB_CLIENT_SECRET").unwrap_or_default(),
    });

    let secret = artifact_secret.unwrap_or_else(|| {
        std::env::var("BOARDFLOW_ARTIFACT_SECRET")
            .unwrap_or_else(|_| "default-dev-secret".to_string())
            .into_bytes()
    });

    router
        .route(
            "/api/v1/openapi.json",
            axum::routing::get({
                let api = api.clone();
                move || async move { Json(api) }
            }),
        )
        .layer(Extension(s3_client))
        .layer(Extension(oauth))
        .layer(Extension(ArtifactSecret(secret)))
        .layer(axum::middleware::from_fn(
            middleware::request_id::request_id_middleware,
        ))
        .with_state(pool)
}

#[derive(Clone)]
pub struct ArtifactSecret(pub Vec<u8>);

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
