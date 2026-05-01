use boardflow_db::queries::{board_project, board_run};
use boardflow_domain::models::github_job::GithubJob;
use boardflow_github::{GitHubAppClient, GitHubClientError};
use boardflow_github::types::IssueState;
use sqlx::PgPool;

use crate::comment_body;
use crate::config::WorkerConfig;

use super::{tree_hash_changed, HandlerResult};

pub async fn handle(
    pool: &PgPool,
    github_client: &dyn GitHubAppClient,
    config: &WorkerConfig,
    job: &GithubJob,
) -> HandlerResult {
    let board_project_id = match job.board_project_id {
        Some(id) => id,
        None => {
            return HandlerResult::Failed {
                reason: "job missing board_project_id".into(),
            };
        }
    };

    let board_run_id = match job.board_run_id {
        Some(id) => id,
        None => {
            return HandlerResult::Failed {
                reason: "job missing board_run_id".into(),
            };
        }
    };

    // Fetch board project with repository info
    let bp = match board_project::find_by_id_with_repository(pool, board_project_id).await {
        Ok(Some(bp)) => bp,
        Ok(None) => {
            return HandlerResult::Failed {
                reason: format!("board_project {board_project_id} not found"),
            };
        }
        Err(e) => {
            return HandlerResult::Reschedule {
                reason: format!("DB error: {e}"),
                backoff_secs: boardflow_jobs::backoff_secs(job.attempts),
            };
        }
    };

    // Need an issue to post comment on
    let issue_number = match bp.issue_number {
        Some(n) => n as u64,
        None => {
            // Issue not created yet, reschedule (create_issue should run first)
            return HandlerResult::Reschedule {
                reason: "issue not yet created, waiting for create_issue job".into(),
                backoff_secs: 5.0,
            };
        }
    };

    // If dashboard comment already exists, skip (idempotency)
    if bp.dashboard_comment_id.is_some() {
        return HandlerResult::Completed;
    }

    let installation_id = bp.repo_installation_id as u64;

    // Check Issue state (closed/404 detection per spec 11.7 / 13.1)
    match github_client
        .get_issue(installation_id, &bp.repo_owner, &bp.repo_name, issue_number)
        .await
    {
        Ok(issue_info) => {
            if issue_info.state == IssueState::Closed {
                // Issue is closed — check recreate_issue_on_update
                if !bp.recreate_issue_on_update {
                    tracing::info!(job_id = %job.id, "Issue is closed and recreate_issue_on_update=false, stopping");
                    return HandlerResult::Completed;
                }
                // Only recreate if tree_hash has changed since the previous completed run
                match tree_hash_changed(pool, board_project_id, board_run_id).await {
                    Ok(false) => {
                        tracing::info!(job_id = %job.id, "Issue is closed but tree_hash unchanged, skipping recreation");
                        return HandlerResult::Completed;
                    }
                    Ok(true) => { /* proceed with recreation */ }
                    Err(e) => {
                        return HandlerResult::Reschedule {
                            reason: format!("DB error checking tree_hash: {e}"),
                            backoff_secs: boardflow_jobs::backoff_secs(job.attempts),
                        };
                    }
                }
                // Need to recreate: clear issue info and reschedule (create_issue will handle)
                let _ = board_project::clear_issue_info(pool, board_project_id).await;
                let _ = boardflow_db::queries::github_job::enqueue(
                    pool,
                    uuid::Uuid::now_v7(),
                    job.installation_id,
                    job.repository_id,
                    job.board_project_id,
                    job.board_run_id,
                    "create_issue",
                    &serde_json::json!({}),
                )
                .await;
                return HandlerResult::Reschedule {
                    reason: "Issue closed, enqueued create_issue for recreation".into(),
                    backoff_secs: 5.0,
                };
            }
        }
        Err(GitHubClientError::NotFound(_)) => {
            // Issue not found (404) — clear issue info so create_issue re-runs
            tracing::warn!(job_id = %job.id, "Issue not found (404), clearing issue info");
            let _ = board_project::clear_issue_info(pool, board_project_id).await;
            let _ = boardflow_db::queries::github_job::enqueue(
                pool,
                uuid::Uuid::now_v7(),
                job.installation_id,
                job.repository_id,
                job.board_project_id,
                job.board_run_id,
                "create_issue",
                &serde_json::json!({}),
            )
            .await;
            return HandlerResult::Reschedule {
                reason: "Issue 404, cleared issue info and enqueued create_issue".into(),
                backoff_secs: 5.0,
            };
        }
        Err(e) => {
            return handle_github_error(e, job.attempts);
        }
    }

    // Fetch the board run for comment content
    let run = match board_run::find_by_id(pool, board_run_id).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            return HandlerResult::Failed {
                reason: format!("board_run {board_run_id} not found"),
            };
        }
        Err(e) => {
            return HandlerResult::Reschedule {
                reason: format!("DB error: {e}"),
                backoff_secs: boardflow_jobs::backoff_secs(job.attempts),
            };
        }
    };

    let body = comment_body::dashboard_comment(
        &bp.project_path,
        &run,
        board_project_id,
        bp.github_repository_id,
        &config.app_base_url,
    );

    // Create comment via GitHub API
    let created = match github_client
        .create_comment(installation_id, &bp.repo_owner, &bp.repo_name, issue_number, &body)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            return handle_github_error(e, job.attempts);
        }
    };

    // Update board_project with dashboard_comment_id
    if let Err(e) =
        board_project::update_dashboard_comment_id(pool, board_project_id, created.id as i64).await
    {
        tracing::error!(error = %e, "Failed to update dashboard_comment_id");
        return HandlerResult::Reschedule {
            reason: format!("DB error updating dashboard_comment_id: {e}"),
            backoff_secs: boardflow_jobs::backoff_secs(job.attempts),
        };
    }

    tracing::info!(
        job_id = %job.id,
        comment_id = created.id,
        "Created dashboard comment"
    );

    HandlerResult::Completed
}

fn handle_github_error(e: GitHubClientError, attempts: i32) -> HandlerResult {
    match e {
        GitHubClientError::RateLimited { retry_after_secs } => {
            let backoff = retry_after_secs
                .map(|s| s as f64)
                .unwrap_or_else(|| boardflow_jobs::backoff_secs(attempts) * 2.0);
            HandlerResult::Reschedule {
                reason: format!("Rate limited: {e}"),
                backoff_secs: backoff,
            }
        }
        GitHubClientError::Auth(_) => HandlerResult::Reschedule {
            reason: format!("Auth error: {e}"),
            backoff_secs: boardflow_jobs::backoff_secs(attempts),
        },
        _ => HandlerResult::Reschedule {
            reason: format!("GitHub API error: {e}"),
            backoff_secs: boardflow_jobs::backoff_secs(attempts),
        },
    }
}
