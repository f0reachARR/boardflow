use boardflow_db::queries::{board_project, github_job};
use boardflow_domain::models::github_job::GithubJob;
use boardflow_github::{GitHubAppClient, GitHubClientError};
use sqlx::PgPool;

use crate::comment_body;
use crate::config::WorkerConfig;

use super::HandlerResult;

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

    // If issue already exists, mark as completed (idempotency)
    if bp.issue_number.is_some() {
        return HandlerResult::Completed;
    }

    let installation_id = bp.repo_installation_id as u64;
    let title = comment_body::issue_title(&bp.display_name);
    let body = comment_body::issue_body(
        bp.github_repository_id,
        &bp.project_path,
        board_project_id,
        &config.app_base_url,
        bp.latest_completed_run_id,
    );

    // Create the issue via GitHub API
    let created = match github_client
        .create_issue(installation_id, &bp.repo_owner, &bp.repo_name, &title, &body)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            return handle_github_error(e, job.attempts);
        }
    };

    // Update board_project with issue info
    if let Err(e) = board_project::update_issue_info(
        pool,
        board_project_id,
        created.number as i32,
        &created.node_id,
        &created.html_url,
    )
    .await
    {
        tracing::error!(error = %e, "Failed to update board_project issue info");
        return HandlerResult::Reschedule {
            reason: format!("DB error updating issue info: {e}"),
            backoff_secs: boardflow_jobs::backoff_secs(job.attempts),
        };
    }

    // Now enqueue create_dashboard_comment since we have an issue
    let _ = github_job::enqueue(
        pool,
        uuid::Uuid::now_v7(),
        job.installation_id,
        job.repository_id,
        job.board_project_id,
        job.board_run_id,
        "create_dashboard_comment",
        &serde_json::json!({}),
    )
    .await;

    tracing::info!(
        job_id = %job.id,
        issue_number = created.number,
        "Created issue for board project"
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
