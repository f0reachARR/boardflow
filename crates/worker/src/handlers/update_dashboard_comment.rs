use boardflow_db::queries::{board_project, board_run};
use boardflow_domain::models::github_job::GithubJob;
use boardflow_github::types::IssueState;
use boardflow_github::{GitHubAppClient, GitHubClientError};
use sqlx::PgPool;

use crate::comment_body;
use crate::config::WorkerConfig;

use super::{HandlerResult, tree_hash_changed};

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

    // Need an issue to post/update comment on
    let issue_number = match bp.issue_number {
        Some(n) => n as u64,
        None => {
            return HandlerResult::Reschedule {
                reason: "issue not yet created, waiting for create_issue job".into(),
                backoff_secs: 5.0,
            };
        }
    };

    let installation_id = bp.repo_installation_id as u64;

    // Check Issue state (closed/404 detection per spec 11.7 / 13.1)
    match github_client
        .get_issue(installation_id, &bp.repo_owner, &bp.repo_name, issue_number)
        .await
    {
        Ok(issue_info) => {
            if issue_info.state == IssueState::Closed {
                if !bp.recreate_issue_on_update {
                    tracing::info!(job_id = %job.id, "Issue is closed and recreate_issue_on_update=false, stopping");
                    return HandlerResult::Completed;
                }
                // Only recreate if tree_hash has changed since the previous completed run
                // Use latest_completed_run_id to avoid stale job issues
                let effective_run_id = bp.latest_completed_run_id.unwrap_or(board_run_id);
                match tree_hash_changed(pool, board_project_id, effective_run_id).await {
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
                // Recreate: clear issue info, enqueue create_issue, reschedule
                if let (Some(num), Some(node_id), Some(url)) = (
                    bp.issue_number,
                    bp.issue_node_id.as_deref(),
                    bp.issue_url.as_deref(),
                ) {
                    if let Err(e) = board_project::insert_issue_history(
                        pool,
                        uuid::Uuid::now_v7(),
                        board_project_id,
                        num,
                        node_id,
                        url,
                        "recreated",
                        None,
                    )
                    .await
                    {
                        tracing::warn!(error = %e, "Failed to insert issue history");
                    }
                }
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
            tracing::warn!(job_id = %job.id, "Issue not found (404), clearing issue info");
            if let (Some(num), Some(node_id), Some(url)) = (
                bp.issue_number,
                bp.issue_node_id.as_deref(),
                bp.issue_url.as_deref(),
            ) {
                if let Err(e) = board_project::insert_issue_history(
                    pool,
                    uuid::Uuid::now_v7(),
                    board_project_id,
                    num,
                    node_id,
                    url,
                    "deleted",
                    None,
                )
                .await
                {
                    tracing::warn!(error = %e, "Failed to insert issue history");
                }
            }
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

    // Debounce: always use latest_completed_run_id from board_project so that
    // regardless of which update job executes last, we produce the current state.
    let effective_run_id = bp.latest_completed_run_id.unwrap_or(board_run_id);
    let run = match board_run::find_by_id(pool, effective_run_id).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            return HandlerResult::Failed {
                reason: format!("board_run {effective_run_id} not found"),
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
        &config.app_domain,
    );

    // Fallback: if dashboard_comment_id is None, create instead of update
    let comment_id = match bp.dashboard_comment_id {
        Some(id) => id as u64,
        None => {
            // No existing comment — create one (fallback per spec)
            return create_dashboard_comment_fallback(
                pool,
                github_client,
                installation_id,
                &bp.repo_owner,
                &bp.repo_name,
                issue_number,
                &body,
                board_project_id,
                job,
            )
            .await;
        }
    };

    // Update comment via GitHub API
    match github_client
        .update_comment(
            installation_id,
            &bp.repo_owner,
            &bp.repo_name,
            comment_id,
            &body,
        )
        .await
    {
        Ok(()) => {
            tracing::info!(
                job_id = %job.id,
                comment_id = comment_id,
                "Updated dashboard comment"
            );
            HandlerResult::Completed
        }
        Err(GitHubClientError::NotFound(_)) => {
            // Comment was deleted — clear dashboard_comment_id and recreate
            tracing::warn!(job_id = %job.id, "Dashboard comment 404, recreating");
            let _ = board_project::clear_dashboard_comment_id(pool, board_project_id).await;
            create_dashboard_comment_fallback(
                pool,
                github_client,
                installation_id,
                &bp.repo_owner,
                &bp.repo_name,
                issue_number,
                &body,
                board_project_id,
                job,
            )
            .await
        }
        Err(e) => handle_github_error(e, job.attempts),
    }
}

/// Create a dashboard comment as fallback (when dashboard_comment_id is None or comment was 404)
#[allow(clippy::too_many_arguments)]
async fn create_dashboard_comment_fallback(
    pool: &PgPool,
    github_client: &dyn GitHubAppClient,
    installation_id: u64,
    owner: &str,
    repo: &str,
    issue_number: u64,
    body: &str,
    board_project_id: uuid::Uuid,
    job: &GithubJob,
) -> HandlerResult {
    match github_client
        .create_comment(installation_id, owner, repo, issue_number, body)
        .await
    {
        Ok(created) => {
            if let Err(e) = board_project::update_dashboard_comment_id(
                pool,
                board_project_id,
                created.id as i64,
            )
            .await
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
                "Created dashboard comment (fallback)"
            );
            HandlerResult::Completed
        }
        Err(e) => handle_github_error(e, job.attempts),
    }
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
