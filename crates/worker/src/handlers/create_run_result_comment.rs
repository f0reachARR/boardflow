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
            // Issue not created yet, reschedule
            return HandlerResult::Reschedule {
                reason: "issue not yet created, waiting for create_issue job".into(),
                backoff_secs: 5.0,
            };
        }
    };

    // Fetch current board run
    let current_run = match board_run::find_by_id(pool, board_run_id).await {
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

    // Fetch previous completed run to determine whether to post
    let previous_run =
        match board_run::find_previous_completed(pool, board_project_id, board_run_id).await {
            Ok(r) => r,
            Err(e) => {
                return HandlerResult::Reschedule {
                    reason: format!("DB error: {e}"),
                    backoff_secs: boardflow_jobs::backoff_secs(job.attempts),
                };
            }
        };

    // Check if we should post a run result comment
    if !comment_body::should_post_run_result(&current_run, previous_run.as_ref()) {
        tracing::info!(
            job_id = %job.id,
            "Skipping run result comment (no significant change)"
        );
        return HandlerResult::Completed;
    }

    let installation_id = bp.repo_installation_id as u64;
    let body = comment_body::run_result_comment(
        &current_run,
        board_project_id,
        bp.github_repository_id,
        &config.app_base_url,
    );

    // Create comment via GitHub API
    match github_client
        .create_comment(installation_id, &bp.repo_owner, &bp.repo_name, issue_number, &body)
        .await
    {
        Ok(created) => {
            tracing::info!(
                job_id = %job.id,
                comment_id = created.id,
                "Created run result comment"
            );
            HandlerResult::Completed
        }
        Err(e) => HandlerResult::Reschedule {
            reason: format!("GitHub API error: {e}"),
            backoff_secs: boardflow_jobs::backoff_secs(job.attempts),
        },
    }
}
