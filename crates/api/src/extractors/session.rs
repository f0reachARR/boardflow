use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use sqlx::PgPool;

use boardflow_domain::models::user::User;

use crate::error::{AppError, RequestId};

pub struct AuthenticatedSession {
    pub user: User,
    pub session_id: uuid::Uuid,
}

const SESSION_COOKIE_NAME: &str = "boardflow_session";

impl<S> FromRequestParts<S> for AuthenticatedSession
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

        // Extract session ID from cookie
        let cookie_header = parts
            .headers
            .get(axum::http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let session_id_str = parse_cookie(cookie_header, SESSION_COOKIE_NAME)
            .ok_or_else(|| AppError::unauthorized("missing session cookie", &request_id))?;

        let session_id = uuid::Uuid::parse_str(session_id_str)
            .map_err(|_| AppError::unauthorized("invalid session cookie", &request_id))?;

        // Look up session
        let session = boardflow_db::queries::session::find_by_id(&pool, session_id)
            .await
            .map_err(|e| {
                tracing::error!("session lookup failed: {e}");
                AppError::internal_error("database error", &request_id)
            })?
            .ok_or_else(|| AppError::unauthorized("session not found", &request_id))?;

        // Check expiry
        if session.expires_at < chrono::Utc::now() {
            return Err(AppError::unauthorized("session expired", &request_id));
        }

        // Look up user
        let user = boardflow_db::queries::user::find_by_id(&pool, session.user_id)
            .await
            .map_err(|e| {
                tracing::error!("user lookup failed: {e}");
                AppError::internal_error("database error", &request_id)
            })?
            .ok_or_else(|| AppError::unauthorized("user not found", &request_id))?;

        Ok(AuthenticatedSession {
            user,
            session_id: session.id,
        })
    }
}

fn parse_cookie<'a>(cookie_header: &'a str, name: &str) -> Option<&'a str> {
    for part in cookie_header.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix(name) {
            if let Some(value) = value.strip_prefix('=') {
                return Some(value.trim());
            }
        }
    }
    None
}
