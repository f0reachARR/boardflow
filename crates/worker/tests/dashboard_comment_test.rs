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
use serial_test::serial;

/// Mock GitHub client with configurable results for dashboard comment tests.
struct MockGitHubClient {
    get_issue_result: tokio::sync::Mutex<Option<Result<IssueInfo, GitHubClientError>>>,
    create_comment_result: tokio::sync::Mutex<Option<Result<CreatedComment, GitHubClientError>>>,
    update_comment_result: tokio::sync::Mutex<Option<Result<(), GitHubClientError>>>,
    captured_comment_body: std::sync::Mutex<Option<String>>,
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
            captured_comment_body: std::sync::Mutex::new(None),
        }
    }

    fn with_get_issue(mut self, result: Result<IssueInfo, GitHubClientError>) -> Self {
        self.get_issue_result = tokio::sync::Mutex::new(Some(result));
        self
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
        match self.get_issue_result.lock().await.take() {
            Some(r) => r,
            None => panic!("get_issue called unexpectedly"),
        }
    }

    async fn create_comment(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: u64,
        body: &str,
    ) -> Result<CreatedComment, GitHubClientError> {
        *self.captured_comment_body.lock().unwrap() = Some(body.to_string());
        match self.create_comment_result.lock().await.take() {
            Some(r) => r,
            None => panic!("create_comment called unexpectedly"),
        }
    }

    async fn update_comment(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: u64,
        _: &str,
    ) -> Result<(), GitHubClientError> {
        match self.update_comment_result.lock().await.take() {
            Some(r) => r,
            None => panic!("update_comment called unexpectedly"),
        }
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

/// Setup with issue_number + two board_runs for tree_hash comparison.
/// Returns (repo_id, bp_id, prev_run_id, current_run_id, installation_id).
/// prev_run has tree_hash "treehash123", current_run has tree_hash "treehash456".
async fn setup_with_two_runs(pool: &PgPool) -> (Uuid, Uuid, Uuid, Uuid, i64) {
    let (repo_id, bp_id, run_id, installation_id) = setup_with_issue(pool).await;

    // run_id (from setup) has tree_hash "treehash123" — this is the previous run.
    // Create a second run with different tree_hash — this is the current/latest run.
    let run2_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO board_runs (id, board_project_id, commit_sha, branch, ref, github_run_id, github_run_attempt, tree_hash, status, erc_errors, erc_warnings, drc_errors, drc_warnings, review_status, diff_status, created_at, completed_at) \
         VALUES ($1, $2, 'def5678', 'main', 'refs/heads/main', 2, 1, 'treehash456', 'completed', 0, 0, 0, 0, 'pending', 'pending', NOW() + interval '1 second', NOW() + interval '1 second')"
    )
    .bind(run2_id)
    .bind(bp_id)
    .execute(pool)
    .await
    .unwrap();

    // Set latest_completed_run_id to the second run
    sqlx::query("UPDATE board_projects SET latest_completed_run_id = $2 WHERE id = $1")
        .bind(bp_id)
        .bind(run2_id)
        .execute(pool)
        .await
        .unwrap();

    (repo_id, bp_id, run_id, run2_id, installation_id)
}

async fn cleanup_test_data(pool: &PgPool, repo_id: Uuid, bp_id: Uuid) {
    let _ = sqlx::query("DELETE FROM github_jobs WHERE board_project_id = $1")
        .bind(bp_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM board_project_issue_history WHERE board_project_id = $1")
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
#[serial]
async fn test_create_dashboard_comment_success() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id, run_id, installation_id) = setup_with_issue(&pool).await;

    let client = MockGitHubClient::default_success();
    let config = make_config();
    let mut job = make_job("create_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result =
        boardflow_worker::handlers::create_dashboard_comment::handle(&pool, &client, &config, &job)
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
#[serial]
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
        async fn create_issue(
            &self,
            _: u64,
            _: &str,
            _: &str,
            _: &str,
            _: &str,
        ) -> Result<CreatedIssue, GitHubClientError> {
            panic!("should not be called")
        }
        async fn get_issue(
            &self,
            _: u64,
            _: &str,
            _: &str,
            _: u64,
        ) -> Result<IssueInfo, GitHubClientError> {
            panic!("should not be called")
        }
        async fn create_comment(
            &self,
            _: u64,
            _: &str,
            _: &str,
            _: u64,
            _: &str,
        ) -> Result<CreatedComment, GitHubClientError> {
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

    let config = make_config();
    let mut job = make_job("create_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result = boardflow_worker::handlers::create_dashboard_comment::handle(
        &pool,
        &PanicClient,
        &config,
        &job,
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
#[serial]
async fn test_create_dashboard_comment_no_issue() {
    let Some(pool) = get_pool().await else { return };
    // setup_test_data does NOT set issue_number
    let (repo_id, bp_id, run_id, installation_id) = setup_test_data(&pool).await;

    let client = MockGitHubClient::default_success();
    let config = make_config();
    let mut job = make_job("create_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result =
        boardflow_worker::handlers::create_dashboard_comment::handle(&pool, &client, &config, &job)
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

#[tokio::test]
#[ignore]
#[serial]
async fn test_create_dashboard_comment_missing_board_project_id() {
    let Some(pool) = get_pool().await else { return };

    let client = MockGitHubClient::default_success();
    let config = make_config();
    let job = make_job("create_dashboard_comment", None, Some(Uuid::now_v7()));

    let result =
        boardflow_worker::handlers::create_dashboard_comment::handle(&pool, &client, &config, &job)
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
#[serial]
async fn test_create_dashboard_comment_missing_board_run_id() {
    let Some(pool) = get_pool().await else { return };

    let client = MockGitHubClient::default_success();
    let config = make_config();
    let job = make_job("create_dashboard_comment", Some(Uuid::now_v7()), None);

    let result =
        boardflow_worker::handlers::create_dashboard_comment::handle(&pool, &client, &config, &job)
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
#[serial]
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

    let result =
        boardflow_worker::handlers::create_dashboard_comment::handle(&pool, &client, &config, &job)
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
#[serial]
async fn test_update_dashboard_comment_success() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id, run_id, installation_id) = setup_with_issue(&pool).await;

    // Set existing dashboard_comment_id
    sqlx::query("UPDATE board_projects SET dashboard_comment_id = 200 WHERE id = $1")
        .bind(bp_id)
        .execute(&pool)
        .await
        .unwrap();

    // create_comment is None → panics if called, ensuring only update path is exercised
    let client = MockGitHubClient {
        get_issue_result: tokio::sync::Mutex::new(Some(Ok(IssueInfo {
            number: 1,
            node_id: "MDU6SXNzdWUx".into(),
            state: IssueState::Open,
            html_url: "https://github.com/test-owner/test-repo/issues/1".into(),
        }))),
        create_comment_result: tokio::sync::Mutex::new(None),
        update_comment_result: tokio::sync::Mutex::new(Some(Ok(()))),
        captured_comment_body: std::sync::Mutex::new(None),
    };
    let config = make_config();
    let mut job = make_job("update_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result =
        boardflow_worker::handlers::update_dashboard_comment::handle(&pool, &client, &config, &job)
            .await;

    assert!(
        matches!(result, boardflow_worker::handlers::HandlerResult::Completed),
        "Expected Completed, got: {}",
        handler_result_debug(&result)
    );

    // Verify dashboard_comment_id remains 200 (not changed by update)
    let row: (Option<i64>,) =
        sqlx::query_as("SELECT dashboard_comment_id FROM board_projects WHERE id = $1")
            .bind(bp_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        row.0,
        Some(200),
        "dashboard_comment_id should remain unchanged after update"
    );

    cleanup_test_data(&pool, repo_id, bp_id).await;
}

#[tokio::test]
#[ignore]
#[serial]
async fn test_update_dashboard_comment_fallback_create() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id, run_id, installation_id) = setup_with_issue(&pool).await;

    // dashboard_comment_id is None → should fallback to create_comment
    let client =
        MockGitHubClient::default_success().with_create_comment(Ok(CreatedComment { id: 300 }));
    let config = make_config();
    let mut job = make_job("update_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result =
        boardflow_worker::handlers::update_dashboard_comment::handle(&pool, &client, &config, &job)
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
#[serial]
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
        .with_update_comment(Err(GitHubClientError::NotFound("comment not found".into())))
        .with_create_comment(Ok(CreatedComment { id: 400 }));
    let config = make_config();
    let mut job = make_job("update_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result =
        boardflow_worker::handlers::update_dashboard_comment::handle(&pool, &client, &config, &job)
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
#[serial]
async fn test_update_dashboard_comment_no_issue() {
    let Some(pool) = get_pool().await else { return };
    // setup_test_data does NOT set issue_number
    let (repo_id, bp_id, run_id, installation_id) = setup_test_data(&pool).await;

    let client = MockGitHubClient::default_success();
    let config = make_config();
    let mut job = make_job("update_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result =
        boardflow_worker::handlers::update_dashboard_comment::handle(&pool, &client, &config, &job)
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

// ─── create_dashboard_comment: closed Issue tests ───────────────────────────

#[tokio::test]
#[ignore]
#[serial]
async fn test_create_dashboard_comment_issue_closed_recreate_tree_hash_changed() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id, _prev_run_id, current_run_id, installation_id) =
        setup_with_two_runs(&pool).await;

    // get_issue returns Closed
    let client = MockGitHubClient::default_success().with_get_issue(Ok(IssueInfo {
        number: 1,
        node_id: "MDU6SXNzdWUx".into(),
        state: IssueState::Closed,
        html_url: "https://github.com/test-owner/test-repo/issues/1".into(),
    }));
    let config = make_config();
    let mut job = make_job(
        "create_dashboard_comment",
        Some(bp_id),
        Some(current_run_id),
    );
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result =
        boardflow_worker::handlers::create_dashboard_comment::handle(&pool, &client, &config, &job)
            .await;

    // Should Reschedule after clearing issue info and enqueuing create_issue
    match result {
        boardflow_worker::handlers::HandlerResult::Reschedule { reason, .. } => {
            assert!(
                reason.contains("closed"),
                "Expected 'closed' in reason, got: {reason}"
            );
        }
        _ => panic!(
            "Expected Reschedule, got: {}",
            handler_result_debug(&result)
        ),
    }

    // Verify issue info was cleared
    let row: (Option<i32>,) =
        sqlx::query_as("SELECT issue_number FROM board_projects WHERE id = $1")
            .bind(bp_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, None, "issue_number should be cleared");

    // Verify create_issue job was enqueued
    let job_row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM github_jobs WHERE board_project_id = $1 AND type = 'create_issue'",
    )
    .bind(bp_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(job_row.0 >= 1, "create_issue job should be enqueued");

    // Verify issue history was saved
    let history_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM board_project_issue_history WHERE board_project_id = $1",
    )
    .bind(bp_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(history_count.0 >= 1, "Issue history should be recorded");

    cleanup_test_data(&pool, repo_id, bp_id).await;
}

#[tokio::test]
#[ignore]
#[serial]
async fn test_create_dashboard_comment_issue_closed_tree_hash_unchanged() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id, _prev_run_id, current_run_id, installation_id) =
        setup_with_two_runs(&pool).await;

    // Make tree_hash the same on both runs so tree_hash_changed returns false
    sqlx::query("UPDATE board_runs SET tree_hash = 'same_hash' WHERE board_project_id = $1")
        .bind(bp_id)
        .execute(&pool)
        .await
        .unwrap();

    let client = MockGitHubClient::default_success().with_get_issue(Ok(IssueInfo {
        number: 1,
        node_id: "MDU6SXNzdWUx".into(),
        state: IssueState::Closed,
        html_url: "https://github.com/test-owner/test-repo/issues/1".into(),
    }));
    let config = make_config();
    let mut job = make_job(
        "create_dashboard_comment",
        Some(bp_id),
        Some(current_run_id),
    );
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result =
        boardflow_worker::handlers::create_dashboard_comment::handle(&pool, &client, &config, &job)
            .await;

    // tree_hash unchanged → Completed (no recreation needed)
    assert!(
        matches!(result, boardflow_worker::handlers::HandlerResult::Completed),
        "Expected Completed, got: {}",
        handler_result_debug(&result)
    );

    cleanup_test_data(&pool, repo_id, bp_id).await;
}

#[tokio::test]
#[ignore]
#[serial]
async fn test_create_dashboard_comment_issue_closed_no_recreate() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id, run_id, installation_id) = setup_with_issue(&pool).await;

    // Set recreate_issue_on_update = false
    sqlx::query("UPDATE board_projects SET recreate_issue_on_update = false WHERE id = $1")
        .bind(bp_id)
        .execute(&pool)
        .await
        .unwrap();

    let client = MockGitHubClient::default_success().with_get_issue(Ok(IssueInfo {
        number: 1,
        node_id: "MDU6SXNzdWUx".into(),
        state: IssueState::Closed,
        html_url: "https://github.com/test-owner/test-repo/issues/1".into(),
    }));
    let config = make_config();
    let mut job = make_job("create_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result =
        boardflow_worker::handlers::create_dashboard_comment::handle(&pool, &client, &config, &job)
            .await;

    // recreate_issue_on_update=false → Completed (stop updating)
    assert!(
        matches!(result, boardflow_worker::handlers::HandlerResult::Completed),
        "Expected Completed, got: {}",
        handler_result_debug(&result)
    );

    cleanup_test_data(&pool, repo_id, bp_id).await;
}

// ─── create_dashboard_comment: Issue 404 test ───────────────────────────────

#[tokio::test]
#[ignore]
#[serial]
async fn test_create_dashboard_comment_issue_404() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id, run_id, installation_id) = setup_with_issue(&pool).await;

    let client = MockGitHubClient::default_success()
        .with_get_issue(Err(GitHubClientError::NotFound("not found".into())));
    let config = make_config();
    let mut job = make_job("create_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result =
        boardflow_worker::handlers::create_dashboard_comment::handle(&pool, &client, &config, &job)
            .await;

    // Should Reschedule after clearing issue info and enqueuing create_issue
    match result {
        boardflow_worker::handlers::HandlerResult::Reschedule { reason, .. } => {
            assert!(
                reason.contains("404"),
                "Expected '404' in reason, got: {reason}"
            );
        }
        _ => panic!(
            "Expected Reschedule, got: {}",
            handler_result_debug(&result)
        ),
    }

    // Verify issue_number was cleared
    let row: (Option<i32>,) =
        sqlx::query_as("SELECT issue_number FROM board_projects WHERE id = $1")
            .bind(bp_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, None, "issue_number should be cleared after 404");

    // Verify create_issue job was enqueued
    let job_row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM github_jobs WHERE board_project_id = $1 AND type = 'create_issue'",
    )
    .bind(bp_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(job_row.0 >= 1, "create_issue job should be enqueued");

    // Verify issue history was saved
    let history_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM board_project_issue_history WHERE board_project_id = $1",
    )
    .bind(bp_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(history_count.0 >= 1, "Issue history should be recorded");

    cleanup_test_data(&pool, repo_id, bp_id).await;
}

// ─── create_dashboard_comment: debounce / stale job test ────────────────────

#[tokio::test]
#[ignore]
#[serial]
async fn test_create_dashboard_comment_uses_latest_completed_run() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id, old_run_id, _new_run_id, installation_id) =
        setup_with_two_runs(&pool).await;

    let client = MockGitHubClient::default_success();
    let config = make_config();
    // Job references the OLD run, but latest_completed_run_id points to the NEW run
    let mut job = make_job("create_dashboard_comment", Some(bp_id), Some(old_run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result =
        boardflow_worker::handlers::create_dashboard_comment::handle(&pool, &client, &config, &job)
            .await;

    assert!(
        matches!(result, boardflow_worker::handlers::HandlerResult::Completed),
        "Expected Completed, got: {}",
        handler_result_debug(&result)
    );

    // Verify dashboard_comment_id was saved (handler used latest run to create comment)
    let row: (Option<i64>,) =
        sqlx::query_as("SELECT dashboard_comment_id FROM board_projects WHERE id = $1")
            .bind(bp_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, Some(100), "dashboard_comment_id should be saved");

    // Verify the comment body uses the latest run (def5678) not the old run (abc1234)
    {
        let captured = client.captured_comment_body.lock().unwrap();
        let body = captured
            .as_ref()
            .expect("create_comment should have been called");
        assert!(
            body.contains("def5678"),
            "Comment body should contain latest run commit SHA 'def5678', got: {}",
            body
        );
        assert!(
            !body.contains("abc1234"),
            "Comment body should NOT contain old run commit SHA"
        );
    }

    cleanup_test_data(&pool, repo_id, bp_id).await;
}

// ─── update_dashboard_comment: closed / 404 tests ───────────────────────────

#[tokio::test]
#[ignore]
#[serial]
async fn test_update_dashboard_comment_issue_closed_no_recreate() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id, run_id, installation_id) = setup_with_issue(&pool).await;

    // Set recreate_issue_on_update = false and dashboard_comment_id
    sqlx::query("UPDATE board_projects SET recreate_issue_on_update = false, dashboard_comment_id = 200 WHERE id = $1")
        .bind(bp_id)
        .execute(&pool)
        .await
        .unwrap();

    let client = MockGitHubClient {
        get_issue_result: tokio::sync::Mutex::new(Some(Ok(IssueInfo {
            number: 1,
            node_id: "MDU6SXNzdWUx".into(),
            state: IssueState::Closed,
            html_url: "https://github.com/test-owner/test-repo/issues/1".into(),
        }))),
        create_comment_result: tokio::sync::Mutex::new(None),
        update_comment_result: tokio::sync::Mutex::new(None),
        captured_comment_body: std::sync::Mutex::new(None),
    };
    let config = make_config();
    let mut job = make_job("update_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result =
        boardflow_worker::handlers::update_dashboard_comment::handle(&pool, &client, &config, &job)
            .await;

    // recreate_issue_on_update=false → Completed
    assert!(
        matches!(result, boardflow_worker::handlers::HandlerResult::Completed),
        "Expected Completed, got: {}",
        handler_result_debug(&result)
    );

    cleanup_test_data(&pool, repo_id, bp_id).await;
}

#[tokio::test]
#[ignore]
#[serial]
async fn test_update_dashboard_comment_issue_404() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id, run_id, installation_id) = setup_with_issue(&pool).await;

    // Set dashboard_comment_id
    sqlx::query("UPDATE board_projects SET dashboard_comment_id = 200 WHERE id = $1")
        .bind(bp_id)
        .execute(&pool)
        .await
        .unwrap();

    let client = MockGitHubClient {
        get_issue_result: tokio::sync::Mutex::new(Some(Err(GitHubClientError::NotFound(
            "not found".into(),
        )))),
        create_comment_result: tokio::sync::Mutex::new(None),
        update_comment_result: tokio::sync::Mutex::new(None),
        captured_comment_body: std::sync::Mutex::new(None),
    };
    let config = make_config();
    let mut job = make_job("update_dashboard_comment", Some(bp_id), Some(run_id));
    job.installation_id = installation_id;
    job.repository_id = repo_id;

    let result =
        boardflow_worker::handlers::update_dashboard_comment::handle(&pool, &client, &config, &job)
            .await;

    // Issue 404 → Reschedule
    match result {
        boardflow_worker::handlers::HandlerResult::Reschedule { reason, .. } => {
            assert!(
                reason.contains("404"),
                "Expected '404' in reason, got: {reason}"
            );
        }
        _ => panic!(
            "Expected Reschedule, got: {}",
            handler_result_debug(&result)
        ),
    }

    // Verify issue_number was cleared
    let row: (Option<i32>,) =
        sqlx::query_as("SELECT issue_number FROM board_projects WHERE id = $1")
            .bind(bp_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, None, "issue_number should be cleared after 404");

    // Verify create_issue job was enqueued
    let job_row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM github_jobs WHERE board_project_id = $1 AND type = 'create_issue'",
    )
    .bind(bp_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(job_row.0 >= 1, "create_issue job should be enqueued");

    // Verify issue history was saved
    let history_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM board_project_issue_history WHERE board_project_id = $1",
    )
    .bind(bp_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        history_count.0 >= 1,
        "Issue history should be recorded on 404"
    );

    cleanup_test_data(&pool, repo_id, bp_id).await;
}

fn handler_result_debug(result: &boardflow_worker::handlers::HandlerResult) -> String {
    match result {
        boardflow_worker::handlers::HandlerResult::Completed => "Completed".into(),
        boardflow_worker::handlers::HandlerResult::Reschedule {
            reason,
            backoff_secs,
        } => {
            format!("Reschedule({reason}, {backoff_secs}s)")
        }
        boardflow_worker::handlers::HandlerResult::Failed { reason } => {
            format!("Failed({reason})")
        }
    }
}
