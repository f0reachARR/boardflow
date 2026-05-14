use boardflow_artifact::{ArtifactError, BundleManifest};
use boardflow_db::queries::{
    artifact, artifact_bundle, board_project, board_run, diff, github_job, run_check,
    run_check_finding, snapshot,
};
use boardflow_domain::models::github_job::{GithubJob, GithubJobType};
use sqlx::Postgres;
use uuid::Uuid;

use super::normalize;
use super::s3_ops::UploadedArtifact;

/// Accumulated ERC/DRC check statistics returned by [`persist_checks_and_findings`].
pub(super) struct CheckSummary {
    pub erc_status: Option<&'static str>,
    pub erc_errors: i32,
    pub erc_warnings: i32,
    pub drc_status: Option<&'static str>,
    pub drc_errors: i32,
    pub drc_warnings: i32,
}

/// Insert artifact records for both available and non-available entries in the manifest.
pub(super) async fn persist_artifacts(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    board_run_id: Uuid,
    bundle_id: Uuid,
    manifest: &BundleManifest,
    uploaded: &[UploadedArtifact],
) -> Result<(), ArtifactError> {
    // Insert artifacts (available ones that were uploaded)
    for up in uploaded {
        let manifest_entry = &manifest.artifacts[up.manifest_idx];
        artifact::insert(
            &mut **tx,
            Uuid::now_v7(),
            board_run_id,
            manifest_entry.r#type,
            "available",
            Some(&manifest_entry.filename),
            manifest_entry.source_path.as_deref(),
            manifest_entry.logical_name.as_deref(),
            Some(&manifest_entry.content_type),
            Some(&up.storage_key),
            Some(&up.sha256),
            Some(up.size),
            None,
            Some(bundle_id),
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
            &mut **tx,
            Uuid::now_v7(),
            board_run_id,
            manifest_entry.r#type,
            &manifest_entry.status,
            Some(&manifest_entry.filename),
            manifest_entry.source_path.as_deref(),
            manifest_entry.logical_name.as_deref(),
            Some(&manifest_entry.content_type),
            None,
            None,
            None,
            manifest_entry.status_reason.as_deref(),
            Some(bundle_id),
        )
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;
    }

    Ok(())
}

/// Insert run-check and finding records, returning accumulated ERC/DRC summary.
pub(super) async fn persist_checks_and_findings(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    board_run_id: Uuid,
    manifest: &BundleManifest,
) -> Result<CheckSummary, ArtifactError> {
    let mut erc_status: Option<&str> = None;
    let mut erc_errors = 0i32;
    let mut erc_warnings = 0i32;
    let mut drc_status: Option<&str> = None;
    let mut drc_errors = 0i32;
    let mut drc_warnings = 0i32;

    for check in &manifest.checks {
        let check_id = Uuid::now_v7();
        run_check::insert(
            &mut **tx,
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
        for (idx, raw_finding) in check.findings.iter().enumerate() {
            match serde_json::from_value::<boardflow_artifact::ManifestFinding>(raw_finding.clone())
            {
                Ok(finding) => {
                    let severity = normalize::normalize_severity(finding.severity.as_str());

                    let subject_kind = finding
                        .subject_kind
                        .as_deref()
                        .and_then(normalize::normalize_subject_kind);

                    let (x_um, y_um) = finding
                        .pos_mm
                        .as_ref()
                        .map(|p| {
                            let (x, y) = normalize::pos_mm_to_um(p);
                            (Some(x), Some(y))
                        })
                        .unwrap_or((None, None));

                    if let Err(e) = run_check_finding::insert(
                        &mut **tx,
                        Uuid::now_v7(),
                        check_id,
                        severity,
                        Some(&finding.rule_code),
                        Some(&finding.title),
                        finding.message.as_deref(),
                        subject_kind,
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
                        tracing::error!(
                            check_id = %check_id,
                            sort_index = idx,
                            error = %e,
                            "Failed to insert finding after normalization, skipping"
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        check_id = %check_id,
                        sort_index = idx,
                        error = %e,
                        "Failed to parse finding, storing raw payload"
                    );
                    if let Err(insert_err) = run_check_finding::insert(
                        &mut **tx,
                        Uuid::now_v7(),
                        check_id,
                        "notice",
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
                        Some(raw_finding),
                        idx as i32,
                    )
                    .await
                    {
                        tracing::error!(
                            check_id = %check_id,
                            sort_index = idx,
                            error = %insert_err,
                            "Failed to insert raw finding fallback, skipping"
                        );
                    }
                }
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

    Ok(CheckSummary {
        erc_status: erc_status.map(|s| match s {
            "passed" => "passed",
            "failed" => "failed",
            _ => "skipped",
        }),
        erc_errors,
        erc_warnings,
        drc_status: drc_status.map(|s| match s {
            "passed" => "passed",
            "failed" => "failed",
            _ => "skipped",
        }),
        drc_errors,
        drc_warnings,
    })
}

/// Insert snapshot, diff-metadata, and diff records.
pub(super) async fn persist_snapshot_and_diff(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    board_project_id: Uuid,
    board_run_id: Uuid,
    manifest: &BundleManifest,
) -> Result<(), ArtifactError> {
    // Create snapshot
    let file_hashes_json = serde_json::to_value(&manifest.files)
        .map_err(|e| ArtifactError::Manifest(e.to_string()))?;
    snapshot::insert(
        &mut **tx,
        Uuid::now_v7(),
        board_project_id,
        board_run_id,
        &manifest.tree_hash,
        &manifest.commit_sha,
        &file_hashes_json,
    )
    .await
    .map_err(|e| ArtifactError::S3(e.to_string()))?;

    // Save diff metadata
    if let Some(ref dm) = manifest.diff_metadata {
        diff::insert_diff_metadata(
            &mut **tx,
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

    // Resolve baseline and create diff record
    let bp = board_project::find_by_id(&mut **tx, board_project_id)
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
        let file_count = manifest.files.len();
        Some(serde_json::json!({
            "total_files": file_count,
            "status": "computed"
        }))
    } else {
        None
    };

    diff::insert_diff(
        &mut **tx,
        Uuid::now_v7(),
        board_run_id,
        base_run_id,
        diff_status,
        summary_json.as_ref(),
    )
    .await
    .map_err(|e| ArtifactError::S3(e.to_string()))?;

    Ok(())
}

/// Mark the board-run, board-project, artifact-bundle, and github-job as completed.
pub(super) async fn complete_run(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    board_project_id: Uuid,
    board_run_id: Uuid,
    bundle_id: Uuid,
    job_id: Uuid,
    tree_hash: &str,
    check_summary: &CheckSummary,
) -> Result<(), ArtifactError> {
    // Mark board_run as completed
    board_run::mark_completed(
        &mut **tx,
        board_run_id,
        check_summary.erc_status,
        check_summary.erc_errors,
        check_summary.erc_warnings,
        check_summary.drc_status,
        check_summary.drc_errors,
        check_summary.drc_warnings,
    )
    .await
    .map_err(|e| ArtifactError::S3(e.to_string()))?;

    // Update board_project latest_completed_run_id
    board_project::update_latest_completed_run(
        &mut **tx,
        board_project_id,
        board_run_id,
        tree_hash,
    )
    .await
    .map_err(|e| ArtifactError::S3(e.to_string()))?;

    // Mark bundle as completed
    artifact_bundle::mark_completed(&mut **tx, bundle_id)
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;

    // Mark job as completed
    github_job::mark_completed(&mut **tx, job_id)
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;

    Ok(())
}

/// Enqueue follow-up jobs (create-issue, dashboard-comment, run-result-comment).
pub(super) async fn enqueue_follow_up_jobs(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    job: &GithubJob,
    board_project_id: Uuid,
    board_run_id: Uuid,
) -> Result<(), ArtifactError> {
    // 1. Issue sync: if board_project has no issue_number, enqueue create_issue
    let bp_for_jobs = board_project::find_by_id(&mut **tx, board_project_id)
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;

    if bp_for_jobs
        .as_ref()
        .and_then(|bp| bp.issue_number)
        .is_none()
    {
        let _ = github_job::enqueue(
            &mut **tx,
            Uuid::now_v7(),
            job.installation_id,
            job.repository_id,
            Some(board_project_id),
            Some(board_run_id),
            GithubJobType::CreateIssue,
            &serde_json::json!({}),
        )
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;
    }

    // 2. Dashboard comment: create or update based on board_project.dashboard_comment_id
    let dashboard_job_type = if bp_for_jobs
        .as_ref()
        .and_then(|bp| bp.dashboard_comment_id)
        .is_some()
    {
        GithubJobType::UpdateDashboardComment
    } else {
        GithubJobType::CreateDashboardComment
    };
    let _ = github_job::enqueue(
        &mut **tx,
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
        &mut **tx,
        Uuid::now_v7(),
        job.installation_id,
        job.repository_id,
        Some(board_project_id),
        Some(board_run_id),
        GithubJobType::CreateRunResultComment,
        &serde_json::json!({}),
    )
    .await
    .map_err(|e| ArtifactError::S3(e.to_string()))?;

    Ok(())
}
