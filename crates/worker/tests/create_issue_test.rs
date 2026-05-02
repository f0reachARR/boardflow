//! Integration tests for create_issue handler.
//!
//! Requires DATABASE_URL to be set to a PostgreSQL database with migrations applied.
//! Run with: `cargo test -p boardflow-worker --test create_issue_test -- --ignored`
//! Tests are skipped (ignored) by default and must be explicitly opted in.

use boardflow_domain::models::github_job::{GithubJob, GithubJobStatus};
use boardflow_github::{CreatedComment, CreatedIssue, GitHubAppClient, GitHubClientError, IssueInfo, IssueState};
use chrono::Utc;
use secrecy::SecretString;
use sqlx::PgPool;
use uuid::Uuid;

/// Simple mock GitHub client for testing.
struct MockGitHubClient {
    create_issue_result: tokio::sync::Mutex<Option<Result<CreatedIssue, GitHubClientError>>>,
}

impl MockGitHubClient {
    fn success(number: u64, node_id: &str, html_url: &str) -> Self {
        Self {
            create_issue_result: tokio::sync::Mutex::new(Some(Ok(CreatedIssue {
                number,
                node_id: node_id.to_string(),
                html_url: html_url.to_string(),
            }))),
        }
    }

    fn failing(err: GitHubClientError) -> Self {
        Self {
            create_issue_result: tokio::sync::Mutex::new(Some(Err(err))),
        }
    }
}

#[async_trait::async_trait]
impl GitHubAppClient for MockGitHubClient {
    async fn get_installation_token(&self, _: u64) -> Result<SecretString, GitHubClientError> {
        Ok(SecretString::from("mock-token".to_string()))
    }

    async fn create_issue(
        &self,
        _installation_id: u64,
        _owner: &str,
        _repo: &str,
        _title: &str,
        _body: &str,
    ) -> Result<CreatedIssue, GitHubClientError> {
        self.create_issue_result
            .lock()
            .await
            .take()
            .expect("create_issue called more than once")
    }

    async fn get_issue(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: u64,
    ) -> Result<IssueInfo, GitHubClientError> {
        Ok(IssueInfo {
            number: 1,
            node_id: "MDU6SXNzdWUx".to_string(),
            state: IssueState::Open,
            html_url: "https://github.com/test/test/issues/1".to_string(),
        })
    }

    async fn create_comment(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: u64,
        _: &str,
    ) -> Result<CreatedComment, GitHubClientError> {
        Ok(CreatedComment {
            id: 1,
        })
    }

    async fn update_comment(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: u64,
        _: &str,
    ) -> Result<(), GitHubClientError> {
        Ok(())
    }
}

fn make_config() -> boardflow_worker::WorkerConfig {
    boardflow_worker::WorkerConfig {
        database_url: String::new(),
        staging_bucket: "test-staging".into(),
        artifacts_bucket: "test-artifacts".into(),
        s3_endpoint: None,
        s3_access_key: None,
        s3_secret_key: None,
        poll_interval_secs: 2,
        github_app_id: None,
        github_private_key_pem: None,
        app_base_url: "https://test.boardflow.example.com".into(),
    }
}

fn make_job(board_project_id: Option<Uuid>, board_run_id: Option<Uuid>) -> GithubJob {
    GithubJob {
        id: Uuid::now_v7(),
        installation_id: 12345,
        repository_id: Uuid::now_v7(),
        board_project_id,
        board_run_id,
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

async fn get_pool() -> Option<PgPool> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("Skipping test: DATABASE_URL not set");
            return None;
        }
    };
    Some(PgPool::connect(&database_url).await.unwrap())
}

/// Setup test data: create a repository and board_project, returning their IDs.
async fn setup_test_data(pool: &PgPool) -> (Uuid, Uuid, i64) {
    let repo_id = Uuid::now_v7();
    let github_repository_id: i64 = rand::random::<i32>().unsigned_abs() as i64;
    let installation_id: i64 = 99999;

    // Insert repository
    sqlx::query(
        "INSERT INTO repositories (id, github_repository_id, owner, name, installation_id, created_at, updated_at) \
         VALUES ($1, $2, 'test-owner', 'test-repo', $3, NOW(), NOW())"
    )
    .bind(repo_id)
    .bind(github_repository_id)
    .bind(installation_id)
    .execute(pool)
    .await
    .unwrap();

    // Insert board_project (no issue yet)
    let bp_id = Uuid::now_v7();
    let project_path = format!("hardware/test_{}/test.kicad_pro", bp_id);
    sqlx::query(
        "INSERT INTO board_projects (id, repository_id, project_path, project_dir, display_name, issue_sync_status, recreate_issue_on_update, created_at, updated_at) \
         VALUES ($1, $2, $3, 'hardware/test', 'test_board', 'pending', true, NOW(), NOW())"
    )
    .bind(bp_id)
    .bind(repo_id)
    .bind(&project_path)
    .execute(pool)
    .await
    .unwrap();

    (repo_id, bp_id, installation_id)
}

/// Cleanup test data.
async fn cleanup_test_data(pool: &PgPool, repo_id: Uuid, bp_id: Uuid) {
    let _ = sqlx::query("DELETE FROM github_jobs WHERE board_project_id = $1")
        .bind(bp_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM board_projects WHERE id = $1")
        .bind(bp_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM repositories WHERE id = $1")
        .bind(repo_id)
        .execute(pool)
        .await;
}

#[tokio::test]
#[ignore] // Requires DATABASE_URL; run with --ignored
async fn test_create_issue_success() {
    let Some(pool) = get_pool().await else { return };

    let (repo_id, bp_id, installation_id) = setup_test_data(&pool).await;

    let client = MockGitHubClient::success(
        42,
        "MDU6SXNzdWU0Mg==",
        "https://github.com/test-owner/test-repo/issues/42",
    );
    let config = make_config();
    let mut job = make_job(Some(bp_id), Some(Uuid::now_v7()));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result = boardflow_worker::handlers::create_issue::handle(&pool, &client, &config, &job).await;

    // Should complete successfully
    assert!(
        matches!(result, boardflow_worker::handlers::HandlerResult::Completed),
        "Expected Completed, got {:?}",
        match &result {
            boardflow_worker::handlers::HandlerResult::Completed => "Completed",
            boardflow_worker::handlers::HandlerResult::Reschedule { reason, .. } => reason.as_str(),
            boardflow_worker::handlers::HandlerResult::Failed { reason } => reason.as_str(),
        }
    );

    // Verify issue info was saved to board_project
    let row: (Option<i32>, Option<String>) = sqlx::query_as(
        "SELECT issue_number, issue_node_id FROM board_projects WHERE id = $1",
    )
    .bind(bp_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, Some(42));
    assert_eq!(row.1.as_deref(), Some("MDU6SXNzdWU0Mg=="));

    // Verify follow-up job was enqueued
    let follow_up_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM github_jobs WHERE board_project_id = $1 AND type = 'create_dashboard_comment'",
    )
    .bind(bp_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(follow_up_count.0 >= 1, "Expected create_dashboard_comment job to be enqueued");

    cleanup_test_data(&pool, repo_id, bp_id).await;
}

#[tokio::test]
#[ignore] // Requires DATABASE_URL; run with --ignored
async fn test_create_issue_idempotent() {
    let Some(pool) = get_pool().await else { return };

    let (repo_id, bp_id, installation_id) = setup_test_data(&pool).await;

    // Set issue_number to simulate issue already existing
    sqlx::query("UPDATE board_projects SET issue_number = 99, issue_node_id = 'existing', issue_url = 'https://example.com/99' WHERE id = $1")
        .bind(bp_id)
        .execute(&pool)
        .await
        .unwrap();

    // Client that would panic if called
    struct PanicClient;
    #[async_trait::async_trait]
    impl GitHubAppClient for PanicClient {
        async fn get_installation_token(&self, _: u64) -> Result<SecretString, GitHubClientError> { panic!() }
        async fn create_issue(&self, _: u64, _: &str, _: &str, _: &str, _: &str) -> Result<CreatedIssue, GitHubClientError> { panic!("should not be called for idempotent case") }
        async fn get_issue(&self, _: u64, _: &str, _: &str, _: u64) -> Result<IssueInfo, GitHubClientError> { panic!() }
        async fn create_comment(&self, _: u64, _: &str, _: &str, _: u64, _: &str) -> Result<CreatedComment, GitHubClientError> { panic!() }
        async fn update_comment(&self, _: u64, _: &str, _: &str, _: u64, _: &str) -> Result<(), GitHubClientError> { panic!() }
    }

    let config = make_config();
    let mut job = make_job(Some(bp_id), Some(Uuid::now_v7()));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result = boardflow_worker::handlers::create_issue::handle(&pool, &PanicClient, &config, &job).await;
    assert!(matches!(result, boardflow_worker::handlers::HandlerResult::Completed));

    cleanup_test_data(&pool, repo_id, bp_id).await;
}

#[tokio::test]
#[ignore] // Requires DATABASE_URL; run with --ignored
async fn test_create_issue_board_project_not_found() {
    let Some(pool) = get_pool().await else { return };

    let non_existent_bp_id = Uuid::now_v7();
    let client = MockGitHubClient::success(1, "x", "http://x");
    let config = make_config();
    let job = make_job(Some(non_existent_bp_id), Some(Uuid::now_v7()));

    let result = boardflow_worker::handlers::create_issue::handle(&pool, &client, &config, &job).await;
    match result {
        boardflow_worker::handlers::HandlerResult::Failed { reason } => {
            assert!(reason.contains("not found"), "Expected 'not found' in reason: {reason}");
        }
        _ => panic!("Expected Failed"),
    }
}

#[tokio::test]
#[ignore] // Requires DATABASE_URL; run with --ignored
async fn test_create_issue_github_rate_limited() {
    let Some(pool) = get_pool().await else { return };

    let (repo_id, bp_id, installation_id) = setup_test_data(&pool).await;

    let client = MockGitHubClient::failing(GitHubClientError::RateLimited {
        retry_after_secs: Some(60),
    });
    let config = make_config();
    let mut job = make_job(Some(bp_id), Some(Uuid::now_v7()));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result = boardflow_worker::handlers::create_issue::handle(&pool, &client, &config, &job).await;
    match result {
        boardflow_worker::handlers::HandlerResult::Reschedule { reason, backoff_secs } => {
            assert!(reason.contains("Rate limited"));
            assert_eq!(backoff_secs, 60.0);
        }
        _ => panic!("Expected Reschedule"),
    }

    cleanup_test_data(&pool, repo_id, bp_id).await;
}
