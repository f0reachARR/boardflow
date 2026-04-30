use axum::extract::{FromRef, FromRequestParts};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use boardflow_domain::models::api_token::BoardflowApiToken;

use crate::error::{AppError, RequestId};

pub struct AuthenticatedToken(pub BoardflowApiToken);

impl<S> FromRequestParts<S> for AuthenticatedToken
where
    S: Send + Sync,
    PgPool: axum::extract::FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let request_id = parts
            .extensions
            .get::<RequestId>()
            .map(|r| r.0.clone())
            .unwrap_or_default();

        let pool = PgPool::from_ref(state);

        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                AppError::unauthorized("missing authorization header", &request_id)
            })?;

        let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
            AppError::unauthorized("invalid authorization header format", &request_id)
        })?;

        if token.is_empty() {
            return Err(AppError::unauthorized("empty bearer token", &request_id));
        }

        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        let hash = format!("{:x}", hasher.finalize());

        let api_token = boardflow_db::queries::api_token::find_by_hash(&pool, &hash)
            .await
            .map_err(|_| {
                AppError::internal_error("authentication service error", &request_id)
            })?
            .ok_or_else(|| AppError::unauthorized("invalid token", &request_id))?;

        if api_token.revoked_at.is_some() {
            return Err(AppError::unauthorized(
                "token has been revoked",
                &request_id,
            ));
        }

        let update_pool = pool.clone();
        let token_id = api_token.id;
        tokio::spawn(async move {
            let _ =
                boardflow_db::queries::api_token::update_last_used_at(&update_pool, token_id).await;
        });

        Ok(AuthenticatedToken(api_token))
    }
}
