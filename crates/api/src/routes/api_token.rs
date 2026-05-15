use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use sqlx::PgPool;

use crate::error::{AppError, RequestId};
use crate::extractors::AuthenticatedSession;
use crate::github_access::DynGithubAccessChecker;
use crate::pagination::{PaginatedResponse, PaginationParams};
use crate::services::api_token::{
    ApiTokenDetailResponse, ApiTokenListItem, CreateApiTokenRequest, CreateApiTokenResponse,
};

// ─── POST /api/v1/repositories/{github_repository_id}/api-tokens ─────────────

#[utoipa::path(
    post,
    path = "/api/v1/repositories/{github_repository_id}/api-tokens",
    params(("github_repository_id" = i64, Path, description = "GitHub repository ID")),
    request_body = CreateApiTokenRequest,
    responses(
        (status = 201, description = "Token created", body = CreateApiTokenResponse),
        (status = 400, description = "Validation error", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    )
)]
pub async fn create_api_token(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Path(github_repository_id): Path<i64>,
    payload: Result<Json<CreateApiTokenRequest>, JsonRejection>,
) -> Result<(axum::http::StatusCode, Json<CreateApiTokenResponse>), AppError> {
    let Json(body) =
        payload.map_err(|e| AppError::validation_failed(e.body_text(), &request_id))?;
    let response = crate::services::api_token::execute_create_api_token(
        &pool,
        &access_checker,
        &session.user.github_access_token,
        github_repository_id,
        &body.name,
        &request_id,
    )
    .await?;
    Ok((axum::http::StatusCode::CREATED, Json(response)))
}

// ─── GET /api/v1/repositories/{github_repository_id}/api-tokens ──────────────

#[utoipa::path(
    get,
    path = "/api/v1/repositories/{github_repository_id}/api-tokens",
    params(
        ("github_repository_id" = i64, Path, description = "GitHub repository ID"),
        PaginationParams
    ),
    responses(
        (status = 200, description = "Token list", body = PaginatedResponse<ApiTokenListItem>),
        (status = 400, description = "Validation error", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    )
)]
pub async fn list_api_tokens(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Path(github_repository_id): Path<i64>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<ApiTokenListItem>>, AppError> {
    let response = crate::services::api_token::execute_list_api_tokens(
        &pool,
        &access_checker,
        &session.user.github_access_token,
        github_repository_id,
        &params,
        &request_id,
    )
    .await?;
    Ok(Json(response))
}

// ─── POST /api/v1/repositories/{github_repository_id}/api-tokens/{token_id}/revoke ──

#[utoipa::path(
    post,
    path = "/api/v1/repositories/{github_repository_id}/api-tokens/{token_id}/revoke",
    params(
        ("github_repository_id" = i64, Path, description = "GitHub repository ID"),
        ("token_id" = String, Path, description = "API token ID"),
    ),
    responses(
        (status = 200, description = "Token revoked", body = ApiTokenDetailResponse),
        (status = 400, description = "Validation error", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    )
)]
pub async fn revoke_api_token(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Path((github_repository_id, token_id_str)): Path<(i64, String)>,
) -> Result<Json<ApiTokenDetailResponse>, AppError> {
    let response = crate::services::api_token::execute_revoke_api_token(
        &pool,
        &access_checker,
        &session.user.github_access_token,
        github_repository_id,
        &token_id_str,
        &request_id,
    )
    .await?;
    Ok(Json(response))
}
