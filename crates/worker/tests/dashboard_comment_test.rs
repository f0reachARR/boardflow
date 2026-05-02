//! Integration tests for create_dashboard_comment and update_dashboard_comment handlers.
//!
//! Requires DATABASE_URL to be set to a PostgreSQL database with migrations applied.
//! Run with: `cargo test -p boardflow-worker --test dashboard_comment_test -- --ignored`

use boardflow_domain::models::github_job::{GithubJob, GithubJobStatus};
use boardflow_github::{
    CreatedComment, CreatedIssue, GitHubAppClient, GitHubClientError, IssueInfo, IssueState,
};
use chrono::Utc;
use secrecy::SecretString;
use sqlx::PgPool;
use uuid::Uuid;

/// Mock GitHub client with configurable results for dashboard comment tests.
struct MockGitHubClient {
    get_issue_result: tokio::sync::Mutex<Option<Result<IssueInfo, GitHubClientError>>>,
    create_comment_result: tokio::sync::Mutex<Option<Result<CreatedComment, GitHubClientError>>>,
    update_comment_result: tokio::sync::Mutex<Option<Result<(), GitHubClientError>>>,
}

impl MockGitHubClient {
    /// Default mock: get_issue returns Open, create_comment returns id=100, update_comment succeeds.
    fn default_success() -> Self {
        Self {
            get_issue_result: tokio::sync::Mutex::new(Some(Ok(IssueInfo {
                number: 1,
                node_id: "MDU6SXNzdWUx".into(),
                state: IssueState::Open,
                html_url: "https://github.com/test-owner/test-repo/issues/1".into(),
            }))),
            create_comment_result: tokio::sync::Mutex::new(Some(Ok(CreatedComment { id: 100 }))),
            update_comment_result: tokio::sync::Mutex::new(Some(Ok(()))),
        }
    }

    fn with_create_comment(mut self, result: Result<CreatedComment, GitHubClientError>) -> Self {
        self.create_comment_result = tokio::sync::Mutex::new(Some(result));
        self
    }

    fn with_update_comment(mut self, result: Result<(), GitHubClientError>) -> Self {
        self.update_comment_result = tokio::sync::Mutex::new(Some(result));
        self
    }
}

#[async_trait::async_trait]
impl GitHubAppClient for MockGitHubClient {
    async fn get_installation_token(&self, _: u64) -> Result<SecretString, GitHubClientError> {
        Ok(SecretString::from("mock-token".to_string()))
    }

    async fn create_issue(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<CreatedIssue, GitHubClientError> {
        panic!("create_issue should not be called in dashboard comment tests")
    }

    async fn get_issue(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: u64,
    ) -> Result<IssueInfo, GitHubClientError> {
        self.get_issue_result
            .lock()
            .await
            .take()
            .expect("get_issue called more than once")
    }

    async fn create_comment(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: u64,
        _: &str,
    ) -> Result<CreatedComment, GitHubClientError> {
        self.create_comment_result
            .lock()
            .await
            .take()
            .expect("create_comment called more than once")
    }

    async fn update_comment(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: u64,
        _: &str,
    ) -> Result<(), GitHubClientError> {
        self.update_comment_result
            .lock()
            .await
            .take()
            .expect("update_comment called more than once")
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
        timeout_sweep_interval_secs: 60,
        github_app_id: None,
        github_private_key_pem: None,
        app_base_url: "https://test.boardflow.example.com".into(),
    }
}

fn make_job(
    job_type: &str,
    board_project_id: Option<Uuid>,
    board_run_id: Option<Uuid>,
) -> GithubJob {
    GithubJob {
        id: Uuid::now_v7(),
        installation_id: 99999,
        repository_id: Uuid::now_v7(),
        board_project_id,
        board_run_id,
        r#type: job_type.into(),
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

/// Setup: repository + board_project + board_run. Returns (repo_id, bp_id, run_id, installation_id).
async fn setup_test_data(pool: &PgPool) -> (Uuid, Uuid, Uuid, i64) {
    let repo_id = Uuid::now_v7();
    let github_repository_id: i64 = rand::random::<i32>().unsigned_abs() as i64;
    let installation_id: i64 = 99999;

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

    let run_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO board_runs (id, board_project_id, commit_sha, branch, ref, github_run_id, github_run_attempt, tree_hash, status, erc_errors, erc_warnings, drc_errors, drc_warnings, review_status, diff_status, created_at, completed_at) \
         VALUES ($1, $2, 'abc1234', 'main', 'refs/heads/main', 1, 1, 'treehash123', 'completed', 0, 0, 0, 0, 'pending', 'pending', NOW(), NOW())"
    )
    .bind(run_id)
    .bind(bp_id)
    .execute(pool)
    .await
    .unwrap();

    (repo_id, bp_id, run_id, installation_id)
}

/// Setup with issue_number already set on board_project.
async fn setup_with_issue(pool: &PgPool) -> (Uuid, Uuid, Uuid, i64) {
    let (repo_id, bp_id, run_id, installation_id) = setup_test_data(pool).await;

    sqlx::query(
        "UPDATE board_projects SET issue_number = 1, issue_node_id = 'MDU6SXNzdWUx', issue_url = 'https://github.com/test-owner/test-repo/issues/1' WHERE id = $1"
    )
    .bind(bp_id)
    .execute(pool)
    .await
    .unwrap();

    (repo_id, bp_id, run_id, installation_id)
}

async fn cleanup_test_data(pool: &PgPool, repo_id: Uuid, bp_id: Uuid) {
    let _ = sqlx::query("DELETE FROM github_jobs WHERE board_project_id = $1")
        .bind(bp_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM board_runs WHERE board_project_id = $1")
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

// ─── create_dashboard_comment tests ─────────────────────────────────────────

#[tokio::test]
#[ignore]
async fn test_create_dashboard_comment_success() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id, run_id, installation_id) = setup_with_issue(&pool).await;

    let client = MockGitHubClient::default_success();
    let config = make_config();
    let mut job = make_job("create_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result = boardflow_worker::handlers::create_dashboard_comment::handle(
        &pool, &client, &config, &job,
    )
    .await;

    assert!(
        matches!(result, boardflow_worker::handlers::HandlerResult::Completed),
        "Expected Completed, got: {}",
        handler_result_debug(&result)
    );

    // Verify dashboard_comment_id was saved
    let row: (Option<i64>,) =
        sqlx::query_as("SELECT dashboard_comment_id FROM board_projects WHERE id = $1")
            .bind(bp_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, Some(100), "dashboard_comment_id should be saved");

    cleanup_test_data(&pool, repo_id, bp_id).await;
}

#[tokio::test]
#[ignore]
async fn test_create_dashboard_comment_idempotent() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id, run_id, installation_id) = setup_with_issue(&pool).await;

    // Pre-set dashboard_comment_id to simulate already-created comment
    sqlx::query("UPDATE board_projects SET dashboard_comment_id = 999 WHERE id = $1")
        .bind(bp_id)
        .execute(&pool)
        .await
        .unwrap();

    // Client that panics if API is called — idempotent path should skip API calls
    struct PanicClient;
    #[async_trait::async_trait]
    impl GitHubAppClient for PanicClient {
        async fn get_installation_token(&self, _: u64) -> Result<SecretString, GitHubClientError> {
            panic!("should not be called")
        }
        async fn create_issue(&self, _: u64, _: &str, _: &str, _: &str, _: &str) -> Result<CreatedIssue, GitHubClientError> {
            panic!("should not be called")
        }
        async fn get_issue(&self, _: u64, _: &str, _: &str, _: u64) -> Result<IssueInfo, GitHubClientError> {
            panic!("should not be called")
        }
        async fn create_comment(&self, _: u64, _: &str, _: &str, _: u64, _: &str) -> Result<CreatedComment, GitHubClientError> {
            panic!("should not be called")
        }
        async fn update_comment(&self, _: u64, _: &str, _: &str, _: u64, _: &str) -> Result<(), GitHubClientError> {
            panic!("should not be called")
        }
    }

    let config = make_config();
    let mut job = make_job("create_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result = boardflow_worker::handlers::create_dashboard_comment::handle(
        &pool, &PanicClient, &config, &job,
    )
    .await;

    assert!(matches!(
        result,
        boardflow_worker::handlers::HandlerResult::Completed
    ));

    cleanup_test_data(&pool, repo_id, bp_id).await;
}

#[tokio::test]
#[ignore]
async fn test_create_dashboard_comment_no_issue() {
    let Some(pool) = get_pool().await else { return };
    // setup_test_data does NOT set issue_number
    let (repo_id, bp_id, run_id, installation_id) = setup_test_data(&pool).await;

    let client = MockGitHubClient::default_success();
    let config = make_config();
    let mut job = make_job("create_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result = boardflow_worker::handlers::create_dashboard_comment::handle(
        &pool, &client, &config, &job,
    )
    .await;

    match result {
        boardflow_worker::handlers::HandlerResult::Reschedule {
            reason,
            backoff_secs,
        } => {
            assert!(
                reason.contains("issue not yet created"),
                "Expected issue-not-created reason, got: {reason}"
            );
            assert_eq!(backoff_secs, 5.0);
        }
        _ => panic!("Expected Reschedule, got: {}", handler_result_debug(&result)),
    }

    cleanup_test_data(&pool, repo_id, bp_id).await;
}

#[tokio::test]
#[ignore]
async fn test_create_dashboard_comment_missing_board_project_id() {
    let Some(pool) = get_pool().await else { return };

    let client = MockGitHubClient::default_success();
    let config = make_config();
    let job = make_job("create_dashboard_comment", None, Some(Uuid::now_v7()));

    let result = boardflow_worker::handlers::create_dashboard_comment::handle(
        &pool, &client, &config, &job,
    )
    .await;

    match result {
        boardflow_worker::handlers::HandlerResult::Failed { reason } => {
            assert!(reason.contains("board_project_id"));
        }
        _ => panic!("Expected Failed"),
    }
}

#[tokio::test]
#[ignore]
async fn test_create_dashboard_comment_missing_board_run_id() {
    let Some(pool) = get_pool().await else { return };

    let client = MockGitHubClient::default_success();
    let config = make_config();
    let job = make_job("create_dashboard_comment", Some(Uuid::now_v7()), None);

    let result = boardflow_worker::handlers::create_dashboard_comment::handle(
        &pool, &client, &config, &job,
    )
    .await;

    match result {
        boardflow_worker::handlers::HandlerResult::Failed { reason } => {
            assert!(reason.contains("board_run_id"));
        }
        _ => panic!("Expected Failed"),
    }
}

#[tokio::test]
#[ignore]
async fn test_create_dashboard_comment_project_not_found() {
    let Some(pool) = get_pool().await else { return };

    let non_existent_bp_id = Uuid::now_v7();
    let client = MockGitHubClient::default_success();
    let config = make_config();
    let job = make_job(
        "create_dashboard_comment",
        Some(non_existent_bp_id),
        Some(Uuid::now_v7()),
    );

    let result = boardflow_worker::handlers::create_dashboard_comment::handle(
        &pool, &client, &config, &job,
    )
    .await;

    match result {
        boardflow_worker::handlers::HandlerResult::Failed { reason } => {
            assert!(
                reason.contains("not found"),
                "Expected 'not found' in reason: {reason}"
            );
        }
        _ => panic!("Expected Failed"),
    }
}

// ─── update_dashboard_comment tests ─────────────────────────────────────────

#[tokio::test]
#[ignore]
async fn test_update_dashboard_comment_success() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id, run_id, installation_id) = setup_with_issue(&pool).await;

    // Set existing dashboard_comment_id
    sqlx::query("UPDATE board_projects SET dashboard_comment_id = 200 WHERE id = $1")
        .bind(bp_id)
        .execute(&pool)
        .await
        .unwrap();

    let client = MockGitHubClient::default_success();
    let config = make_config();
    let mut job = make_job("update_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result = boardflow_worker::handlers::update_dashboard_comment::handle(
        &pool, &client, &config, &job,
    )
    .await;

    assert!(
        matches!(result, boardflow_worker::handlers::HandlerResult::Completed),
        "Expected Completed, got: {}",
        handler_result_debug(&result)
    );

    cleanup_test_data(&pool, repo_id, bp_id).await;
}

#[tokio::test]
#[ignore]
async fn test_update_dashboard_comment_fallback_create() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id, run_id, installation_id) = setup_with_issue(&pool).await;

    // dashboard_comment_id is None → should fallback to create_comment
    let client = MockGitHubClient::default_success()
        .with_create_comment(Ok(CreatedComment { id: 300 }));
    let config = make_config();
    let mut job = make_job("update_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result = boardflow_worker::handlers::update_dashboard_comment::handle(
        &pool, &client, &config, &job,
    )
    .await;

    assert!(
        matches!(result, boardflow_worker::handlers::HandlerResult::Completed),
        "Expected Completed, got: {}",
        handler_result_debug(&result)
    );

    // Verify new dashboard_comment_id was saved
    let row: (Option<i64>,) =
        sqlx::query_as("SELECT dashboard_comment_id FROM board_projects WHERE id = $1")
            .bind(bp_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, Some(300), "fallback should save new comment id");

    cleanup_test_data(&pool, repo_id, bp_id).await;
}

#[tokio::test]
#[ignore]
async fn test_update_dashboard_comment_404_recreate() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id, run_id, installation_id) = setup_with_issue(&pool).await;

    // Set existing dashboard_comment_id that will 404
    sqlx::query("UPDATE board_projects SET dashboard_comment_id = 200 WHERE id = $1")
        .bind(bp_id)
        .execute(&pool)
        .await
        .unwrap();

    // update_comment returns NotFound, then create_comment succeeds with new id
    let client = MockGitHubClient::default_success()
        .with_update_comment(Err(GitHubClientError::NotFound(
            "comment not found".into(),
        )))
        .with_create_comment(Ok(CreatedComment { id: 400 }));
    let config = make_config();
    let mut job = make_job("update_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result = boardflow_worker::handlers::update_dashboard_comment::handle(
        &pool, &client, &config, &job,
    )
    .await;

    assert!(
        matches!(result, boardflow_worker::handlers::HandlerResult::Completed),
        "Expected Completed, got: {}",
        handler_result_debug(&result)
    );

    // Verify new dashboard_comment_id was saved (replaced old 200)
    let row: (Option<i64>,) =
        sqlx::query_as("SELECT dashboard_comment_id FROM board_projects WHERE id = $1")
            .bind(bp_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, Some(400), "should save recreated comment id");

    cleanup_test_data(&pool, repo_id, bp_id).await;
}

#[tokio::test]
#[ignore]
async fn test_update_dashboard_comment_no_issue() {
    let Some(pool) = get_pool().await else { return };
    // setup_test_data does NOT set issue_number
    let (repo_id, bp_id, run_id, installation_id) = setup_test_data(&pool).await;

    let client = MockGitHubClient::default_success();
    let config = make_config();
    let mut job = make_job("update_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result = boardflow_worker::handlers::update_dashboard_comment::handle(
        &pool, &client, &config, &job,
    )
    .await;

    match result {
        boardflow_worker::handlers::HandlerResult::Reschedule {
            reason,
            backoff_secs,
        } => {
            assert!(
                reason.contains("issue not yet created"),
                "Expected issue-not-created reason, got: {reason}"
            );
            assert_eq!(backoff_secs, 5.0);
        }
        _ => panic!(
            "Expected Reschedule, got: {}",
            handler_result_debug(&result)
        ),
    }

    cleanup_test_data(&pool, repo_id, bp_id).await;
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn handler_result_debug(result: &boardflow_worker::handlers::HandlerResult) -> String {
    match result {
        boardflow_worker::handlers::HandlerResult::Completed => "Completed".into(),
        boardflow_worker::handlers::HandlerResult::Reschedule { reason, backoff_secs } => {
            format!("Reschedule({reason}, {backoff_secs}s)")
        }
        boardflow_worker::handlers::HandlerResult::Failed { reason } => {
            format!("Failed({reason})")
        }
    }
}
