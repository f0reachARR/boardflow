use reqwest::StatusCode;

use super::types::{AccessError, AccessResult, GithubAccessChecker};

// ─── Production implementation ───────────────────────────────────────────────

/// Production implementation: calls GitHub API.
pub struct RealGithubAccessChecker {
    client: reqwest::Client,
}

impl Default for RealGithubAccessChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl RealGithubAccessChecker {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl GithubAccessChecker for RealGithubAccessChecker {
    async fn check_access(
        &self,
        github_access_token: &str,
        owner: &str,
        name: &str,
    ) -> AccessResult {
        let url = format!("https://api.github.com/repos/{}/{}", owner, name);

        let result = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", github_access_token))
            .header("User-Agent", "BoardFlow")
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await;

        match result {
            Ok(resp) => {
                let status = resp.status();
                match status {
                    StatusCode::OK => AccessResult::Allowed,
                    StatusCode::UNAUTHORIZED => AccessResult::Error(AccessError::TokenExpired),
                    StatusCode::TOO_MANY_REQUESTS => AccessResult::Error(AccessError::RateLimited),
                    StatusCode::NOT_FOUND => AccessResult::Denied,
                    StatusCode::FORBIDDEN => {
                        // GitHub uses 403 for rate limiting (primary/secondary) and org restrictions.
                        // Check rate limit headers to distinguish.
                        let is_rate_limited = resp
                            .headers()
                            .get("x-ratelimit-remaining")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u32>().ok())
                            .is_some_and(|remaining| remaining == 0)
                            || resp.headers().contains_key("retry-after");
                        if is_rate_limited {
                            AccessResult::Error(AccessError::RateLimited)
                        } else {
                            AccessResult::Error(AccessError::Upstream(
                                "forbidden: possible org restriction or API abuse".to_string(),
                            ))
                        }
                    }
                    _ => AccessResult::Error(AccessError::Upstream(format!(
                        "unexpected status: {status}"
                    ))),
                }
            }
            Err(e) => AccessResult::Error(AccessError::Upstream(e.to_string())),
        }
    }

    async fn list_accessible_repo_ids(
        &self,
        github_access_token: &str,
    ) -> Result<Option<Vec<i64>>, AccessError> {
        let mut ids = Vec::new();
        let mut page = 1u32;

        loop {
            let url = format!("https://api.github.com/user/repos?per_page=100&page={page}");

            let resp = self
                .client
                .get(&url)
                .header("Authorization", format!("Bearer {}", github_access_token))
                .header("User-Agent", "BoardFlow")
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send()
                .await
                .map_err(|e| AccessError::Upstream(e.to_string()))?;

            match resp.status() {
                StatusCode::UNAUTHORIZED => return Err(AccessError::TokenExpired),
                StatusCode::TOO_MANY_REQUESTS => return Err(AccessError::RateLimited),
                StatusCode::FORBIDDEN => {
                    let is_rate_limited = resp
                        .headers()
                        .get("x-ratelimit-remaining")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u32>().ok())
                        .is_some_and(|remaining| remaining == 0)
                        || resp.headers().contains_key("retry-after");
                    if is_rate_limited {
                        return Err(AccessError::RateLimited);
                    } else {
                        return Err(AccessError::Upstream("forbidden".to_string()));
                    }
                }
                StatusCode::OK => {}
                status => {
                    return Err(AccessError::Upstream(format!(
                        "unexpected status: {status}"
                    )));
                }
            }

            let repos: Vec<serde_json::Value> = resp
                .json()
                .await
                .map_err(|e| AccessError::Upstream(e.to_string()))?;

            if repos.is_empty() {
                break;
            }

            for repo in &repos {
                if let Some(id) = repo.get("id").and_then(|v| v.as_i64()) {
                    ids.push(id);
                }
            }

            if repos.len() < 100 {
                break;
            }
            page += 1;
        }

        Ok(Some(ids))
    }
}
