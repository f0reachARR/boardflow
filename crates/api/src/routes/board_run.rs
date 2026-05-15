use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::{Extension, Json};
use sqlx::PgPool;

use boardflow_api_types::board_run::*;

use crate::error::{AppError, RequestId};
use crate::extractors::AuthenticatedToken;

// ─── POST /api/v1/board-runs ─────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/api/v1/board-runs",
    request_body = CreateBoardRunRequest,
    responses(
        (status = 200, description = "Board run created or existing returned", body = CreateBoardRunResponse),
        (status = 400, description = "Validation error", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub async fn create_board_run(
    auth: AuthenticatedToken,
    Extension(request_id): Extension<RequestId>,
    State(pool): State<PgPool>,
    Extension(s3_client): Extension<Option<aws_sdk_s3::Client>>,
    Extension(staging_bucket): Extension<crate::StagingBucket>,
    payload: Result<Json<CreateBoardRunRequest>, JsonRejection>,
) -> Result<Json<CreateBoardRunResponse>, AppError> {
    let rid = &request_id.0;
    let Json(req) = payload.map_err(|e| AppError::validation_failed(e.body_text(), rid))?;

    let response = crate::services::board_run::execute_create_board_run(
        &pool,
        &s3_client,
        &staging_bucket.0,
        auth.0.repository_id,
        req,
        rid,
    )
    .await?;

    Ok(Json(response))
}

// ─── POST /api/v1/board-runs/:board_run_id/fail ──────────────────────────────

#[utoipa::path(
    post,
    path = "/api/v1/board-runs/{board_run_id}/fail",
    params(("board_run_id" = String, Path, description = "Board run ID with br_ prefix")),
    request_body = FailBoardRunRequest,
    responses(
        (status = 200, description = "Board run marked as failed", body = FailBoardRunResponse),
        (status = 400, description = "Validation error", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
        (status = 409, description = "Conflict", body = crate::error::ErrorResponse),
        (status = 410, description = "Gone", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub async fn fail_board_run(
    auth: AuthenticatedToken,
    Extension(request_id): Extension<RequestId>,
    State(pool): State<PgPool>,
    Path(board_run_id_str): Path<String>,
    payload: Result<Json<FailBoardRunRequest>, JsonRejection>,
) -> Result<Json<FailBoardRunResponse>, AppError> {
    let rid = &request_id.0;
    let Json(req) = payload.map_err(|e| AppError::validation_failed(e.body_text(), rid))?;

    let response = crate::services::board_run::execute_fail_board_run(
        &pool,
        auth.0.repository_id,
        &board_run_id_str,
        req,
        rid,
    )
    .await?;

    Ok(Json(response))
}

// ─── POST /api/v1/board-runs/:board_run_id/artifact-bundles/import ───────────

#[utoipa::path(
    post,
    path = "/api/v1/board-runs/{board_run_id}/artifact-bundles/import",
    params(("board_run_id" = String, Path, description = "Board run ID with br_ prefix")),
    request_body = ImportArtifactBundleRequest,
    responses(
        (status = 200, description = "Import queued or existing returned", body = ImportArtifactBundleResponse),
        (status = 400, description = "Validation error", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
        (status = 409, description = "Conflict", body = crate::error::ErrorResponse),
        (status = 410, description = "Gone", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub async fn import_artifact_bundle(
    auth: AuthenticatedToken,
    Extension(request_id): Extension<RequestId>,
    State(pool): State<PgPool>,
    Path(board_run_id_str): Path<String>,
    payload: Result<Json<ImportArtifactBundleRequest>, JsonRejection>,
) -> Result<Json<ImportArtifactBundleResponse>, AppError> {
    let rid = &request_id.0;
    let Json(req) = payload.map_err(|e| AppError::validation_failed(e.body_text(), rid))?;

    let response = crate::services::board_run::execute_import_artifact_bundle(
        &pool,
        auth.0.repository_id,
        auth.0.installation_id,
        &board_run_id_str,
        req,
        rid,
    )
    .await?;

    Ok(Json(response))
}
