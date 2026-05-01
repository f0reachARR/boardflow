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
use crate::error::{AppError, RequestId};
use crate::{ArtifactSecret, FinalBucket};

#[derive(Debug, Deserialize)]
pub struct ProxyQuery {
    pub token: Option<String>,
}

pub async fn get_artifact(
    State(pool): State<PgPool>,
    Extension(s3_client): Extension<Option<aws_sdk_s3::Client>>,
    Extension(secret): Extension<ArtifactSecret>,
    Extension(final_bucket): Extension<FinalBucket>,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Path(artifact_id_str): Path<String>,
    Query(query): Query<ProxyQuery>,
) -> Result<Response<Body>, AppError> {
    // Validate token
    let token = query.token.as_deref().unwrap_or("");
    if token.is_empty() {
        return Err(AppError::unauthorized("missing token", &request_id));
    }

    let (token_artifact_id, _user_id) = verify_artifact_token(token, &secret.0)
        .ok_or_else(|| AppError::unauthorized("invalid or expired token", &request_id))?;

    // Parse artifact_id from path
    let artifact_id = Uuid::parse_str(&artifact_id_str)
        .map_err(|_| AppError::not_found("artifact not found", &request_id))?;

    // Verify token matches requested artifact
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
            tracing::error!("S3 error fetching artifact {artifact_id}: {e}");
            AppError::internal_error("storage error", &request_id)
        })?;

    // Determine content type from DB metadata (preferred) or S3 response
    let content_type = artifact
        .content_type
        .as_deref()
        .unwrap_or("application/octet-stream");

    // Determine CSP based on artifact type
    let csp = match artifact.r#type.as_str() {
        "ibom_html" => "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:",
        _ => "default-src 'none'",
    };

    // Build streaming response body from S3 SdkBody
    let sdk_body = s3_resp.body.into_inner();
    let body = Body::new(sdk_body);

    let mut builder = Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header("X-Content-Type-Options", "nosniff")
        .header("Content-Security-Policy", csp);

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
