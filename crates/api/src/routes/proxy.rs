use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::http::Response;
use axum::Extension;
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use boardflow_domain::models::artifact::ArtifactStatus;

use crate::artifact_token::verify_artifact_token;
use crate::error::{AppError, ErrorCode, RequestId};
use crate::{AppDomain, ArtifactSecret, FinalBucket};

/// Parse artifact_id from path parameter, expecting `art_` prefix.
fn parse_artifact_id(s: &str) -> Option<Uuid> {
    s.strip_prefix("art_").and_then(|v| Uuid::parse_str(v).ok())
}

#[derive(Debug, Deserialize)]
pub struct ProxyQuery {
    pub token: Option<String>,
}

pub async fn get_artifact(
    State(pool): State<PgPool>,
    Extension(s3_client): Extension<Option<aws_sdk_s3::Client>>,
    Extension(secret): Extension<ArtifactSecret>,
    Extension(final_bucket): Extension<FinalBucket>,
    Extension(app_domain): Extension<AppDomain>,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Path(artifact_id_str): Path<String>,
    Query(query): Query<ProxyQuery>,
) -> Result<Response<Body>, AppError> {
    // Validate token
    let token = query.token.as_deref().unwrap_or("");
    if token.is_empty() {
        return Err(AppError::unauthorized("missing token", &request_id));
    }

    // user_id is embedded in the token for audit purposes but not checked here;
    // the proxy endpoint is accessed via img/iframe src without a session cookie,
    // so authentication relies solely on the HMAC-signed short-lived token (1h expiry).
    // Design decision: Bearer token only. No session verification required.
    // The token is HMAC-signed with a server secret and short-lived; viewer-sources
    // issues tokens only to authenticated users, so proxy-side session check is unnecessary.
    let (token_artifact_id, _user_id) = verify_artifact_token(token, &secret.0)
        .ok_or_else(|| AppError::unauthorized("invalid or expired token", &request_id))?;

    // Parse artifact_id from path (expects art_ prefix per viewer-sources URL format)
    let artifact_id = parse_artifact_id(&artifact_id_str)
        .ok_or_else(|| AppError::new(ErrorCode::ValidationFailed, "invalid artifact_id format", &request_id))?;

    // Verify token's artifact_id matches the URL path artifact_id
    if token_artifact_id != artifact_id {
        return Err(AppError::unauthorized("token mismatch", &request_id));
    }

    // Look up artifact in DB
    let artifact = boardflow_db::queries::artifact::find_by_id(&pool, artifact_id)
        .await
        .map_err(|e| {
            tracing::error!("DB error fetching artifact: {e}");
            AppError::internal_error("internal error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("artifact not found", &request_id))?;

    // Check status
    if artifact.status != ArtifactStatus::Available {
        return Err(AppError::not_found("artifact not available", &request_id));
    }

    // Get storage key
    let storage_key = artifact
        .storage_key
        .as_deref()
        .ok_or_else(|| AppError::internal_error("artifact has no storage key", &request_id))?;

    // Get S3 client
    let client = s3_client
        .as_ref()
        .ok_or_else(|| AppError::internal_error("storage not configured", &request_id))?;

    // Fetch object from S3
    let s3_resp = client
        .get_object()
        .bucket(&final_bucket.0)
        .key(storage_key)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("S3 upstream error fetching artifact {artifact_id}: {e}");
            AppError::internal_error("upstream storage error", &request_id)
        })?;

    // Determine content type from DB metadata (preferred) or S3 response
    let content_type = artifact
        .content_type
        .as_deref()
        .unwrap_or("application/octet-stream");

    // Determine CSP and X-Frame-Options based on artifact type.
    // Design: artifact proxy is served on a separate domain (e.g. artifacts.boardflow.example.com).
    // For iframe artifacts (ibom_html), we use CSP frame-ancestors to allow embedding
    // from the app domain only. X-Frame-Options is omitted for iframe artifacts because
    // ALLOW-FROM is deprecated; CSP frame-ancestors is the standard mechanism.
    // For non-iframe artifacts, X-Frame-Options: DENY prevents any framing.
    let is_iframe_artifact = artifact.r#type.as_str() == "ibom_html";
    let csp = if is_iframe_artifact {
        format!(
            "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; frame-ancestors {}",
            app_domain.0
        )
    } else {
        "default-src 'none'; frame-ancestors 'none'".to_string()
    };

    // Build streaming response body from S3 SdkBody
    let sdk_body = s3_resp.body.into_inner();
    let body = Body::new(sdk_body);

    let mut builder = Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header("X-Content-Type-Options", "nosniff")
        .header("Referrer-Policy", "no-referrer")
        .header("Content-Security-Policy", &csp)
        .header("Access-Control-Allow-Origin", &app_domain.0)
        .header("Access-Control-Allow-Methods", "GET")
        .header("Vary", "Origin");

    // Non-iframe artifacts get X-Frame-Options: DENY.
    // iframe artifacts rely solely on CSP frame-ancestors (ALLOW-FROM is deprecated).
    if !is_iframe_artifact {
        builder = builder.header("X-Frame-Options", "DENY");
    }

    // Add Content-Length if available
    if let Some(size) = artifact.size_bytes {
        builder = builder.header(header::CONTENT_LENGTH, size.to_string());
    }

    // Add Content-Disposition for downloadable types
    if let Some(filename) = &artifact.filename {
        let disposition = match artifact.r#type.as_str() {
            "ibom_html" | "schematic_svg" | "pcb_svg" | "schematic_pdf" | "pcb_pdf" => {
                format!("inline; filename=\"{filename}\"")
            }
            _ => format!("attachment; filename=\"{filename}\""),
        };
        builder = builder.header(header::CONTENT_DISPOSITION, disposition);
    }

    builder.body(body).map_err(|e| {
        tracing::error!("Failed to build response: {e}");
        AppError::internal_error("response error", &request_id)
    })
}
