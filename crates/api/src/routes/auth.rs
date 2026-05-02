use axum::Extension;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{AppError, RequestId};
use crate::extractors::AuthenticatedSession;

// ─── Shared state for OAuth config ──────────────────────────────────────────

#[derive(Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
}

// ─── GET /api/v1/auth/login ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct LoginQuery {
    pub redirect_uri: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/login",
    responses(
        (status = 302, description = "Redirect to GitHub OAuth"),
    )
)]
pub async fn login(
    Extension(oauth_config): Extension<OAuthConfig>,
    Query(_query): Query<LoginQuery>,
) -> Response {
    let state = Uuid::new_v4().to_string();
    let url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&scope=read:user&state={}",
        oauth_config.client_id,
        urlencoding::encode(&state)
    );

    let oauth_state_cookie = format!(
        "boardflow_oauth_state={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=300",
        state
    );

    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, url)
        .header(header::SET_COOKIE, oauth_state_cookie)
        .body(axum::body::Body::empty())
        .unwrap()
}

// ─── GET /api/v1/auth/callback ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    pub state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubTokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct GitHubUserResponse {
    id: i64,
    login: String,
    avatar_url: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/callback",
    responses(
        (status = 302, description = "Redirect after successful OAuth"),
        (status = 401, description = "OAuth failed"),
        (status = 403, description = "CSRF state mismatch"),
    )
)]
pub async fn callback(
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(oauth_config): Extension<OAuthConfig>,
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Result<Response, AppError> {
    // Verify CSRF state: compare cookie state with query param state
    let cookie_state = extract_cookie_value(&headers, "boardflow_oauth_state");
    let query_state = query.state.as_deref().unwrap_or("");

    match cookie_state {
        Some(ref cs) if cs == query_state => {}
        _ => {
            return Err(AppError::new(
                crate::error::ErrorCode::Forbidden,
                "OAuth state mismatch",
                &request_id,
            ));
        }
    }
    // Exchange code for access token
    let client = reqwest::Client::new();
    let token_resp = client
        .post("https://github.com/login/oauth/access_token")
        .header(header::ACCEPT, "application/json")
        .form(&[
            ("client_id", oauth_config.client_id.as_str()),
            ("client_secret", oauth_config.client_secret.as_str()),
            ("code", query.code.as_str()),
        ])
        .send()
        .await
        .map_err(|e| {
            tracing::error!("GitHub token exchange failed: {e}");
            AppError::unauthorized("oauth token exchange failed", &request_id)
        })?;

    let token_data: GitHubTokenResponse = token_resp.json().await.map_err(|e| {
        tracing::error!("Failed to parse GitHub token response: {e}");
        AppError::unauthorized("oauth token exchange failed", &request_id)
    })?;

    // Fetch GitHub user info
    let user_resp = client
        .get("https://api.github.com/user")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", token_data.access_token),
        )
        .header(header::USER_AGENT, "BoardFlow")
        .send()
        .await
        .map_err(|e| {
            tracing::error!("GitHub user fetch failed: {e}");
            AppError::unauthorized("failed to fetch user info", &request_id)
        })?;

    let github_user: GitHubUserResponse = user_resp.json().await.map_err(|e| {
        tracing::error!("Failed to parse GitHub user response: {e}");
        AppError::unauthorized("failed to fetch user info", &request_id)
    })?;

    // Upsert user
    let user = boardflow_db::queries::user::upsert(
        &pool,
        github_user.id,
        &github_user.login,
        github_user.avatar_url.as_deref(),
        &token_data.access_token,
    )
    .await
    .map_err(|e| {
        tracing::error!("user upsert failed: {e}");
        AppError::internal_error("database error", &request_id)
    })?;

    // Create session (7 days)
    let expires_at = Utc::now() + chrono::Duration::days(7);
    let session = boardflow_db::queries::session::create(&pool, user.id, expires_at)
        .await
        .map_err(|e| {
            tracing::error!("session create failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?;

    // Set session cookie, clear oauth_state cookie, and redirect to "/"
    let session_cookie = format!(
        "boardflow_session={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=604800",
        session.id
    );
    let clear_oauth_state_cookie =
        "boardflow_oauth_state=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0";

    let response = Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, "/")
        .header(header::SET_COOKIE, session_cookie)
        .header(header::SET_COOKIE, clear_oauth_state_cookie)
        .body(axum::body::Body::empty())
        .unwrap();

    Ok(response)
}

// ─── POST /api/v1/auth/logout ───────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    responses(
        (status = 200, description = "Logged out"),
        (status = 401, description = "Unauthorized"),
    )
)]
pub async fn logout(
    Extension(RequestId(request_id)): Extension<RequestId>,
    State(pool): State<PgPool>,
    session: AuthenticatedSession,
) -> Result<Response, AppError> {
    boardflow_db::queries::session::delete_by_id(&pool, session.session_id)
        .await
        .map_err(|e| {
            tracing::error!("session delete failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?;

    let cookie = "boardflow_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0";
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::SET_COOKIE, cookie)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(r#"{"ok":true}"#))
        .unwrap();

    Ok(response)
}

// ─── GET /api/v1/auth/me ────────────────────────────────────────────────────

#[derive(Debug, Serialize, ToSchema)]
pub struct MeResponse {
    pub user_id: String,
    pub github_login: String,
    pub github_avatar_url: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/me",
    responses(
        (status = 200, description = "Current user info", body = MeResponse),
        (status = 401, description = "Unauthorized"),
    )
)]
pub async fn me(session: AuthenticatedSession) -> Json<MeResponse> {
    Json(MeResponse {
        user_id: session.user.id.to_string(),
        github_login: session.user.github_login,
        github_avatar_url: session.user.github_avatar_url,
    })
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn extract_cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(';'))
        .map(|s| s.trim())
        .find_map(|cookie| {
            let (k, v) = cookie.split_once('=')?;
            if k.trim() == name {
                Some(v.trim().to_string())
            } else {
                None
            }
        })
}
