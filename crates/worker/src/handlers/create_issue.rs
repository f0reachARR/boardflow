use boardflow_db::queries::{board_project, github_job};
use boardflow_domain::models::github_job::GithubJob;
use boardflow_domain::public_ids::{BoardProjectId, BoardRunId};
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

    // Phase 1: Read board_project (no lock) for early checks and API call preparation.
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

    // Fast idempotency check (without lock). If issue already exists, done.
    if bp.issue_number.is_some() {
        return HandlerResult::Completed;
    }

    let installation_id = bp.repo_installation_id as u64;
    let title = comment_body::issue_title(&bp.display_name);
    let body = comment_body::issue_body(
        bp.github_repository_id,
        &bp.project_path,
        BoardProjectId::from(board_project_id),
        &config.app_domain,
        bp.latest_completed_run_id.map(BoardRunId::from),
    );

    // Phase 2: Call GitHub API (no DB lock held, avoiding long lock hold).
    let created = match github_client
        .create_issue(
            installation_id,
            &bp.repo_owner,
            &bp.repo_name,
            &title,
            &body,
        )
        .await
    {
        Ok(c) => c,
        Err(e) => {
            return handle_github_error(e, job.attempts);
        }
    };

    // Phase 3: Transaction with FOR UPDATE to atomically verify + persist.
    // This prevents duplicate DB writes if a concurrent handler also created an issue.
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return HandlerResult::Reschedule {
                reason: format!("DB error starting transaction: {e}"),
                backoff_secs: boardflow_jobs::backoff_secs(job.attempts),
            };
        }
    };

    // Re-check under lock: another handler may have completed while we called the API.
    let bp_locked = match board_project::find_by_id_with_repository_for_update(
        &mut *tx,
        board_project_id,
    )
    .await
    {
        Ok(Some(bp)) => bp,
        Ok(None) => {
            let _ = tx.rollback().await;
            return HandlerResult::Failed {
                reason: format!("board_project {board_project_id} not found"),
            };
        }
        Err(e) => {
            let _ = tx.rollback().await;
            return HandlerResult::Reschedule {
                reason: format!("DB error: {e}"),
                backoff_secs: boardflow_jobs::backoff_secs(job.attempts),
            };
        }
    };

    // Idempotency re-check under lock: if someone else created the issue, we're done.
    // Note: this means a GitHub issue may have been created that's now orphaned,
    // but that's acceptable — the important thing is DB consistency.
    if bp_locked.issue_number.is_some() {
        let _ = tx.rollback().await;
        return HandlerResult::Completed;
    }

    // Update board_project with issue info
    if let Err(e) = board_project::update_issue_info(
        &mut *tx,
        board_project_id,
        created.number as i32,
        &created.node_id,
        &created.html_url,
    )
    .await
    {
        let _ = tx.rollback().await;
        tracing::error!(error = %e, "Failed to update board_project issue info");
        return HandlerResult::Reschedule {
            reason: format!("DB error updating issue info: {e}"),
            backoff_secs: boardflow_jobs::backoff_secs(job.attempts),
        };
    }

    // Enqueue follow-up create_dashboard_comment job (must succeed for atomicity)
    if let Err(e) = github_job::enqueue(
        &mut *tx,
        uuid::Uuid::now_v7(),
        job.installation_id,
        job.repository_id,
        job.board_project_id,
        job.board_run_id,
        boardflow_domain::models::github_job::GithubJobType::CreateDashboardComment,
        &serde_json::json!({}),
    )
    .await
    {
        let _ = tx.rollback().await;
        tracing::error!(error = %e, "Failed to enqueue create_dashboard_comment");
        return HandlerResult::Reschedule {
            reason: format!("DB error enqueuing follow-up job: {e}"),
            backoff_secs: boardflow_jobs::backoff_secs(job.attempts),
        };
    }

    // Commit transaction — releases the FOR UPDATE lock
    if let Err(e) = tx.commit().await {
        tracing::error!(error = %e, "Failed to commit create_issue transaction");
        return HandlerResult::Reschedule {
            reason: format!("DB error committing: {e}"),
            backoff_secs: boardflow_jobs::backoff_secs(job.attempts),
        };
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use boardflow_domain::models::github_job::{GithubJob, GithubJobStatus, GithubJobType};
    use chrono::Utc;
    use uuid::Uuid;

    fn make_job(board_project_id: Option<Uuid>) -> GithubJob {
        GithubJob {
            id: Uuid::now_v7(),
            installation_id: 12345,
            repository_id: Uuid::now_v7(),
            board_project_id,
            board_run_id: Some(Uuid::now_v7()),
            r#type: GithubJobType::CreateIssue,
            payload_json: serde_json::json!({}),
            status: GithubJobStatus::Running,
            attempts: 1,
            run_after: Utc::now(),
            last_error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_handle_github_error_rate_limited_with_retry_after() {
        let err = GitHubClientError::RateLimited {
            retry_after_secs: Some(120),
        };
        let result = handle_github_error(err, 1);
        match result {
            HandlerResult::Reschedule {
                reason,
                backoff_secs,
            } => {
                assert!(reason.contains("Rate limited"));
                assert_eq!(backoff_secs, 120.0);
            }
            _ => panic!("Expected Reschedule"),
        }
    }

    #[test]
    fn test_handle_github_error_rate_limited_without_retry_after() {
        let err = GitHubClientError::RateLimited {
            retry_after_secs: None,
        };
        let result = handle_github_error(err, 2);
        match result {
            HandlerResult::Reschedule {
                reason,
                backoff_secs,
            } => {
                assert!(reason.contains("Rate limited"));
                // backoff = BASE_BACKOFF_SECS * 3^attempts * 2 = 10 * 9 * 2 = 180
                assert_eq!(backoff_secs, boardflow_jobs::backoff_secs(2) * 2.0);
            }
            _ => panic!("Expected Reschedule"),
        }
    }

    #[test]
    fn test_handle_github_error_auth() {
        let err = GitHubClientError::Auth("token expired".into());
        let result = handle_github_error(err, 1);
        match result {
            HandlerResult::Reschedule { reason, .. } => {
                assert!(reason.contains("Auth error"));
            }
            _ => panic!("Expected Reschedule"),
        }
    }

    #[test]
    fn test_handle_github_error_api() {
        let err = GitHubClientError::Api("server error".into());
        let result = handle_github_error(err, 3);
        match result {
            HandlerResult::Reschedule {
                reason,
                backoff_secs,
            } => {
                assert!(reason.contains("GitHub API error"));
                assert_eq!(backoff_secs, boardflow_jobs::backoff_secs(3));
            }
            _ => panic!("Expected Reschedule"),
        }
    }

    #[test]
    fn test_handle_github_error_not_found() {
        let err = GitHubClientError::NotFound("issue not found".into());
        let result = handle_github_error(err, 1);
        match result {
            HandlerResult::Reschedule { reason, .. } => {
                assert!(reason.contains("GitHub API error"));
            }
            _ => panic!("Expected Reschedule"),
        }
    }

    /// Test that missing board_project_id results in Failed.
    /// This test validates the early return without needing a DB pool.
    #[tokio::test]
    async fn test_handle_missing_board_project_id() {
        // We need a pool but it won't be used because the handler returns early.
        // Use connect_lazy with a dummy URL — no actual connection is established.
        let pool = sqlx::PgPool::connect_lazy("postgres://dummy:dummy@localhost/dummy")
            .expect("connect_lazy should not fail");

        let config = WorkerConfig {
            db: boardflow_config::DatabaseConfig {
                database_url: String::new(),
            },
            s3: boardflow_config::S3Config {
                endpoint: None,
                access_key: None,
                secret_key: None,
                staging_bucket: String::new(),
                final_bucket: String::new(),
            },
            poll_interval_secs: 2,
            timeout_sweep_interval_secs: 60,
            cache_cleanup_interval_secs: 3600,
            github_app_id: None,
            github_private_key_pem: None,
            app_domain: "https://test.example.com".into(),
        };

        // Minimal mock that should never be called
        struct NeverCalledClient;
        #[async_trait::async_trait]
        impl GitHubAppClient for NeverCalledClient {
            async fn get_installation_token(
                &self,
                _: u64,
            ) -> Result<secrecy::SecretString, GitHubClientError> {
                panic!("should not be called")
            }
            async fn create_issue(
                &self,
                _: u64,
                _: &str,
                _: &str,
                _: &str,
                _: &str,
            ) -> Result<boardflow_github::CreatedIssue, GitHubClientError> {
                panic!("should not be called")
            }
            async fn get_issue(
                &self,
                _: u64,
                _: &str,
                _: &str,
                _: u64,
            ) -> Result<boardflow_github::IssueInfo, GitHubClientError> {
                panic!("should not be called")
            }
            async fn create_comment(
                &self,
                _: u64,
                _: &str,
                _: &str,
                _: u64,
                _: &str,
            ) -> Result<boardflow_github::CreatedComment, GitHubClientError> {
                panic!("should not be called")
            }
            async fn update_comment(
                &self,
                _: u64,
                _: &str,
                _: &str,
                _: u64,
                _: &str,
            ) -> Result<(), GitHubClientError> {
                panic!("should not be called")
            }
        }

        let client = NeverCalledClient;
        let job = make_job(None); // No board_project_id

        let result = handle(&pool, &client, &config, &job).await;
        match result {
            HandlerResult::Failed { reason } => {
                assert_eq!(reason, "job missing board_project_id");
            }
            _ => panic!("Expected Failed, got {:?}", result_variant(&result)),
        }
    }

    fn result_variant(r: &HandlerResult) -> &'static str {
        match r {
            HandlerResult::Completed => "Completed",
            HandlerResult::Reschedule { .. } => "Reschedule",
            HandlerResult::Failed { .. } => "Failed",
        }
    }
}
