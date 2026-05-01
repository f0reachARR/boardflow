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

    // Need an existing dashboard comment to update
    let comment_id = match bp.dashboard_comment_id {
        Some(id) => id as u64,
        None => {
            // No dashboard comment yet — this shouldn't happen but handle gracefully
            tracing::warn!(
                job_id = %job.id,
                "update_dashboard_comment: no dashboard_comment_id, skipping"
            );
            return HandlerResult::Completed;
        }
    };

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

    // Update comment via GitHub API
    if let Err(e) = github_client
        .update_comment(installation_id, &bp.repo_owner, &bp.repo_name, comment_id, &body)
        .await
    {
        return HandlerResult::Reschedule {
            reason: format!("GitHub API error: {e}"),
            backoff_secs: boardflow_jobs::backoff_secs(job.attempts),
        };
    }

    tracing::info!(
        job_id = %job.id,
        comment_id = comment_id,
        "Updated dashboard comment"
    );

    HandlerResult::Completed
}
