use boardflow_artifact::{
    download_bundle, extract_bundle, upload_artifact, verify_sha256, ArtifactError,
};
use boardflow_db::queries::{
    artifact, artifact_bundle, board_project, board_run, diff, github_job, run_check, snapshot,
};
use boardflow_domain::models::github_job::GithubJob;
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

mod config;

use config::WorkerConfig;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let config = WorkerConfig::from_env();
    tracing::info!("BoardFlow worker starting");

    let pool = boardflow_db::create_pool(&config.database_url)
        .await
        .expect("failed to connect to database");

    let aws_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let s3_config = if let Some(ref endpoint) = config.s3_endpoint {
        aws_sdk_s3::config::Builder::from(&aws_config)
            .endpoint_url(endpoint)
            .force_path_style(true)
            .build()
    } else {
        aws_sdk_s3::config::Builder::from(&aws_config).build()
    };
    let s3_client = aws_sdk_s3::Client::from_conf(s3_config);

    tracing::info!("BoardFlow worker started, polling for jobs");

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                tracing::info!("Shutdown signal received, stopping worker");
                break;
            }
            _ = poll_and_process(&pool, &s3_client, &config) => {}
        }
    }

    tracing::info!("BoardFlow worker stopped");
}

async fn poll_and_process(
    pool: &PgPool,
    s3_client: &aws_sdk_s3::Client,
    config: &WorkerConfig,
) {
    match github_job::dequeue(pool, "artifact_bundle_import").await {
        Ok(Some(job)) => {
            tracing::info!(job_id = %job.id, "Dequeued import job");
            if let Err(e) = process_import_job(pool, s3_client, config, &job).await {
                tracing::error!(job_id = %job.id, error = %e, "Import job failed");
                handle_job_failure(pool, &job, &e.to_string()).await;
            }
        }
        Ok(None) => {
            tokio::time::sleep(std::time::Duration::from_secs(config.poll_interval_secs)).await;
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to dequeue job");
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }
}

#[derive(Debug, Deserialize)]
struct ImportPayload {
    staging_object_key: String,
    bundle_sha256: String,
    #[allow(dead_code)]
    bundle_size_bytes: i64,
}

async fn process_import_job(
    pool: &PgPool,
    s3_client: &aws_sdk_s3::Client,
    config: &WorkerConfig,
    job: &GithubJob,
) -> Result<(), ArtifactError> {
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

    // Step 1: Download bundle from S3
    tracing::info!(key = %payload.staging_object_key, "Downloading bundle from S3");
    let data =
        download_bundle(s3_client, &config.staging_bucket, &payload.staging_object_key).await?;

    // Step 2: Verify SHA256
    verify_sha256(&data, &payload.bundle_sha256)?;

    // Step 3: Extract bundle
    tracing::info!("Extracting bundle");
    let (manifest, extracted_artifacts) = extract_bundle(&data)?;

    // Step 4: Upload artifacts to final bucket
    tracing::info!(count = extracted_artifacts.len(), "Uploading artifacts to final bucket");

    for manifest_entry in &manifest.artifacts {
        let artifact_id = Uuid::now_v7();
        let storage_key = format!(
            "artifacts/{board_run_id}/{}/{}",
            manifest_entry.r#type, manifest_entry.filename
        );

        if manifest_entry.status == "available" {
            // Find the matching extracted artifact
            if let Some(extracted) = extracted_artifacts
                .iter()
                .find(|a| manifest_entry.source_path.as_deref() == Some(&a.path))
            {
                upload_artifact(
                    s3_client,
                    &config.artifacts_bucket,
                    &storage_key,
                    extracted.data.clone(),
                    &manifest_entry.content_type,
                )
                .await?;

                artifact::insert(
                    pool,
                    artifact_id,
                    board_run_id,
                    &manifest_entry.r#type,
                    "available",
                    Some(&manifest_entry.filename),
                    manifest_entry.source_path.as_deref(),
                    manifest_entry.logical_name.as_deref(),
                    Some(&manifest_entry.content_type),
                    Some(&storage_key),
                    Some(&extracted.sha256),
                    Some(extracted.data.len() as i64),
                    None,
                    Some(bundle.id),
                )
                .await
                .map_err(|e| ArtifactError::S3(e.to_string()))?;
            }
        } else {
            // missing/failed/skipped artifacts
            artifact::insert(
                pool,
                artifact_id,
                board_run_id,
                &manifest_entry.r#type,
                &manifest_entry.status,
                Some(&manifest_entry.filename),
                manifest_entry.source_path.as_deref(),
                manifest_entry.logical_name.as_deref(),
                Some(&manifest_entry.content_type),
                None,
                None,
                None,
                manifest_entry.status_reason.as_deref(),
                Some(bundle.id),
            )
            .await
            .map_err(|e| ArtifactError::S3(e.to_string()))?;
        }
    }

    // Step 5: Save run checks (ERC/DRC)
    let mut erc_status: Option<&str> = None;
    let mut erc_errors = 0i32;
    let mut erc_warnings = 0i32;
    let mut drc_status: Option<&str> = None;
    let mut drc_errors = 0i32;
    let mut drc_warnings = 0i32;

    for check in &manifest.checks {
        let check_id = Uuid::now_v7();
        run_check::insert(
            pool,
            check_id,
            board_run_id,
            &check.kind,
            &check.status,
            check.error_count,
            check.warning_count,
            check.notice_count,
            check.tool_name.as_deref(),
            check.tool_version.as_deref(),
            check.raw_summary.as_ref(),
        )
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;

        match check.kind.as_str() {
            "erc" => {
                erc_status = Some(if check.status == "passed" {
                    "passed"
                } else if check.status == "failed" {
                    "failed"
                } else {
                    "skipped"
                });
                erc_errors = check.error_count;
                erc_warnings = check.warning_count;
            }
            "drc" => {
                drc_status = Some(if check.status == "passed" {
                    "passed"
                } else if check.status == "failed" {
                    "failed"
                } else {
                    "skipped"
                });
                drc_errors = check.error_count;
                drc_warnings = check.warning_count;
            }
            _ => {}
        }
    }

    // Step 6: Create snapshot
    let file_hashes_json =
        serde_json::to_value(&manifest.files).map_err(|e| ArtifactError::Manifest(e.to_string()))?;
    snapshot::insert(
        pool,
        Uuid::now_v7(),
        board_project_id,
        board_run_id,
        &manifest.tree_hash,
        &manifest.commit_sha,
        &file_hashes_json,
    )
    .await
    .map_err(|e| ArtifactError::S3(e.to_string()))?;

    // Step 7: Save diff metadata
    if let Some(ref dm) = manifest.diff_metadata {
        diff::insert_diff_metadata(
            pool,
            Uuid::now_v7(),
            board_run_id,
            dm.file_hashes.as_ref(),
            dm.bom_summary.as_ref(),
            dm.checks_summary.as_ref(),
            dm.artifacts_summary.as_ref(),
            dm.previews.as_ref(),
        )
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;
    }

    // Step 8: Create diff record (no baseline for MVP)
    let diff_status = "no_baseline";
    diff::insert_diff(pool, Uuid::now_v7(), board_run_id, None, diff_status, None)
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;

    // Step 9: Mark board_run as completed
    board_run::mark_completed(
        pool,
        board_run_id,
        erc_status,
        erc_errors,
        erc_warnings,
        drc_status,
        drc_errors,
        drc_warnings,
    )
    .await
    .map_err(|e| ArtifactError::S3(e.to_string()))?;

    // Step 10: Update board_project latest_completed_run_id
    board_project::update_latest_completed_run(
        pool,
        board_project_id,
        board_run_id,
        &manifest.tree_hash,
    )
    .await
    .map_err(|e| ArtifactError::S3(e.to_string()))?;

    // Step 11: Mark bundle as completed
    artifact_bundle::mark_completed(pool, bundle.id)
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;

    // Step 12: Mark job as completed
    github_job::mark_completed(pool, job.id)
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;

    tracing::info!(job_id = %job.id, board_run_id = %board_run_id, "Import job completed successfully");
    Ok(())
}

const MAX_ATTEMPTS: i32 = 5;
const BASE_BACKOFF_SECS: f64 = 10.0;

async fn handle_job_failure(pool: &PgPool, job: &GithubJob, error_message: &str) {
    // Mark bundle as failed if exists
    if let Some(board_run_id) = job.board_run_id {
        if let Ok(Some(bundle)) = artifact_bundle::find_by_board_run_id(pool, board_run_id).await {
            let _ = artifact_bundle::mark_failed(pool, bundle.id, error_message).await;
        }
    }

    if job.attempts >= MAX_ATTEMPTS {
        // Terminal failure
        let _ = github_job::mark_failed(pool, job.id, error_message).await;
        // Also mark board_run as failed
        if let Some(board_run_id) = job.board_run_id {
            let _ = board_run::mark_failed(pool, board_run_id).await;
        }
    } else {
        // Reschedule with backoff
        let backoff = BASE_BACKOFF_SECS * 3_f64.powi(job.attempts);
        let _ = github_job::reschedule(pool, job.id, error_message, backoff).await;
    }
}
