use boardflow_db::queries::{board_project, github_job};
use boardflow_github::GitHubAppClient;
use boardflow_jobs::MAX_ATTEMPTS;
use sqlx::PgPool;

use crate::config::WorkerConfig;
use crate::handlers::{
    self, create_dashboard_comment, create_issue, create_run_result_comment,
    update_dashboard_comment, HandlerResult,
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
                    HandlerResult::Reschedule { reason, backoff_secs: backoff } => {
                        tracing::warn!(job_id = %job.id, reason = %reason, "Rescheduling job");
                        if job.attempts >= MAX_ATTEMPTS {
                            let _ = github_job::mark_failed(pool, job.id, &reason).await;
                            // Mark issue_sync_status as failed for create_issue terminal failures
                            if job_type == "create_issue" {
                                if let Some(bp_id) = job.board_project_id {
                                    let _ = board_project::update_issue_sync_status(pool, bp_id, "failed").await;
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
                                let _ = board_project::update_issue_sync_status(pool, bp_id, "failed").await;
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
