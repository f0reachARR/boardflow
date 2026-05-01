use std::sync::Arc;

use reqwest::StatusCode;

/// Trait for checking GitHub repository access.
/// Production implementation calls the GitHub API; test implementations can mock.
#[async_trait::async_trait]
pub trait GithubAccessChecker: Send + Sync {
    async fn check_access(&self, github_access_token: &str, owner: &str, name: &str) -> bool;
}

/// Production implementation: calls GitHub API GET /repos/{owner}/{name}
pub struct RealGithubAccessChecker;

#[async_trait::async_trait]
impl GithubAccessChecker for RealGithubAccessChecker {
    async fn check_access(&self, github_access_token: &str, owner: &str, name: &str) -> bool {
        let client = reqwest::Client::new();
        let url = format!("https://api.github.com/repos/{}/{}", owner, name);

        let result = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", github_access_token))
            .header("User-Agent", "BoardFlow")
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await;

        match result {
            Ok(resp) => resp.status() == StatusCode::OK,
            Err(_) => false,
        }
    }
}

/// Mock implementation that always grants access (for tests)
pub struct AllowAllGithubAccessChecker;

#[async_trait::async_trait]
impl GithubAccessChecker for AllowAllGithubAccessChecker {
    async fn check_access(&self, _token: &str, _owner: &str, _name: &str) -> bool {
        true
    }
}

/// Mock implementation that always denies access (for authorization tests)
pub struct DenyAllGithubAccessChecker;

#[async_trait::async_trait]
impl GithubAccessChecker for DenyAllGithubAccessChecker {
    async fn check_access(&self, _token: &str, _owner: &str, _name: &str) -> bool {
        false
    }
}

pub type DynGithubAccessChecker = Arc<dyn GithubAccessChecker>;
