use chrono::DateTime;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::AppError;
use crate::github_access::DynGithubAccessChecker;
use crate::pagination::{PaginatedResponse, PaginationParams, encode_cursor};
use crate::services::authz::ensure_repository_access;

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
pub struct ApiTokenDetailResponse {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
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

// ─── Service functions ───────────────────────────────────────────────────────

pub(crate) async fn execute_create_api_token(
    pool: &PgPool,
    access_checker: &DynGithubAccessChecker,
    github_access_token: &str,
    github_repository_id: i64,
    name: &str,
    request_id: &str,
) -> Result<CreateApiTokenResponse, AppError> {
    // Validate name
    let name = name.trim();
    if name.is_empty() || name.len() > 100 {
        return Err(AppError::validation_failed(
            "name must be between 1 and 100 characters",
            request_id,
        ));
    }

    // Lookup repository + access check
    let repo = ensure_repository_access(
        pool,
        access_checker,
        github_access_token,
        github_repository_id,
        request_id,
    )
    .await?;

    // Generate token
    let raw_token = generate_raw_token();
    let token_hash = hash_token(&raw_token);
    let id = Uuid::now_v7();

    let token = boardflow_db::queries::api_token::create(
        pool,
        id,
        repo.installation_id,
        repo.id,
        name,
        &token_hash,
    )
    .await
    .map_err(|e| {
        tracing::error!("create api_token failed: {e}");
        AppError::internal_error("database error", request_id)
    })?;

    Ok(CreateApiTokenResponse {
        id: token.id.to_string(),
        name: token.name,
        token: raw_token,
        created_at: token.created_at.to_rfc3339(),
    })
}

pub(crate) async fn execute_list_api_tokens(
    pool: &PgPool,
    access_checker: &DynGithubAccessChecker,
    github_access_token: &str,
    github_repository_id: i64,
    params: &PaginationParams,
    request_id: &str,
) -> Result<PaginatedResponse<ApiTokenListItem>, AppError> {
    // Lookup repository + access check
    let repo = ensure_repository_access(
        pool,
        access_checker,
        github_access_token,
        github_repository_id,
        request_id,
    )
    .await?;

    // Pagination
    let limit = params.effective_limit();
    let cursor = params.decoded_cursor(request_id)?;

    let tokens =
        boardflow_db::queries::api_token::list_by_repository_id(pool, repo.id, limit + 1, cursor)
            .await
            .map_err(|e| {
                tracing::error!("list api_tokens failed: {e}");
                AppError::internal_error("database error", request_id)
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

    Ok(PaginatedResponse {
        items,
        next_cursor,
        has_more,
    })
}

pub(crate) async fn execute_revoke_api_token(
    pool: &PgPool,
    access_checker: &DynGithubAccessChecker,
    github_access_token: &str,
    github_repository_id: i64,
    token_id_str: &str,
    request_id: &str,
) -> Result<ApiTokenDetailResponse, AppError> {
    let token_id = Uuid::parse_str(token_id_str)
        .map_err(|_| AppError::validation_failed("invalid token_id format", request_id))?;

    // Lookup repository + access check
    let repo = ensure_repository_access(
        pool,
        access_checker,
        github_access_token,
        github_repository_id,
        request_id,
    )
    .await?;

    // Revoke (idempotent: COALESCE keeps existing revoked_at)
    let token = boardflow_db::queries::api_token::revoke(pool, token_id, repo.id)
        .await
        .map_err(|e| {
            tracing::error!("revoke api_token failed: {e}");
            AppError::internal_error("database error", request_id)
        })?
        .ok_or_else(|| AppError::not_found("token not found", request_id))?;

    Ok(ApiTokenDetailResponse {
        id: token.id.to_string(),
        name: token.name,
        created_at: token.created_at.to_rfc3339(),
        last_used_at: token.last_used_at.map(|ts| ts.to_rfc3339()),
        revoked_at: token.revoked_at.map(|ts| ts.to_rfc3339()),
    })
}
