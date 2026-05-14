mod normalize;
mod persist;
mod s3_ops;

use boardflow_artifact::ArtifactError;
use boardflow_db::queries::{artifact_bundle, board_run, github_job};
use boardflow_domain::models::github_job::GithubJob;
use boardflow_jobs::{BASE_BACKOFF_SECS, MAX_ATTEMPTS};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::WorkerConfig;

#[derive(Debug, Deserialize)]
struct ImportPayload {
    staging_object_key: String,
    bundle_sha256: String,
    #[allow(dead_code)]
    bundle_size_bytes: i64,
}

pub async fn handle(
    pool: &PgPool,
    s3_client: &aws_sdk_s3::Client,
    config: &WorkerConfig,
    job: &GithubJob,
) -> super::HandlerResult {
    match process_import_job(pool, s3_client, config, job).await {
        Ok(()) => super::HandlerResult::Completed,
        Err(e) => {
            tracing::error!(job_id = %job.id, error = %e, "Import job failed");
            handle_import_failure(pool, job, &e.to_string()).await;
            // Already handled mark_failed/reschedule inside handle_import_failure
            // Return Completed to avoid double-handling in dispatcher
            super::HandlerResult::Completed
        }
    }
}

async fn validate_payload(
    pool: &PgPool,
    job: &GithubJob,
) -> Result<
    (
        ImportPayload,
        Uuid,
        Uuid,
        boardflow_domain::models::artifact_bundle::ArtifactBundle,
    ),
    ArtifactError,
> {
    let payload: ImportPayload = serde_json::from_value(job.payload_json.clone())
        .map_err(|e| ArtifactError::Manifest(format!("invalid job payload: {e}")))?;

    let board_run_id = job
        .board_run_id
        .ok_or_else(|| ArtifactError::Manifest("job missing board_run_id".into()))?;
    let board_project_id = job
        .board_project_id
        .ok_or_else(|| ArtifactError::Manifest("job missing board_project_id".into()))?;

    // Find the artifact bundle
    let bundle = artifact_bundle::find_by_board_run_id(pool, board_run_id)
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?
        .ok_or_else(|| ArtifactError::Manifest("artifact bundle not found".into()))?;

    // Update bundle status to importing
    artifact_bundle::mark_importing(pool, bundle.id)
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;

    Ok((payload, board_run_id, board_project_id, bundle))
}

async fn process_import_job(
    pool: &PgPool,
    s3_client: &aws_sdk_s3::Client,
    config: &WorkerConfig,
    job: &GithubJob,
) -> Result<(), ArtifactError> {
    // Phase 1: Validate payload and mark bundle as importing
    let (payload, board_run_id, board_project_id, bundle) = validate_payload(pool, job).await?;

    // Phase 2: S3 operations (pre-transaction)
    let data = s3_ops::download_and_verify(
        s3_client,
        config,
        &payload.staging_object_key,
        &payload.bundle_sha256,
    )
    .await?;
    let (manifest, uploaded) =
        s3_ops::extract_and_upload(s3_client, config, &data, board_run_id).await?;

    // Phase 3: Transaction — all DB writes
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;

    persist::persist_artifacts(&mut tx, board_run_id, bundle.id, &manifest, &uploaded).await?;
    let check_summary =
        persist::persist_checks_and_findings(&mut tx, board_run_id, &manifest).await?;
    persist::persist_snapshot_and_diff(&mut tx, board_project_id, board_run_id, &manifest).await?;
    persist::complete_run(
        &mut tx,
        board_project_id,
        board_run_id,
        bundle.id,
        job.id,
        &manifest.tree_hash,
        &check_summary,
    )
    .await?;
    persist::enqueue_follow_up_jobs(&mut tx, job, board_project_id, board_run_id).await?;

    tx.commit()
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;

    tracing::info!(job_id = %job.id, board_run_id = %board_run_id, "Import job completed successfully");
    Ok(())
}

async fn handle_import_failure(pool: &PgPool, job: &GithubJob, error_message: &str) {
    if job.attempts >= MAX_ATTEMPTS {
        // Terminal failure — mark bundle and run as failed
        if let Some(board_run_id) = job.board_run_id {
            if let Ok(Some(bundle)) =
                artifact_bundle::find_by_board_run_id(pool, board_run_id).await
            {
                let _ = artifact_bundle::mark_failed(pool, bundle.id, error_message).await;
            }
            let _ = board_run::mark_failed(pool, board_run_id).await;
        }
        let _ = github_job::mark_failed(pool, job.id, error_message).await;
    } else {
        // Retryable — reschedule job, keep bundle in 'importing' state
        let backoff = BASE_BACKOFF_SECS * 3_f64.powi(job.attempts);
        let _ = github_job::reschedule(pool, job.id, error_message, backoff).await;
    }
}
