use boardflow_db::queries::{board_project, board_run};
use boardflow_domain::models::github_job::GithubJob;
use boardflow_github::GitHubAppClient;
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

    let installation_id = bp.repo_installation_id as u64;
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
            return HandlerResult::Reschedule {
                reason: format!("GitHub API error: {e}"),
                backoff_secs: boardflow_jobs::backoff_secs(job.attempts),
            };
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
