pub mod artifact_token;
pub mod config;
pub mod error;
pub mod extractors;
pub mod github_access;
pub mod middleware;
pub mod routes;

use std::sync::Arc;

use axum::{Extension, Json, Router};
use sqlx::PgPool;
use utoipa::openapi::security::{Http, HttpAuthScheme, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use github_access::{DynGithubAccessChecker, RealGithubAccessChecker};
use routes::auth::OAuthConfig;

use boardflow_config::{optional_env, optional_env_or};

pub fn create_app(pool: PgPool, s3_client: Option<aws_sdk_s3::Client>) -> Router {
    create_app_with_config(
        pool, s3_client, None, None, None, None, None, None, None, None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn create_app_with_config(
    pool: PgPool,
    s3_client: Option<aws_sdk_s3::Client>,
    oauth_config: Option<OAuthConfig>,
    artifact_secret: Option<Vec<u8>>,
    access_checker: Option<DynGithubAccessChecker>,
    final_bucket: Option<String>,
    staging_bucket: Option<String>,
    app_domain: Option<String>,
    artifact_base_url: Option<String>,
    webhook_secret: Option<String>,
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
        .routes(routes!(routes::read::get_board_run_diff))
        .routes(routes!(routes::read::list_findings))
        .routes(routes!(routes::auth::login))
        .routes(routes!(routes::auth::callback))
        .routes(routes!(routes::auth::logout))
        .routes(routes!(routes::auth::me))
        .routes(routes!(routes::api_token::create_api_token))
        .routes(routes!(routes::api_token::list_api_tokens))
        .routes(routes!(routes::api_token::revoke_api_token))
        .split_for_parts();

    let oauth = oauth_config.unwrap_or_else(|| OAuthConfig {
        client_id: optional_env("GITHUB_CLIENT_ID").unwrap_or_default(),
        client_secret: optional_env("GITHUB_CLIENT_SECRET").unwrap_or_default(),
    });

    let secret = artifact_secret.unwrap_or_else(|| {
        boardflow_config::required_env("BOARDFLOW_ARTIFACT_SECRET")
            .expect("BOARDFLOW_ARTIFACT_SECRET must be set")
            .into_bytes()
    });

    let checker: DynGithubAccessChecker =
        access_checker.unwrap_or_else(|| Arc::new(RealGithubAccessChecker::new()));

    let bucket = FinalBucket(final_bucket.unwrap_or_else(|| {
        optional_env_or("MINIO_BUCKET_FINAL", "boardflow-final")
    }));

    let staging = StagingBucket(staging_bucket.unwrap_or_else(|| {
        optional_env_or("MINIO_BUCKET_STAGING", "boardflow-staging")
    }));

    let domain = AppDomain(app_domain.unwrap_or_else(|| {
        optional_env_or("BOARDFLOW_APP_DOMAIN", "http://localhost:3000")
    }));

    let base_url = ArtifactBaseUrl(artifact_base_url.unwrap_or_else(|| {
        optional_env_or("BOARDFLOW_ARTIFACT_BASE_URL", "http://localhost:8080")
    }));

    router
        .route(
            "/api/v1/openapi.json",
            axum::routing::get({
                let api = api.clone();
                move || async move { Json(api) }
            }),
        )
        .route(
            "/proxy/artifacts/{artifact_id}",
            axum::routing::get(routes::proxy::get_artifact),
        )
        .route(
            "/api/v1/github/webhook",
            axum::routing::post(routes::webhook::github_webhook),
        )
        .layer(Extension(pool.clone()))
        .layer(Extension(WebhookSecret(webhook_secret)))
        .layer(Extension(s3_client))
        .layer(Extension(oauth))
        .layer(Extension(ArtifactSecret(secret)))
        .layer(Extension(bucket))
        .layer(Extension(staging))
        .layer(Extension(domain))
        .layer(Extension(base_url))
        .layer(Extension(checker))
        .layer(axum::middleware::from_fn(
            middleware::request_id::request_id_middleware,
        ))
        .with_state(pool)
}

#[derive(Clone)]
pub struct ArtifactSecret(pub Vec<u8>);

#[derive(Clone)]
pub struct WebhookSecret(pub Option<String>);

#[derive(Clone)]
pub struct FinalBucket(pub String);

#[derive(Clone)]
pub struct StagingBucket(pub String);

#[derive(Clone)]
pub struct AppDomain(pub String);

#[derive(Clone)]
pub struct ArtifactBaseUrl(pub String);

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
