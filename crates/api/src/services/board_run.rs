use sqlx::PgPool;
use uuid::Uuid;

use boardflow_api_types::board_run::*;
use boardflow_domain::models::artifact_bundle::ArtifactBundleStatus;
use boardflow_domain::models::board_run::BoardRunStatus;
use boardflow_domain::models::github_job::GithubJobType;
use boardflow_domain::public_ids::{ArtifactBundleId, BoardRunId};

use crate::error::AppError;

fn bundle_status(status: ArtifactBundleStatus) -> ImportArtifactBundleStatus {
    match status {
        ArtifactBundleStatus::Pending => ImportArtifactBundleStatus::Queued,
        ArtifactBundleStatus::Validating => ImportArtifactBundleStatus::Running,
        ArtifactBundleStatus::Importing => ImportArtifactBundleStatus::Running,
        ArtifactBundleStatus::Completed => ImportArtifactBundleStatus::Completed,
        ArtifactBundleStatus::Failed => ImportArtifactBundleStatus::Failed,
    }
}

// ─── Presigned URL generation ────────────────────────────────────────────────

async fn generate_upload_info(
    s3_client: &Option<aws_sdk_s3::Client>,
    bucket: &str,
    object_key: &str,
    expires_in_secs: u64,
) -> Result<ArtifactBundleInfo, AppError> {
    match s3_client {
        Some(client) => {
            let presigning_config = aws_sdk_s3::presigning::PresigningConfig::expires_in(
                std::time::Duration::from_secs(expires_in_secs),
            )
            .map_err(|_| AppError::internal_error("presigned config error", ""))?;

            let presigned = client
                .put_object()
                .bucket(bucket)
                .key(object_key)
                .presigned(presigning_config)
                .await
                .map_err(|e| {
                    tracing::error!("presigned URL generation failed: {e}");
                    AppError::internal_error("failed to generate upload URL", "")
                })?;

            let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in_secs as i64);
            Ok(ArtifactBundleInfo {
                upload_mode: ArtifactBundleUploadMode::StagingS3,
                object_key: object_key.to_string(),
                upload_url: presigned.uri().to_string(),
                method: ArtifactBundleUploadMethod::Put,
                expires_at: expires_at.to_rfc3339(),
            })
        }
        None => {
            // Test mode: return placeholder
            let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in_secs as i64);
            Ok(ArtifactBundleInfo {
                upload_mode: ArtifactBundleUploadMode::StagingS3,
                object_key: object_key.to_string(),
                upload_url: format!("http://localhost:9000/{bucket}/{object_key}?presigned=test"),
                method: ArtifactBundleUploadMethod::Put,
                expires_at: expires_at.to_rfc3339(),
            })
        }
    }
}

pub(crate) async fn execute_create_board_run(
    pool: &PgPool,
    s3_client: &Option<aws_sdk_s3::Client>,
    staging_bucket: &str,
    repository_id: Uuid,
    req: CreateBoardRunRequest,
    request_id: &str,
) -> Result<CreateBoardRunResponse, AppError> {
    // 1. Parse board_project_id
    let board_project_id = req.board_project_id.into_uuid();

    // 2. Lookup board_project and verify ownership
    let board_project = boardflow_db::queries::board_project::find_by_id(pool, board_project_id)
        .await
        .map_err(|e| {
            tracing::error!("board_project lookup failed: {e}");
            AppError::internal_error("database error", request_id)
        })?
        .ok_or_else(|| AppError::not_found("board project not found", request_id))?;

    if board_project.repository_id != repository_id {
        return Err(AppError::forbidden(
            "token does not have access to this board project",
            request_id,
        ));
    }

    // 3. Parse numeric fields
    let github_run_id: i64 = req
        .github_run_id
        .parse()
        .map_err(|_| AppError::validation_failed("invalid github_run_id", request_id))?;
    let github_run_attempt: i32 = req
        .github_run_attempt
        .parse()
        .map_err(|_| AppError::validation_failed("invalid github_run_attempt", request_id))?;

    // 4. Idempotency check
    if let Some(existing) = boardflow_db::queries::board_run::find_by_idempotency_key(
        pool,
        board_project_id,
        github_run_id,
        github_run_attempt,
    )
    .await
    .map_err(|e| {
        tracing::error!("idempotency check failed: {e}");
        AppError::internal_error("database error", request_id)
    })? {
        let status = match existing.status {
            BoardRunStatus::Completed => CreateBoardRunStatus::Completed,
            BoardRunStatus::Failed => CreateBoardRunStatus::Failed,
            BoardRunStatus::TimedOut => CreateBoardRunStatus::TimedOut,
            BoardRunStatus::Importing => CreateBoardRunStatus::Importing,
            BoardRunStatus::Created | BoardRunStatus::Uploading => CreateBoardRunStatus::Created,
        };

        // For terminal or importing statuses, return without artifact_bundle
        if matches!(
            existing.status,
            BoardRunStatus::Completed
                | BoardRunStatus::Failed
                | BoardRunStatus::TimedOut
                | BoardRunStatus::Importing
        ) {
            return Ok(CreateBoardRunResponse {
                board_run_id: BoardRunId::from(existing.id),
                status,
                artifact_bundle: None,
            });
        }

        // For created/uploading, generate new presigned URL
        let object_key = format!("staging/runs/{}/bundle.zip", BoardRunId::from(existing.id));
        let upload_info =
            generate_upload_info(s3_client, staging_bucket, &object_key, 3600).await?;
        return Ok(CreateBoardRunResponse {
            board_run_id: BoardRunId::from(existing.id),
            status,
            artifact_bundle: Some(upload_info),
        });
    }

    // 5. Insert new BoardRun
    let run_id = Uuid::now_v7();
    let board_run = boardflow_db::queries::board_run::insert(
        pool,
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
        AppError::internal_error("database error", request_id)
    })?;

    // 6. Create ArtifactBundle
    let bundle_id = Uuid::now_v7();
    let object_key = format!("staging/runs/{}/bundle.zip", BoardRunId::from(board_run.id));
    boardflow_db::queries::artifact_bundle::insert_staging(
        pool,
        bundle_id,
        board_run.id,
        &object_key,
    )
    .await
    .map_err(|e| {
        tracing::error!("artifact_bundle insert failed: {e}");
        AppError::internal_error("database error", request_id)
    })?;

    // 7. Generate presigned URL
    let upload_info = generate_upload_info(s3_client, staging_bucket, &object_key, 3600).await?;

    Ok(CreateBoardRunResponse {
        board_run_id: BoardRunId::from(board_run.id),
        status: CreateBoardRunStatus::Created,
        artifact_bundle: Some(upload_info),
    })
}

pub(crate) async fn execute_fail_board_run(
    pool: &PgPool,
    repository_id: Uuid,
    board_run_id_str: &str,
    req: FailBoardRunRequest,
    request_id: &str,
) -> Result<FailBoardRunResponse, AppError> {
    // 1. Parse board_run_id
    let board_run_id = board_run_id_str
        .parse::<BoardRunId>()
        .map(BoardRunId::into_uuid)
        .map_err(|_| AppError::validation_failed("invalid board_run_id format", request_id))?;

    // 2. Find board_run
    let board_run = boardflow_db::queries::board_run::find_by_id(pool, board_run_id)
        .await
        .map_err(|e| {
            tracing::error!("board_run lookup failed: {e}");
            AppError::internal_error("database error", request_id)
        })?
        .ok_or_else(|| AppError::not_found("board run not found", request_id))?;

    // 3. Verify ownership via board_project
    let board_project =
        boardflow_db::queries::board_project::find_by_id(pool, board_run.board_project_id)
            .await
            .map_err(|e| {
                tracing::error!("board_project lookup failed: {e}");
                AppError::internal_error("database error", request_id)
            })?
            .ok_or_else(|| AppError::internal_error("board project not found", request_id))?;

    if board_project.repository_id != repository_id {
        return Err(AppError::forbidden(
            "token does not have access to this board run",
            request_id,
        ));
    }

    // 4. Check status
    match board_run.status {
        BoardRunStatus::Failed => {
            // Idempotent: already failed
            let failed_at = board_run
                .completed_at
                .unwrap_or_else(chrono::Utc::now)
                .to_rfc3339();
            return Ok(FailBoardRunResponse {
                board_run_id: BoardRunId::from(board_run.id),
                status: req.status,
                failed_at,
            });
        }
        BoardRunStatus::Completed => {
            return Err(AppError::conflict(
                "board run is already completed",
                request_id,
            ));
        }
        BoardRunStatus::TimedOut => {
            return Err(AppError::gone("board run has timed out", request_id));
        }
        _ => {
            // created, uploading, importing → mark failed
        }
    }

    // 5. Mark failed
    let updated = boardflow_db::queries::board_run::mark_failed(pool, board_run_id)
        .await
        .map_err(|e| {
            tracing::error!("mark_failed failed: {e}");
            AppError::internal_error("database error", request_id)
        })?;

    let failed_at = updated
        .completed_at
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();

    Ok(FailBoardRunResponse {
        board_run_id: BoardRunId::from(updated.id),
        status: req.status,
        failed_at,
    })
}

pub(crate) async fn execute_import_artifact_bundle(
    pool: &PgPool,
    repository_id: Uuid,
    installation_id: i64,
    board_run_id_str: &str,
    req: ImportArtifactBundleRequest,
    request_id: &str,
) -> Result<ImportArtifactBundleResponse, AppError> {
    // 1. Parse board_run_id
    let board_run_id = board_run_id_str
        .parse::<BoardRunId>()
        .map(BoardRunId::into_uuid)
        .map_err(|_| AppError::validation_failed("invalid board_run_id format", request_id))?;

    // 2. Begin transaction BEFORE any DB reads
    let mut tx = pool.begin().await.map_err(|e| {
        tracing::error!("transaction begin failed: {e}");
        AppError::internal_error("database error", request_id)
    })?;

    // 3. Find board_run with FOR UPDATE lock
    let board_run = boardflow_db::queries::board_run::find_by_id_for_update(&mut *tx, board_run_id)
        .await
        .map_err(|e| {
            tracing::error!("board_run lookup failed: {e}");
            AppError::internal_error("database error", request_id)
        })?
        .ok_or_else(|| AppError::not_found("board run not found", request_id))?;

    // 4. Verify ownership via board_project
    let board_project =
        boardflow_db::queries::board_project::find_by_id(&mut *tx, board_run.board_project_id)
            .await
            .map_err(|e| {
                tracing::error!("board_project lookup failed: {e}");
                AppError::internal_error("database error", request_id)
            })?
            .ok_or_else(|| AppError::internal_error("board project not found", request_id))?;

    if board_project.repository_id != repository_id {
        return Err(AppError::forbidden(
            "token does not have access to this board run",
            request_id,
        ));
    }

    // 5. Check run status
    match board_run.status {
        BoardRunStatus::Failed | BoardRunStatus::TimedOut => {
            return Err(AppError::gone("board run is no longer active", request_id));
        }
        BoardRunStatus::Completed => {
            // Return existing bundle info
            if let Some(bundle) =
                boardflow_db::queries::artifact_bundle::find_by_board_run_id(&mut *tx, board_run_id)
                    .await
                    .map_err(|e| {
                        tracing::error!("artifact_bundle lookup failed: {e}");
                        AppError::internal_error("database error", request_id)
                    })?
            {
                tx.commit().await.map_err(|e| {
                    tracing::error!("transaction commit failed: {e}");
                    AppError::internal_error("database error", request_id)
                })?;
                return Ok(ImportArtifactBundleResponse {
                    bundle_id: ArtifactBundleId::from(bundle.id),
                    status: bundle_status(bundle.status),
                });
            }
            return Err(AppError::internal_error(
                "completed run has no artifact bundle",
                request_id,
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
        AppError::internal_error("database error", request_id)
    })? {
        tx.commit().await.map_err(|e| {
            tracing::error!("transaction commit failed: {e}");
            AppError::internal_error("database error", request_id)
        })?;
        return Ok(ImportArtifactBundleResponse {
            bundle_id: ArtifactBundleId::from(existing.id),
            status: bundle_status(existing.status),
        });
    }

    // 7. Conflict check: different staging_object_key or sha256 for same run (within tx)
    if let Some(_existing) =
        boardflow_db::queries::artifact_bundle::find_existing_for_run(&mut *tx, board_run_id)
            .await
            .map_err(|e| {
                tracing::error!("conflict check failed: {e}");
                AppError::internal_error("database error", request_id)
            })?
    {
        return Err(AppError::conflict(
            "different bundle already submitted for this run",
            request_id,
        ));
    }

    // 8. Find or create ArtifactBundle and update for import
    let bundle =
        match boardflow_db::queries::artifact_bundle::find_by_board_run_id(&mut *tx, board_run_id)
            .await
            .map_err(|e| {
                tracing::error!("artifact_bundle lookup failed: {e}");
                AppError::internal_error("database error", request_id)
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
                    AppError::internal_error("database error", request_id)
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
        AppError::internal_error("database error", request_id)
    })?;

    let bundle = match updated {
        Some(b) => b,
        None => {
            return Err(AppError::conflict(
                "bundle was concurrently modified",
                request_id,
            ));
        }
    };

    // 9. Mark run as importing
    boardflow_db::queries::board_run::mark_importing(&mut *tx, board_run_id)
        .await
        .map_err(|e| {
            tracing::error!("mark_importing failed: {e}");
            AppError::internal_error("database error", request_id)
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
        installation_id,
        board_project.repository_id,
        board_project.id,
        board_run_id,
        GithubJobType::ArtifactBundleImport,
        &payload_json,
    )
    .await
    .map_err(|e| {
        tracing::error!("enqueue_import failed: {e}");
        AppError::internal_error("database error", request_id)
    })?;

    // 11. Commit transaction
    tx.commit().await.map_err(|e| {
        tracing::error!("transaction commit failed: {e}");
        AppError::internal_error("database error", request_id)
    })?;

    Ok(ImportArtifactBundleResponse {
        bundle_id: ArtifactBundleId::from(bundle.id),
        status: ImportArtifactBundleStatus::Queued,
    })
}
