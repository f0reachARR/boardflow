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

#[cfg(test)]
mod tests {
    use super::*;
    use boardflow_domain::models::github_job::{GithubJob, GithubJobStatus};
    use chrono::Utc;
    use uuid::Uuid;

    fn make_job(board_project_id: Option<Uuid>) -> GithubJob {
        GithubJob {
            id: Uuid::now_v7(),
            installation_id: 12345,
            repository_id: Uuid::now_v7(),
            board_project_id,
            board_run_id: Some(Uuid::now_v7()),
            r#type: "create_issue".into(),
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
            HandlerResult::Reschedule { reason, backoff_secs } => {
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
            HandlerResult::Reschedule { reason, backoff_secs } => {
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
            HandlerResult::Reschedule { reason, backoff_secs } => {
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
            database_url: String::new(),
            staging_bucket: String::new(),
            artifacts_bucket: String::new(),
            s3_endpoint: None,
            s3_access_key: None,
            s3_secret_key: None,
            poll_interval_secs: 2,
            github_app_id: None,
            github_private_key_pem: None,
            app_base_url: "https://test.example.com".into(),
        };

        // Minimal mock that should never be called
        struct NeverCalledClient;
        #[async_trait::async_trait]
        impl GitHubAppClient for NeverCalledClient {
            async fn get_installation_token(&self, _: u64) -> Result<secrecy::SecretString, GitHubClientError> {
                panic!("should not be called")
            }
            async fn create_issue(&self, _: u64, _: &str, _: &str, _: &str, _: &str) -> Result<boardflow_github::CreatedIssue, GitHubClientError> {
                panic!("should not be called")
            }
            async fn get_issue(&self, _: u64, _: &str, _: &str, _: u64) -> Result<boardflow_github::IssueInfo, GitHubClientError> {
                panic!("should not be called")
            }
            async fn create_comment(&self, _: u64, _: &str, _: &str, _: u64, _: &str) -> Result<boardflow_github::CreatedComment, GitHubClientError> {
                panic!("should not be called")
            }
            async fn update_comment(&self, _: u64, _: &str, _: &str, _: u64, _: &str) -> Result<(), GitHubClientError> {
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
