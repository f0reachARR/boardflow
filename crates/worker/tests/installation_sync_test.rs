//! Issue #105: integration tests for the worker installation reconciler.

use std::sync::Mutex;

use async_trait::async_trait;
use boardflow_db::queries::{github_installation_sync_state, repository as repo_queries};
use boardflow_github::{
    CreatedComment, CreatedIssue, GitHubAppClient, GitHubClientError, InstallationRepoInfo,
    IssueInfo,
};
use boardflow_worker::installation_sync;
use secrecy::SecretString;
use serial_test::serial;
use sqlx::PgPool;

mod common {
    pub fn rand_i64() -> i64 {
        rand::random::<u32>() as i64 + 10_000_000_000
    }
}

async fn setup_pool() -> Option<PgPool> {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("Skipping test: DATABASE_URL not set");
            return None;
        }
    };
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::migrate!("../db/migrations").run(&pool).await.unwrap();
    Some(pool)
}

fn make_config() -> boardflow_worker::WorkerConfig {
    boardflow_worker::WorkerConfig {
        db: boardflow_config::DatabaseConfig {
            database_url: String::new(),
        },
        s3: boardflow_config::S3Config {
            endpoint: None,
            access_key: None,
            secret_key: None,
            staging_bucket: "test".into(),
            final_bucket: "test".into(),
        },
        poll_interval_secs: 2,
        timeout_sweep_interval_secs: 60,
        cache_cleanup_interval_secs: 3600,
        installation_sync_interval_secs: 1800,
        installation_sync_stale_after_secs: 86_400,
        installation_sync_min_interval_secs: 3600,
        installation_sync_max_per_sweep: 50,
        github_app_id: None,
        github_private_key_pem: None,
        app_domain: "https://test.example.com".into(),
    }
}

#[derive(Default)]
struct MockApp {
    installations: Vec<u64>,
    repos: std::collections::HashMap<u64, Vec<InstallationRepoInfo>>,
    fail_for: Option<u64>,
    calls: Mutex<Vec<u64>>,
}

impl MockApp {
    fn new(installations: Vec<u64>) -> Self {
        Self {
            installations,
            ..Default::default()
        }
    }
    fn with_repos(mut self, installation_id: u64, repos: Vec<InstallationRepoInfo>) -> Self {
        self.repos.insert(installation_id, repos);
        self
    }
    fn with_fail(mut self, installation_id: u64) -> Self {
        self.fail_for = Some(installation_id);
        self
    }
}

#[async_trait]
impl GitHubAppClient for MockApp {
    async fn get_installation_token(&self, _: u64) -> Result<SecretString, GitHubClientError> {
        Ok(SecretString::from("mock".to_string()))
    }
    async fn create_issue(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<CreatedIssue, GitHubClientError> {
        unimplemented!()
    }
    async fn get_issue(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: u64,
    ) -> Result<IssueInfo, GitHubClientError> {
        unimplemented!()
    }
    async fn create_comment(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: u64,
        _: &str,
    ) -> Result<CreatedComment, GitHubClientError> {
        unimplemented!()
    }
    async fn update_comment(
        &self,
        _: u64,
        _: &str,
        _: &str,
        _: u64,
        _: &str,
    ) -> Result<(), GitHubClientError> {
        unimplemented!()
    }

    async fn list_installation_ids(&self) -> Result<Vec<u64>, GitHubClientError> {
        Ok(self.installations.clone())
    }

    async fn list_installation_repositories(
        &self,
        installation_id: u64,
    ) -> Result<Vec<InstallationRepoInfo>, GitHubClientError> {
        self.calls.lock().unwrap().push(installation_id);
        if self.fail_for == Some(installation_id) {
            return Err(GitHubClientError::Api("boom".into()));
        }
        Ok(self
            .repos
            .get(&installation_id)
            .cloned()
            .unwrap_or_default())
    }
}

#[tokio::test]
#[serial]
async fn syncs_unknown_installation_and_upserts_repos() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let installation_id = common::rand_i64() as u64;
    let repo_id = common::rand_i64();

    let mock = MockApp::new(vec![installation_id]).with_repos(
        installation_id,
        vec![InstallationRepoInfo {
            id: repo_id,
            owner: "octo".into(),
            name: "demo".into(),
        }],
    );
    let config = make_config();

    installation_sync::sync_stale_installations(&pool, &config, &mock).await;

    let repo = repo_queries::find_by_github_id(&pool, repo_id)
        .await
        .unwrap();
    assert!(repo.is_some(), "repo should be upserted");
    let repo = repo.unwrap();
    assert_eq!(repo.owner, "octo");
    assert_eq!(repo.name, "demo");
    assert_eq!(repo.installation_id, installation_id as i64);

    let state =
        github_installation_sync_state::find_by_installation_id(&pool, installation_id as i64)
            .await
            .unwrap()
            .expect("sync_state row should exist");
    assert_eq!(state.last_sync_status.as_deref(), Some("success"));
    assert!(state.last_sync_completed_at.is_some());
}

#[tokio::test]
#[serial]
async fn skips_installation_with_recent_webhook() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let installation_id = common::rand_i64() as u64;

    // Recent webhook seen → should be skipped (and never call list_installation_repositories)
    github_installation_sync_state::upsert_webhook_seen(&pool, installation_id as i64)
        .await
        .unwrap();

    let mock = MockApp::new(vec![installation_id]);
    let config = make_config();

    installation_sync::sync_stale_installations(&pool, &config, &mock).await;

    let calls = mock.calls.lock().unwrap();
    assert!(
        calls.is_empty(),
        "list_installation_repositories should not be called for fresh webhook installations"
    );
}

#[tokio::test]
#[serial]
async fn records_failure_when_repo_fetch_errors() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let installation_id = common::rand_i64() as u64;

    let mock = MockApp::new(vec![installation_id]).with_fail(installation_id);
    let config = make_config();

    installation_sync::sync_stale_installations(&pool, &config, &mock).await;

    let state =
        github_installation_sync_state::find_by_installation_id(&pool, installation_id as i64)
            .await
            .unwrap()
            .expect("sync_state row should exist");
    assert_eq!(state.last_sync_status.as_deref(), Some("failed"));
    assert!(state.last_error.is_some());
}

#[tokio::test]
#[serial]
async fn clears_installation_link_for_removed_repos() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let installation_id = common::rand_i64() as u64;
    let kept_repo = common::rand_i64();
    let removed_repo = common::rand_i64();

    // Seed two repos belonging to this installation
    repo_queries::upsert(&pool, kept_repo, "octo", "kept", installation_id as i64)
        .await
        .unwrap();
    repo_queries::upsert(
        &pool,
        removed_repo,
        "octo",
        "removed",
        installation_id as i64,
    )
    .await
    .unwrap();

    let mock = MockApp::new(vec![installation_id]).with_repos(
        installation_id,
        vec![InstallationRepoInfo {
            id: kept_repo,
            owner: "octo".into(),
            name: "kept".into(),
        }],
    );
    let config = make_config();

    installation_sync::sync_stale_installations(&pool, &config, &mock).await;

    let kept = repo_queries::find_by_github_id(&pool, kept_repo)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(kept.installation_id, installation_id as i64);

    let removed = repo_queries::find_by_github_id(&pool, removed_repo)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        removed.installation_id, 0,
        "removed repo should have its installation linkage cleared"
    );
}
