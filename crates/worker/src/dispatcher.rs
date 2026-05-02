use boardflow_db::queries::{artifact_bundle, board_project, board_run, github_job};
use boardflow_github::GitHubAppClient;
use boardflow_jobs::MAX_ATTEMPTS;
use sqlx::PgPool;

use crate::config::WorkerConfig;
use crate::handlers::{
    self, HandlerResult, create_dashboard_comment, create_issue, create_run_result_comment,
    update_dashboard_comment,
};

/// Job types in priority order.
const JOB_TYPES: &[&str] = &[
    "artifact_bundle_import",
    "create_issue",
    "create_dashboard_comment",
    "update_dashboard_comment",
    "create_run_result_comment",
];

/// Poll for jobs and dispatch to the appropriate handler.
/// Returns after processing one job or sleeping if no jobs are available.
pub async fn poll_and_dispatch(
    pool: &PgPool,
    s3_client: &aws_sdk_s3::Client,
    config: &WorkerConfig,
    github_client: Option<&dyn GitHubAppClient>,
) {
    // Try each job type in priority order
    for &job_type in JOB_TYPES {
        match github_job::dequeue(pool, job_type).await {
            Ok(Some(job)) => {
                tracing::info!(job_id = %job.id, job_type = job_type, "Dequeued job");

                let result = match job_type {
                    "artifact_bundle_import" => {
                        handlers::import::handle(pool, s3_client, config, &job).await
                    }
                    "create_issue" => match github_client {
                        Some(client) => create_issue::handle(pool, client, config, &job).await,
                        None => no_github_client_result(),
                    },
                    "create_dashboard_comment" => match github_client {
                        Some(client) => {
                            create_dashboard_comment::handle(pool, client, config, &job).await
                        }
                        None => no_github_client_result(),
                    },
                    "update_dashboard_comment" => match github_client {
                        Some(client) => {
                            update_dashboard_comment::handle(pool, client, config, &job).await
                        }
                        None => no_github_client_result(),
                    },
                    "create_run_result_comment" => match github_client {
                        Some(client) => {
                            create_run_result_comment::handle(pool, client, config, &job).await
                        }
                        None => no_github_client_result(),
                    },
                    _ => {
                        tracing::error!(job_type = job_type, "Unknown job type");
                        HandlerResult::Failed {
                            reason: format!("unknown job type: {job_type}"),
                        }
                    }
                };

                // Handle result (import handler manages its own completion/failure)
                match result {
                    HandlerResult::Completed => {
                        // For non-import jobs, mark completed here
                        if job_type != "artifact_bundle_import" {
                            if let Err(e) = github_job::mark_completed(pool, job.id).await {
                                tracing::error!(error = %e, "Failed to mark job completed");
                            }
                        }
                    }
                    HandlerResult::Reschedule {
                        reason,
                        backoff_secs: backoff,
                    } => {
                        tracing::warn!(job_id = %job.id, reason = %reason, "Rescheduling job");
                        if job.attempts >= MAX_ATTEMPTS {
                            let _ = github_job::mark_failed(pool, job.id, &reason).await;
                            // Mark issue_sync_status as failed for create_issue terminal failures
                            if job_type == "create_issue" {
                                if let Some(bp_id) = job.board_project_id {
                                    let _ = board_project::update_issue_sync_status(
                                        pool, bp_id, "failed",
                                    )
                                    .await;
                                }
                            }
                        } else {
                            let _ = github_job::reschedule(pool, job.id, &reason, backoff).await;
                        }
                    }
                    HandlerResult::Failed { reason } => {
                        tracing::error!(job_id = %job.id, reason = %reason, "Job failed terminally");
                        let _ = github_job::mark_failed(pool, job.id, &reason).await;
                        // Mark issue_sync_status as failed for create_issue terminal failures
                        if job_type == "create_issue" {
                            if let Some(bp_id) = job.board_project_id {
                                let _ =
                                    board_project::update_issue_sync_status(pool, bp_id, "failed")
                                        .await;
                            }
                        }
                    }
                }

                return; // Processed one job, return to loop
            }
            Ok(None) => {
                // No job of this type available, try next type
                continue;
            }
            Err(e) => {
                tracing::error!(job_type = job_type, error = %e, "Failed to dequeue job");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                return;
            }
        }
    }

    // No jobs available for any type, sleep
    tokio::time::sleep(std::time::Duration::from_secs(config.poll_interval_secs)).await;
}

fn no_github_client_result() -> HandlerResult {
    tracing::warn!("GitHub client not configured, rescheduling job");
    HandlerResult::Reschedule {
        reason: "GitHub client not configured".into(),
        backoff_secs: 60.0,
    }
}

/// Sweep stale BoardRuns that exceeded the 12-hour timeout.
pub async fn sweep_timed_out_runs(pool: &PgPool) {
    match board_run::sweep_timed_out(pool).await {
        Ok(ids) if !ids.is_empty() => {
            tracing::info!(count = ids.len(), "Swept timed-out BoardRuns");
            // Set delete_after on staging bundles for timed-out runs
            match artifact_bundle::set_delete_after_for_timed_out_runs(pool, &ids).await {
                Ok(n) if n > 0 => {
                    tracing::info!(
                        count = n,
                        "Set delete_after on staging bundles for timed-out runs"
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(error = %e, "Failed to set delete_after on staging bundles");
                }
            }
        }
        Ok(_) => {
            tracing::debug!("No BoardRuns to time out");
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to sweep timed-out BoardRuns");
        }
    }
}

/// Delete expired staging bundles from S3 and clear their object keys.
pub async fn sweep_expired_staging_bundles(
    pool: &PgPool,
    s3_client: &aws_sdk_s3::Client,
    config: &WorkerConfig,
) {
    // Self-healing: repair orphaned bundles from terminal runs
    match artifact_bundle::repair_orphaned_staging_bundles(pool).await {
        Ok(n) if n > 0 => {
            tracing::info!(
                count = n,
                "Repaired orphaned staging bundles (set delete_after)"
            );
        }
        Ok(_) => {}
        Err(e) => {
            tracing::error!(error = %e, "Failed to repair orphaned staging bundles");
        }
    }

    let bundles = match artifact_bundle::find_expired_staging(pool).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, "Failed to query expired staging bundles");
            return;
        }
    };

    if bundles.is_empty() {
        tracing::debug!("No expired staging bundles to clean up");
        return;
    }

    let mut deleted = 0u64;
    for bundle in &bundles {
        let key = bundle.staging_object_key.as_deref().unwrap();
        match s3_client
            .delete_object()
            .bucket(&config.staging_bucket)
            .key(key)
            .send()
            .await
        {
            Ok(_) => {
                if let Err(e) = artifact_bundle::clear_staging_object_key(pool, bundle.id).await {
                    tracing::error!(bundle_id = %bundle.id, error = %e, "Failed to clear staging_object_key");
                } else {
                    deleted += 1;
                }
            }
            Err(e) => {
                tracing::warn!(bundle_id = %bundle.id, key = key, error = %e, "Failed to delete staging object, will retry next sweep");
            }
        }
    }

    tracing::info!(
        deleted = deleted,
        total = bundles.len(),
        "Swept expired staging bundles"
    );
}
