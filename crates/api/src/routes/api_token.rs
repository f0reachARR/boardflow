use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::error::{AppError, RequestId};
use crate::extractors::AuthenticatedSession;
use crate::github_access::DynGithubAccessChecker;
use crate::routes::read::access_result_to_error;

// ─── Request / Response types ────────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateApiTokenRequest {
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateApiTokenResponse {
    pub id: String,
    pub name: String,
    pub token: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiTokenListItem {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiTokenListResponse {
    pub items: Vec<ApiTokenListItem>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiTokenDetailResponse {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ApiTokenPaginationParams {
    #[param(default = 50, minimum = 1, maximum = 100)]
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

// ─── Cursor helpers ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct CursorPayload {
    ts: String,
    id: String,
}

fn encode_cursor(ts: &DateTime<Utc>, id: &Uuid) -> String {
    let payload = CursorPayload {
        ts: ts.to_rfc3339(),
        id: id.to_string(),
    };
    let json = serde_json::to_string(&payload).unwrap();
    URL_SAFE_NO_PAD.encode(json.as_bytes())
}

fn decode_cursor(cursor: &str) -> Option<(DateTime<Utc>, Uuid)> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let payload: CursorPayload = serde_json::from_slice(&bytes).ok()?;
    let ts = DateTime::parse_from_rfc3339(&payload.ts).ok()?.to_utc();
    let id = Uuid::parse_str(&payload.id).ok()?;
    Some((ts, id))
}

// ─── Token generation ────────────────────────────────────────────────────────

fn generate_raw_token() -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes);
    format!("bft_{}", hex::encode(bytes))
}

fn hash_token(raw_token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_token.as_bytes());
    hex::encode(hasher.finalize())
}

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

    // Validate name
    let name = body.name.trim();
    if name.is_empty() || name.len() > 100 {
        return Err(AppError::validation_failed(
            "name must be between 1 and 100 characters",
            &request_id,
        ));
    }

    // Lookup repository
    let repo = boardflow_db::queries::repository::find_by_github_id(&pool, github_repository_id)
        .await
        .map_err(|e| {
            tracing::error!("find_by_github_id failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("repository not found", &request_id))?;

    // Access check
    let result = access_checker
        .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
        .await;
    if let Some(err) = access_result_to_error(&result, "repository not found", &request_id) {
        return Err(err);
    }

    // Generate token
    let raw_token = generate_raw_token();
    let token_hash = hash_token(&raw_token);
    let id = Uuid::now_v7();

    let token = boardflow_db::queries::api_token::create(
        &pool,
        id,
        repo.installation_id,
        repo.id,
        name,
        &token_hash,
    )
    .await
    .map_err(|e| {
        tracing::error!("create api_token failed: {e}");
        AppError::internal_error("database error", &request_id)
    })?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(CreateApiTokenResponse {
            id: token.id.to_string(),
            name: token.name,
            token: raw_token,
            created_at: token.created_at.to_rfc3339(),
        }),
    ))
}

// ─── GET /api/v1/repositories/{github_repository_id}/api-tokens ──────────────

#[utoipa::path(
    get,
    path = "/api/v1/repositories/{github_repository_id}/api-tokens",
    params(
        ("github_repository_id" = i64, Path, description = "GitHub repository ID"),
        ApiTokenPaginationParams
    ),
    responses(
        (status = 200, description = "Token list", body = ApiTokenListResponse),
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
    Query(params): Query<ApiTokenPaginationParams>,
) -> Result<Json<ApiTokenListResponse>, AppError> {
    // Lookup repository
    let repo = boardflow_db::queries::repository::find_by_github_id(&pool, github_repository_id)
        .await
        .map_err(|e| {
            tracing::error!("find_by_github_id failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("repository not found", &request_id))?;

    // Access check
    let result = access_checker
        .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
        .await;
    if let Some(err) = access_result_to_error(&result, "repository not found", &request_id) {
        return Err(err);
    }

    // Pagination
    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let cursor = match &params.cursor {
        None => None,
        Some(c) => Some(
            decode_cursor(c)
                .ok_or_else(|| AppError::validation_failed("invalid cursor", &request_id))?,
        ),
    };

    let tokens =
        boardflow_db::queries::api_token::list_by_repository_id(&pool, repo.id, limit + 1, cursor)
            .await
            .map_err(|e| {
                tracing::error!("list api_tokens failed: {e}");
                AppError::internal_error("database error", &request_id)
            })?;

    let has_more = tokens.len() as i64 > limit;
    let items: Vec<ApiTokenListItem> = tokens
        .into_iter()
        .take(limit as usize)
        .map(|t| ApiTokenListItem {
            id: t.id.to_string(),
            name: t.name,
            created_at: t.created_at.to_rfc3339(),
            last_used_at: t.last_used_at.map(|ts| ts.to_rfc3339()),
            revoked_at: t.revoked_at.map(|ts| ts.to_rfc3339()),
        })
        .collect();

    let next_cursor = if has_more {
        items.last().map(|item| {
            let ts = DateTime::parse_from_rfc3339(&item.created_at)
                .unwrap()
                .to_utc();
            let id = Uuid::parse_str(&item.id).unwrap();
            encode_cursor(&ts, &id)
        })
    } else {
        None
    };

    Ok(Json(ApiTokenListResponse {
        items,
        next_cursor,
        has_more,
    }))
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
    let token_id = Uuid::parse_str(&token_id_str)
        .map_err(|_| AppError::validation_failed("invalid token_id format", &request_id))?;

    // Lookup repository
    let repo = boardflow_db::queries::repository::find_by_github_id(&pool, github_repository_id)
        .await
        .map_err(|e| {
            tracing::error!("find_by_github_id failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("repository not found", &request_id))?;

    // Access check
    let result = access_checker
        .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
        .await;
    if let Some(err) = access_result_to_error(&result, "repository not found", &request_id) {
        return Err(err);
    }

    // Revoke (idempotent: COALESCE keeps existing revoked_at)
    let token = boardflow_db::queries::api_token::revoke(&pool, token_id, repo.id)
        .await
        .map_err(|e| {
            tracing::error!("revoke api_token failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("token not found", &request_id))?;

    Ok(Json(ApiTokenDetailResponse {
        id: token.id.to_string(),
        name: token.name,
        created_at: token.created_at.to_rfc3339(),
        last_used_at: token.last_used_at.map(|ts| ts.to_rfc3339()),
        revoked_at: token.revoked_at.map(|ts| ts.to_rfc3339()),
    }))
}
