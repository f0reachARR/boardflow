use boardflow_artifact::{
    download_bundle, extract_bundle, upload_artifact, verify_sha256, ArtifactError,
};
use boardflow_db::queries::{
    artifact, artifact_bundle, board_project, board_run, diff, github_job, run_check,
    run_check_finding, snapshot,
};
use boardflow_domain::models::github_job::GithubJob;
use boardflow_jobs::{BASE_BACKOFF_SECS, MAX_ATTEMPTS};
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

    let s3_config = {
        let mut builder = aws_sdk_s3::config::Builder::new()
            .region(aws_sdk_s3::config::Region::new("us-east-1"));

        if let Some(ref endpoint) = config.s3_endpoint {
            builder = builder.endpoint_url(endpoint).force_path_style(true);
        }

        if let (Some(access_key), Some(secret_key)) =
            (&config.s3_access_key, &config.s3_secret_key)
        {
            builder = builder.credentials_provider(aws_sdk_s3::config::Credentials::new(
                access_key,
                secret_key,
                None,
                None,
                "env",
            ));
        }

        builder.build()
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

    // === Pre-transaction: S3 operations ===

    // Step 1: Download bundle from S3
    tracing::info!(key = %payload.staging_object_key, "Downloading bundle from S3");
    let data =
        download_bundle(s3_client, &config.staging_bucket, &payload.staging_object_key).await?;

    // Step 2: Verify SHA256
    verify_sha256(&data, &payload.bundle_sha256)?;

    // Step 3: Extract bundle
    tracing::info!("Extracting bundle");
    let (manifest, extracted_artifacts) = extract_bundle(&data)?;

    // Step 4: Upload artifacts to final bucket (before transaction)
    tracing::info!(count = extracted_artifacts.len(), "Uploading artifacts to final bucket");

    // Prepare upload results for use inside transaction
    struct UploadedArtifact {
        storage_key: String,
        sha256: String,
        size: i64,
        manifest_idx: usize,
    }
    let mut uploaded: Vec<UploadedArtifact> = Vec::new();

    for (idx, manifest_entry) in manifest.artifacts.iter().enumerate() {
        if manifest_entry.status != "available" {
            continue;
        }
        let storage_key = format!(
            "artifacts/{board_run_id}/{}/{}",
            manifest_entry.r#type, manifest_entry.filename
        );

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

            uploaded.push(UploadedArtifact {
                storage_key,
                sha256: extracted.sha256.clone(),
                size: extracted.data.len() as i64,
                manifest_idx: idx,
            });
        }
    }

    // === Transaction: all DB writes ===
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;

    // Insert artifacts (available ones that were uploaded)
    for up in &uploaded {
        let manifest_entry = &manifest.artifacts[up.manifest_idx];
        artifact::insert(
            &mut *tx,
            Uuid::now_v7(),
            board_run_id,
            &manifest_entry.r#type,
            "available",
            Some(&manifest_entry.filename),
            manifest_entry.source_path.as_deref(),
            manifest_entry.logical_name.as_deref(),
            Some(&manifest_entry.content_type),
            Some(&up.storage_key),
            Some(&up.sha256),
            Some(up.size),
            None,
            Some(bundle.id),
        )
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;
    }

    // Insert non-available artifacts
    for manifest_entry in &manifest.artifacts {
        if manifest_entry.status == "available" {
            continue;
        }
        artifact::insert(
            &mut *tx,
            Uuid::now_v7(),
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
            &mut *tx,
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

        // Insert findings for this check
        for (idx, finding) in check.findings.iter().enumerate() {
            let x_um = finding.pos_mm.as_ref().map(|p| (p.x * 1000.0) as i32);
            let y_um = finding.pos_mm.as_ref().map(|p| (p.y * 1000.0) as i32);

            if let Err(e) = run_check_finding::insert(
                &mut *tx,
                Uuid::now_v7(),
                check_id,
                &finding.severity,
                Some(&finding.rule_code),
                Some(&finding.title),
                finding.message.as_deref(),
                finding.subject_kind.as_deref(),
                finding.subject_ref.as_deref(),
                finding.sheet_path.as_deref(),
                finding.pcb_layer.as_deref(),
                x_um,
                y_um,
                None,
                finding.raw.as_ref(),
                idx as i32,
            )
            .await
            {
                tracing::warn!(
                    check_id = %check_id,
                    sort_index = idx,
                    error = %e,
                    "Failed to insert finding, storing raw payload"
                );
                let raw_fallback = serde_json::to_value(finding).ok();
                let _ = run_check_finding::insert(
                    &mut *tx,
                    Uuid::now_v7(),
                    check_id,
                    &finding.severity,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    raw_fallback.as_ref(),
                    idx as i32,
                )
                .await
                .ok();
            }
        }

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
        &mut *tx,
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
            &mut *tx,
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

    // Step 8: Resolve baseline and create diff record
    let bp = board_project::find_by_id(&mut *tx, board_project_id)
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;
    let base_run_id = bp.and_then(|p| p.latest_completed_run_id);
    let diff_status = if base_run_id.is_some() {
        "ready"
    } else {
        "no_baseline"
    };

    // Compute diff summary from file_hashes if we have a baseline
    let summary_json = if base_run_id.is_some() {
        // Simple summary: count files
        let file_count = manifest.files.len();
        Some(serde_json::json!({
            "total_files": file_count,
            "status": "computed"
        }))
    } else {
        None
    };

    diff::insert_diff(
        &mut *tx,
        Uuid::now_v7(),
        board_run_id,
        base_run_id,
        diff_status,
        summary_json.as_ref(),
    )
    .await
    .map_err(|e| ArtifactError::S3(e.to_string()))?;

    // Step 9: Mark board_run as completed
    board_run::mark_completed(
        &mut *tx,
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
        &mut *tx,
        board_project_id,
        board_run_id,
        &manifest.tree_hash,
    )
    .await
    .map_err(|e| ArtifactError::S3(e.to_string()))?;

    // Step 11: Mark bundle as completed
    artifact_bundle::mark_completed(&mut *tx, bundle.id)
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;

    // Step 12: Mark job as completed
    github_job::mark_completed(&mut *tx, job.id)
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;

    // Step 13: Enqueue follow-up jobs

    // 1. Issue sync: if board_project has no issue_number, enqueue create_issue
    let bp_for_jobs = board_project::find_by_id(&mut *tx, board_project_id)
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;

    if bp_for_jobs.as_ref().and_then(|bp| bp.issue_number).is_none() {
        let _ = github_job::enqueue(
            &mut *tx,
            Uuid::now_v7(),
            job.installation_id,
            job.repository_id,
            Some(board_project_id),
            Some(board_run_id),
            "create_issue",
            &serde_json::json!({}),
        )
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;
    }

    // 2. Dashboard comment: create or update based on board_project.dashboard_comment_id
    let dashboard_job_type = if bp_for_jobs.as_ref().and_then(|bp| bp.dashboard_comment_id).is_some() {
        "update_dashboard_comment"
    } else {
        "create_dashboard_comment"
    };
    let _ = github_job::enqueue(
        &mut *tx,
        Uuid::now_v7(),
        job.installation_id,
        job.repository_id,
        Some(board_project_id),
        Some(board_run_id),
        dashboard_job_type,
        &serde_json::json!({}),
    )
    .await
    .map_err(|e| ArtifactError::S3(e.to_string()))?;

    // 3. Run result comment
    let _ = github_job::enqueue(
        &mut *tx,
        Uuid::now_v7(),
        job.installation_id,
        job.repository_id,
        Some(board_project_id),
        Some(board_run_id),
        "create_run_result_comment",
        &serde_json::json!({}),
    )
    .await
    .map_err(|e| ArtifactError::S3(e.to_string()))?;

    // Commit transaction
    tx.commit()
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;

    tracing::info!(job_id = %job.id, board_run_id = %board_run_id, "Import job completed successfully");
    Ok(())
}

async fn handle_job_failure(pool: &PgPool, job: &GithubJob, error_message: &str) {
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
