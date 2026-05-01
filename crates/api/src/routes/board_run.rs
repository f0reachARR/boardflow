use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use boardflow_domain::models::board_run::BoardRunStatus;

use crate::error::{AppError, RequestId};
use crate::extractors::AuthenticatedToken;

// ─── ID prefix helpers ───────────────────────────────────────────────────────

fn parse_board_project_id(s: &str) -> Option<Uuid> {
    s.strip_prefix("bp_").and_then(|v| Uuid::parse_str(v).ok())
}

fn parse_board_run_id(s: &str) -> Option<Uuid> {
    s.strip_prefix("br_").and_then(|v| Uuid::parse_str(v).ok())
}

fn format_board_run_id(id: Uuid) -> String {
    format!("br_{id}")
}

fn format_bundle_id(id: Uuid) -> String {
    format!("ab_{id}")
}

// ─── Request/Response types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateBoardRunRequest {
    pub board_project_id: String,
    pub project_path: String,
    pub tree_hash: String,
    pub commit_sha: String,
    pub branch: String,
    #[serde(rename = "ref")]
    pub ref_: String,
    pub github_run_id: String,
    pub github_run_attempt: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateBoardRunResponse {
    pub board_run_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_bundle: Option<ArtifactBundleInfo>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ArtifactBundleInfo {
    pub upload_mode: String,
    pub object_key: String,
    pub upload_url: String,
    pub method: String,
    pub expires_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct FailBoardRunRequest {
    pub status: String,
    pub error: FailErrorInfo,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct FailErrorInfo {
    pub message: String,
    #[serde(default)]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FailBoardRunResponse {
    pub board_run_id: String,
    pub status: String,
    pub failed_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ImportArtifactBundleRequest {
    pub staging_object_key: String,
    pub bundle_sha256: String,
    pub bundle_size_bytes: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ImportArtifactBundleResponse {
    pub bundle_id: String,
    pub status: String,
}

// ─── Presigned URL generation ────────────────────────────────────────────────

async fn generate_upload_info(
    s3_client: &Option<aws_sdk_s3::Client>,
    object_key: &str,
    expires_in_secs: u64,
) -> Result<ArtifactBundleInfo, AppError> {
    let bucket = std::env::var("MINIO_BUCKET_STAGING")
        .unwrap_or_else(|_| "boardflow-staging".to_string());

    match s3_client {
        Some(client) => {
            let presigning_config = aws_sdk_s3::presigning::PresigningConfig::expires_in(
                std::time::Duration::from_secs(expires_in_secs),
            )
            .map_err(|_| AppError::internal_error("presigned config error", ""))?;

            let presigned = client
                .put_object()
                .bucket(&bucket)
                .key(object_key)
                .presigned(presigning_config)
                .await
                .map_err(|e| {
                    tracing::error!("presigned URL generation failed: {e}");
                    AppError::internal_error("failed to generate upload URL", "")
                })?;

            let expires_at =
                chrono::Utc::now() + chrono::Duration::seconds(expires_in_secs as i64);
            Ok(ArtifactBundleInfo {
                upload_mode: "staging_s3".to_string(),
                object_key: object_key.to_string(),
                upload_url: presigned.uri().to_string(),
                method: "PUT".to_string(),
                expires_at: expires_at.to_rfc3339(),
            })
        }
        None => {
            // Test mode: return placeholder
            let expires_at =
                chrono::Utc::now() + chrono::Duration::seconds(expires_in_secs as i64);
            Ok(ArtifactBundleInfo {
                upload_mode: "staging_s3".to_string(),
                object_key: object_key.to_string(),
                upload_url: format!(
                    "http://localhost:9000/{bucket}/{object_key}?presigned=test"
                ),
                method: "PUT".to_string(),
                expires_at: expires_at.to_rfc3339(),
            })
        }
    }
}

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
    payload: Result<Json<CreateBoardRunRequest>, JsonRejection>,
) -> Result<Json<CreateBoardRunResponse>, AppError> {
    let rid = &request_id.0;
    let Json(req) = payload.map_err(|e| AppError::validation_failed(e.body_text(), rid))?;

    // 1. Parse board_project_id
    let board_project_id = parse_board_project_id(&req.board_project_id)
        .ok_or_else(|| AppError::validation_failed("invalid board_project_id format", rid))?;

    // 2. Lookup board_project and verify ownership
    let board_project = boardflow_db::queries::board_project::find_by_id(&pool, board_project_id)
        .await
        .map_err(|e| {
            tracing::error!("board_project lookup failed: {e}");
            AppError::internal_error("database error", rid)
        })?
        .ok_or_else(|| AppError::not_found("board project not found", rid))?;

    if board_project.repository_id != auth.0.repository_id {
        return Err(AppError::forbidden(
            "token does not have access to this board project",
            rid,
        ));
    }

    // 3. Parse numeric fields
    let github_run_id: i64 = req
        .github_run_id
        .parse()
        .map_err(|_| AppError::validation_failed("invalid github_run_id", rid))?;
    let github_run_attempt: i32 = req
        .github_run_attempt
        .parse()
        .map_err(|_| AppError::validation_failed("invalid github_run_attempt", rid))?;

    // 4. Idempotency check
    if let Some(existing) = boardflow_db::queries::board_run::find_by_idempotency_key(
        &pool,
        board_project_id,
        github_run_id,
        github_run_attempt,
    )
    .await
    .map_err(|e| {
        tracing::error!("idempotency check failed: {e}");
        AppError::internal_error("database error", rid)
    })? {
        let status_str = match existing.status {
            BoardRunStatus::Completed => "completed",
            BoardRunStatus::Failed => "failed",
            BoardRunStatus::TimedOut => "timed_out",
            BoardRunStatus::Importing => "importing",
            BoardRunStatus::Created | BoardRunStatus::Uploading => "created",
        };

        // For terminal or importing statuses, return without artifact_bundle
        if matches!(
            existing.status,
            BoardRunStatus::Completed
                | BoardRunStatus::Failed
                | BoardRunStatus::TimedOut
                | BoardRunStatus::Importing
        ) {
            return Ok(Json(CreateBoardRunResponse {
                board_run_id: format_board_run_id(existing.id),
                status: status_str.to_string(),
                artifact_bundle: None,
            }));
        }

        // For created/uploading, generate new presigned URL
        let object_key = format!("staging/runs/{}/bundle.zip", format_board_run_id(existing.id));
        let upload_info = generate_upload_info(&s3_client, &object_key, 3600).await?;
        return Ok(Json(CreateBoardRunResponse {
            board_run_id: format_board_run_id(existing.id),
            status: status_str.to_string(),
            artifact_bundle: Some(upload_info),
        }));
    }

    // 5. Insert new BoardRun
    let run_id = Uuid::now_v7();
    let board_run = boardflow_db::queries::board_run::insert(
        &pool,
        run_id,
        board_project_id,
        &req.commit_sha,
        &req.branch,
        &req.ref_,
        github_run_id,
        github_run_attempt,
        &req.tree_hash,
    )
    .await
    .map_err(|e| {
        tracing::error!("board_run insert failed: {e}");
        AppError::internal_error("database error", rid)
    })?;

    // 6. Create ArtifactBundle
    let bundle_id = Uuid::now_v7();
    let object_key = format!("staging/runs/{}/bundle.zip", format_board_run_id(board_run.id));
    boardflow_db::queries::artifact_bundle::insert_staging(&pool, bundle_id, board_run.id, &object_key)
        .await
        .map_err(|e| {
            tracing::error!("artifact_bundle insert failed: {e}");
            AppError::internal_error("database error", rid)
        })?;

    // 7. Generate presigned URL
    let upload_info = generate_upload_info(&s3_client, &object_key, 3600).await?;

    Ok(Json(CreateBoardRunResponse {
        board_run_id: format_board_run_id(board_run.id),
        status: "created".to_string(),
        artifact_bundle: Some(upload_info),
    }))
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

    // 1. Validate status field
    if req.status != "failed" {
        return Err(AppError::validation_failed("status must be 'failed'", rid));
    }

    // 2. Parse board_run_id
    let board_run_id = parse_board_run_id(&board_run_id_str)
        .ok_or_else(|| AppError::validation_failed("invalid board_run_id format", rid))?;

    // 3. Find board_run
    let board_run = boardflow_db::queries::board_run::find_by_id(&pool, board_run_id)
        .await
        .map_err(|e| {
            tracing::error!("board_run lookup failed: {e}");
            AppError::internal_error("database error", rid)
        })?
        .ok_or_else(|| AppError::not_found("board run not found", rid))?;

    // 4. Verify ownership via board_project
    let board_project =
        boardflow_db::queries::board_project::find_by_id(&pool, board_run.board_project_id)
            .await
            .map_err(|e| {
                tracing::error!("board_project lookup failed: {e}");
                AppError::internal_error("database error", rid)
            })?
            .ok_or_else(|| AppError::internal_error("board project not found", rid))?;

    if board_project.repository_id != auth.0.repository_id {
        return Err(AppError::forbidden(
            "token does not have access to this board run",
            rid,
        ));
    }

    // 5. Check status
    match board_run.status {
        BoardRunStatus::Failed => {
            // Idempotent: already failed
            let failed_at = board_run
                .completed_at
                .unwrap_or_else(chrono::Utc::now)
                .to_rfc3339();
            return Ok(Json(FailBoardRunResponse {
                board_run_id: format_board_run_id(board_run.id),
                status: "failed".to_string(),
                failed_at,
            }));
        }
        BoardRunStatus::Completed => {
            return Err(AppError::conflict(
                "board run is already completed",
                rid,
            ));
        }
        BoardRunStatus::TimedOut => {
            return Err(AppError::gone("board run has timed out", rid));
        }
        _ => {
            // created, uploading, importing → mark failed
        }
    }

    // 6. Mark failed
    let updated = boardflow_db::queries::board_run::mark_failed(&pool, board_run_id)
        .await
        .map_err(|e| {
            tracing::error!("mark_failed failed: {e}");
            AppError::internal_error("database error", rid)
        })?;

    let failed_at = updated
        .completed_at
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();

    Ok(Json(FailBoardRunResponse {
        board_run_id: format_board_run_id(updated.id),
        status: "failed".to_string(),
        failed_at,
    }))
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

    // 1. Parse board_run_id
    let board_run_id = parse_board_run_id(&board_run_id_str)
        .ok_or_else(|| AppError::validation_failed("invalid board_run_id format", rid))?;

    // 2. Begin transaction BEFORE any DB reads
    let mut tx = pool.begin().await.map_err(|e| {
        tracing::error!("transaction begin failed: {e}");
        AppError::internal_error("database error", rid)
    })?;

    // 3. Find board_run with FOR UPDATE lock
    let board_run =
        boardflow_db::queries::board_run::find_by_id_for_update(&mut *tx, board_run_id)
            .await
            .map_err(|e| {
                tracing::error!("board_run lookup failed: {e}");
                AppError::internal_error("database error", rid)
            })?
            .ok_or_else(|| AppError::not_found("board run not found", rid))?;

    // 4. Verify ownership via board_project
    let board_project =
        boardflow_db::queries::board_project::find_by_id(&mut *tx, board_run.board_project_id)
            .await
            .map_err(|e| {
                tracing::error!("board_project lookup failed: {e}");
                AppError::internal_error("database error", rid)
            })?
            .ok_or_else(|| AppError::internal_error("board project not found", rid))?;

    if board_project.repository_id != auth.0.repository_id {
        return Err(AppError::forbidden(
            "token does not have access to this board run",
            rid,
        ));
    }

    // 5. Check run status
    match board_run.status {
        BoardRunStatus::Failed | BoardRunStatus::TimedOut => {
            return Err(AppError::gone("board run is no longer active", rid));
        }
        BoardRunStatus::Completed => {
            // Return existing bundle info
            if let Some(bundle) =
                boardflow_db::queries::artifact_bundle::find_by_board_run_id(
                    &mut *tx,
                    board_run_id,
                )
                .await
                .map_err(|e| {
                    tracing::error!("artifact_bundle lookup failed: {e}");
                    AppError::internal_error("database error", rid)
                })?
            {
                tx.commit().await.map_err(|e| {
                    tracing::error!("transaction commit failed: {e}");
                    AppError::internal_error("database error", rid)
                })?;
                return Ok(Json(ImportArtifactBundleResponse {
                    bundle_id: format_bundle_id(bundle.id),
                    status: "completed".to_string(),
                }));
            }
            return Err(AppError::internal_error(
                "completed run has no artifact bundle",
                rid,
            ));
        }
        _ => {}
    }

    // 6. Idempotency check: same key + sha256 (within tx)
    if let Some(existing) = boardflow_db::queries::artifact_bundle::find_by_import_key(
        &mut *tx,
        board_run_id,
        &req.staging_object_key,
        &req.bundle_sha256,
    )
    .await
    .map_err(|e| {
        tracing::error!("idempotency check failed: {e}");
        AppError::internal_error("database error", rid)
    })? {
        tx.commit().await.map_err(|e| {
            tracing::error!("transaction commit failed: {e}");
            AppError::internal_error("database error", rid)
        })?;
        return Ok(Json(ImportArtifactBundleResponse {
            bundle_id: format_bundle_id(existing.id),
            status: "queued".to_string(),
        }));
    }

    // 7. Conflict check: different staging_object_key or sha256 for same run (within tx)
    if let Some(_existing) =
        boardflow_db::queries::artifact_bundle::find_existing_for_run(&mut *tx, board_run_id)
            .await
            .map_err(|e| {
                tracing::error!("conflict check failed: {e}");
                AppError::internal_error("database error", rid)
            })?
    {
        return Err(AppError::conflict(
            "different bundle already submitted for this run",
            rid,
        ));
    }

    // 8. Find or create ArtifactBundle and update for import
    let bundle = match boardflow_db::queries::artifact_bundle::find_by_board_run_id(
        &mut *tx,
        board_run_id,
    )
    .await
    .map_err(|e| {
        tracing::error!("artifact_bundle lookup failed: {e}");
        AppError::internal_error("database error", rid)
    })? {
        Some(existing_bundle) => existing_bundle,
        None => {
            let bundle_id = Uuid::now_v7();
            boardflow_db::queries::artifact_bundle::insert_staging(
                &mut *tx,
                bundle_id,
                board_run_id,
                &req.staging_object_key,
            )
            .await
            .map_err(|e| {
                tracing::error!("artifact_bundle insert failed: {e}");
                AppError::internal_error("database error", rid)
            })?
        }
    };

    let updated = boardflow_db::queries::artifact_bundle::update_for_import(
        &mut *tx,
        bundle.id,
        &req.staging_object_key,
        &req.bundle_sha256,
        req.bundle_size_bytes,
    )
    .await
    .map_err(|e| {
        tracing::error!("artifact_bundle update failed: {e}");
        AppError::internal_error("database error", rid)
    })?;

    let bundle = match updated {
        Some(b) => b,
        None => {
            return Err(AppError::conflict(
                "bundle was concurrently modified",
                rid,
            ));
        }
    };

    // 9. Mark run as importing
    boardflow_db::queries::board_run::mark_importing(&mut *tx, board_run_id)
        .await
        .map_err(|e| {
            tracing::error!("mark_importing failed: {e}");
            AppError::internal_error("database error", rid)
        })?;

    // 10. Enqueue import job
    let job_id = Uuid::now_v7();
    let payload_json = serde_json::json!({
        "bundle_id": bundle.id.to_string(),
        "board_run_id": board_run_id.to_string(),
        "staging_object_key": req.staging_object_key,
        "bundle_sha256": req.bundle_sha256,
        "bundle_size_bytes": req.bundle_size_bytes,
    });
    boardflow_db::queries::github_job::enqueue_import(
        &mut *tx,
        job_id,
        auth.0.installation_id,
        board_project.repository_id,
        board_project.id,
        board_run_id,
        &payload_json,
    )
    .await
    .map_err(|e| {
        tracing::error!("enqueue_import failed: {e}");
        AppError::internal_error("database error", rid)
    })?;

    // 11. Commit transaction
    tx.commit().await.map_err(|e| {
        tracing::error!("transaction commit failed: {e}");
        AppError::internal_error("database error", rid)
    })?;

    Ok(Json(ImportArtifactBundleResponse {
        bundle_id: format_bundle_id(bundle.id),
        status: "queued".to_string(),
    }))
}
