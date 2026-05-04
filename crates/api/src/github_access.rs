use std::sync::Arc;

use reqwest::StatusCode;

// ─── Result / Error types ────────────────────────────────────────────────────

/// Outcome of a single-repository access check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessResult {
    Allowed,
    Denied,
    Error(AccessError),
}

/// Errors originating from the GitHub API layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessError {
    /// GitHub returned 401 – token is invalid or expired.
    TokenExpired,
    /// GitHub returned 429 – rate limit exceeded.
    RateLimited,
    /// Any other GitHub API / network error.
    Upstream(String),
}

// ─── Trait ───────────────────────────────────────────────────────────────────

/// Trait for checking GitHub repository access.
/// Production implementation calls the GitHub API; test implementations can mock.
#[async_trait::async_trait]
pub trait GithubAccessChecker: Send + Sync {
    /// Check access to a specific repository.
    async fn check_access(
        &self,
        github_access_token: &str,
        owner: &str,
        name: &str,
    ) -> AccessResult;

    /// Get list of accessible repository github_ids for the user (for list filtering).
    /// Returns `Ok(None)` if no filtering is needed (e.g. test mode).
    /// Returns `Ok(Some(ids))` with the set of github_repository_ids the user can see.
    async fn list_accessible_repo_ids(
        &self,
        github_access_token: &str,
    ) -> Result<Option<Vec<i64>>, AccessError>;

    /// Invalidate cached repository data for a given user.
    /// Default implementation is a no-op (for non-caching implementations).
    async fn invalidate_repo_cache(&self, _user_id: uuid::Uuid) -> Result<(), AccessError> {
        Ok(())
    }
}

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

// ─── Mock: allow all ─────────────────────────────────────────────────────────

/// Mock implementation that always grants access (for tests).
pub struct AllowAllGithubAccessChecker;

#[async_trait::async_trait]
impl GithubAccessChecker for AllowAllGithubAccessChecker {
    async fn check_access(&self, _token: &str, _owner: &str, _name: &str) -> AccessResult {
        AccessResult::Allowed
    }

    async fn list_accessible_repo_ids(
        &self,
        _token: &str,
    ) -> Result<Option<Vec<i64>>, AccessError> {
        // No filtering needed in test mode
        Ok(None)
    }
}

// ─── Mock: deny all ──────────────────────────────────────────────────────────

/// Mock implementation that always denies access (for authorization tests).
pub struct DenyAllGithubAccessChecker;

#[async_trait::async_trait]
impl GithubAccessChecker for DenyAllGithubAccessChecker {
    async fn check_access(&self, _token: &str, _owner: &str, _name: &str) -> AccessResult {
        AccessResult::Denied
    }

    async fn list_accessible_repo_ids(
        &self,
        _token: &str,
    ) -> Result<Option<Vec<i64>>, AccessError> {
        // Empty list = nothing visible
        Ok(Some(vec![]))
    }
}

// ─── Mock: rate limited ──────────────────────────────────────────────────────

/// Mock implementation that always returns RateLimited (for error handling tests).
pub struct RateLimitedGithubAccessChecker;

#[async_trait::async_trait]
impl GithubAccessChecker for RateLimitedGithubAccessChecker {
    async fn check_access(&self, _token: &str, _owner: &str, _name: &str) -> AccessResult {
        AccessResult::Error(AccessError::RateLimited)
    }

    async fn list_accessible_repo_ids(
        &self,
        _token: &str,
    ) -> Result<Option<Vec<i64>>, AccessError> {
        Err(AccessError::RateLimited)
    }
}

// ─── Mock: upstream error ────────────────────────────────────────────────────

/// Mock implementation that always returns Upstream error (for error handling tests).
pub struct UpstreamErrorGithubAccessChecker;

#[async_trait::async_trait]
impl GithubAccessChecker for UpstreamErrorGithubAccessChecker {
    async fn check_access(&self, _token: &str, _owner: &str, _name: &str) -> AccessResult {
        AccessResult::Error(AccessError::Upstream(
            "simulated upstream failure".to_string(),
        ))
    }

    async fn list_accessible_repo_ids(
        &self,
        _token: &str,
    ) -> Result<Option<Vec<i64>>, AccessError> {
        Err(AccessError::Upstream(
            "simulated upstream failure".to_string(),
        ))
    }
}

// ─── Mock: token expired ─────────────────────────────────────────────────────

/// Mock implementation that always returns TokenExpired (for error handling tests).
pub struct TokenExpiredGithubAccessChecker;

#[async_trait::async_trait]
impl GithubAccessChecker for TokenExpiredGithubAccessChecker {
    async fn check_access(&self, _token: &str, _owner: &str, _name: &str) -> AccessResult {
        AccessResult::Error(AccessError::TokenExpired)
    }

    async fn list_accessible_repo_ids(
        &self,
        _token: &str,
    ) -> Result<Option<Vec<i64>>, AccessError> {
        Err(AccessError::TokenExpired)
    }
}

pub type DynGithubAccessChecker = Arc<dyn GithubAccessChecker>;

// ─── Cached implementation ───────────────────────────────────────────────────

const GITHUB_API_BASE_URL: &str = "https://api.github.com";

/// Caching decorator that stores `list_accessible_repo_ids` results in PostgreSQL.
/// Falls back to stale cache on rate-limit errors (stale-while-error).
pub struct CachedGithubAccessChecker {
    inner: Arc<dyn GithubAccessChecker>,
    pool: sqlx::PgPool,
    github_app_id: Option<u64>,
    github_api_base_url: String,
}

impl CachedGithubAccessChecker {
    pub fn new(pool: sqlx::PgPool, github_app_id: Option<u64>) -> Self {
        Self {
            inner: Arc::new(RealGithubAccessChecker::new()),
            pool,
            github_app_id,
            github_api_base_url: GITHUB_API_BASE_URL.to_string(),
        }
    }

    pub fn with_inner(
        inner: Arc<dyn GithubAccessChecker>,
        pool: sqlx::PgPool,
        github_app_id: Option<u64>,
    ) -> Self {
        Self {
            inner,
            pool,
            github_app_id,
            github_api_base_url: GITHUB_API_BASE_URL.to_string(),
        }
    }

    /// Create a checker with a custom GitHub API base URL (for testing).
    #[doc(hidden)]
    pub fn with_base_url(
        inner: Arc<dyn GithubAccessChecker>,
        pool: sqlx::PgPool,
        github_app_id: Option<u64>,
        base_url: String,
    ) -> Self {
        Self {
            inner,
            pool,
            github_app_id,
            github_api_base_url: base_url,
        }
    }

    /// Invalidate all cached data for a given user.
    pub async fn invalidate_cache(&self, user_id: uuid::Uuid) -> Result<(), sqlx::Error> {
        boardflow_db::queries::github_api_cache::delete_cache_by_user(&self.pool, user_id).await
    }
}

const CACHE_TYPE_REPO_IDS: &str = "accessible_repo_ids";
const CACHE_TTL_SECONDS: i64 = 600; // 10 minutes
const STALE_MAX_SECONDS: i64 = 3600; // 1 hour
const SYNC_CACHE_TYPE: &str = "installation_repos_sync";
const SYNC_TTL_SECONDS: i64 = 600; // 10 minutes

#[async_trait::async_trait]
impl GithubAccessChecker for CachedGithubAccessChecker {
    async fn check_access(
        &self,
        github_access_token: &str,
        owner: &str,
        name: &str,
    ) -> AccessResult {
        self.inner
            .check_access(github_access_token, owner, name)
            .await
    }

    async fn list_accessible_repo_ids(
        &self,
        github_access_token: &str,
    ) -> Result<Option<Vec<i64>>, AccessError> {
        // 1. Resolve user_id from token
        let user = boardflow_db::queries::user::find_by_github_access_token(
            &self.pool,
            github_access_token,
        )
        .await
        .map_err(|e| AccessError::Upstream(format!("db error: {e}")))?;

        let user = match user {
            Some(u) => u,
            None => {
                // Unknown token – pass through to inner without caching
                return self
                    .inner
                    .list_accessible_repo_ids(github_access_token)
                    .await;
            }
        };

        let user_id = user.id;

        // 2. Try valid cache
        if let Ok(Some(cached)) = boardflow_db::queries::github_api_cache::get_valid_cache(
            &self.pool,
            user_id,
            CACHE_TYPE_REPO_IDS,
        )
        .await
        {
            if let Ok(ids) = serde_json::from_value::<Vec<i64>>(cached) {
                // Even on cache hit, trigger fallback sync if repos are missing from DB
                self.maybe_sync_installation_repos(github_access_token, user_id, &ids)
                    .await;
                return Ok(Some(ids));
            }
        }

        // 3. Cache miss – call inner
        match self
            .inner
            .list_accessible_repo_ids(github_access_token)
            .await
        {
            Ok(result) => {
                // 4. On success, upsert cache
                if let Some(ref ids) = result {
                    let value = serde_json::to_value(ids)
                        .unwrap_or_else(|_| serde_json::Value::Array(vec![]));
                    let _ = boardflow_db::queries::github_api_cache::upsert_cache(
                        &self.pool,
                        user_id,
                        CACHE_TYPE_REPO_IDS,
                        &value,
                        CACHE_TTL_SECONDS,
                    )
                    .await;
                }
                // Fallback: sync installation repos if some are missing from DB
                if let Some(ref ids) = result {
                    self.maybe_sync_installation_repos(github_access_token, user_id, ids)
                        .await;
                }
                Ok(result)
            }
            Err(ref e @ AccessError::RateLimited) => {
                // 5. On RateLimited only – try stale cache
                let stale_duration = chrono::Duration::seconds(STALE_MAX_SECONDS);
                if let Ok(Some(stale)) = boardflow_db::queries::github_api_cache::get_stale_cache(
                    &self.pool,
                    user_id,
                    CACHE_TYPE_REPO_IDS,
                    stale_duration,
                )
                .await
                {
                    if let Ok(ids) = serde_json::from_value::<Vec<i64>>(stale) {
                        tracing::warn!(
                            user_id = %user_id,
                            error = %format!("{e:?}"),
                            "using stale cache for accessible_repo_ids due to rate limiting"
                        );
                        return Ok(Some(ids));
                    }
                }
                Err(AccessError::RateLimited)
            }
            Err(e) => {
                // 6. Non-rate-limit errors (TokenExpired, Upstream) – propagate directly
                Err(e)
            }
        }
    }

    async fn invalidate_repo_cache(&self, user_id: uuid::Uuid) -> Result<(), AccessError> {
        boardflow_db::queries::github_api_cache::delete_cache_by_user(&self.pool, user_id)
            .await
            .map_err(|e| AccessError::Upstream(format!("db error: {e}")))?;
        Ok(())
    }
}

// ─── Fallback sync: Installation Repositories API ────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct InstallationInfo {
    id: u64,
    app_id: u64,
    suspended_at: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct InstallationsResponse {
    installations: Vec<InstallationInfo>,
}

#[derive(Debug, serde::Deserialize)]
struct InstallationRepo {
    id: i64,
    full_name: String,
}

#[derive(Debug, serde::Deserialize)]
struct InstallationReposResponse {
    repositories: Vec<InstallationRepo>,
}

impl CachedGithubAccessChecker {
    async fn maybe_sync_installation_repos(
        &self,
        github_access_token: &str,
        user_id: uuid::Uuid,
        accessible_ids: &[i64],
    ) {
        let app_id = match self.github_app_id {
            Some(id) => id,
            None => return,
        };

        if accessible_ids.is_empty() {
            return;
        }

        // Check which ids are missing from DB
        let existing = match boardflow_db::queries::repository::find_existing_github_ids(
            &self.pool,
            accessible_ids,
        )
        .await
        {
            Ok(ids) => ids,
            Err(e) => {
                tracing::warn!(error = %e, "failed to check existing repos for sync");
                return;
            }
        };

        let existing_set: std::collections::HashSet<i64> = existing.into_iter().collect();
        let missing: Vec<i64> = accessible_ids
            .iter()
            .filter(|id| !existing_set.contains(id))
            .copied()
            .collect();

        if missing.is_empty() {
            return;
        }

        // Throttle check
        if let Ok(Some(_)) = boardflow_db::queries::github_api_cache::get_valid_cache(
            &self.pool,
            user_id,
            SYNC_CACHE_TYPE,
        )
        .await
        {
            return; // Already synced recently
        }

        tracing::info!(
            user_id = %user_id,
            missing_count = missing.len(),
            "triggering installation repos fallback sync"
        );

        // Fetch installations
        let client = reqwest::Client::new();
        let installations = match self
            .fetch_user_installations(&client, github_access_token)
            .await
        {
            Ok(installs) => installs,
            Err(e) => {
                tracing::warn!(error = %e, "fallback sync: failed to fetch installations");
                return;
            }
        };

        // Filter by app_id and not suspended
        let relevant_installations: Vec<&InstallationInfo> = installations
            .iter()
            .filter(|i| i.app_id == app_id && i.suspended_at.is_none())
            .collect();

        // For each installation, fetch repos and upsert
        for installation in relevant_installations {
            let repos = match self
                .fetch_installation_repos(&client, github_access_token, installation.id)
                .await
            {
                Ok(repos) => repos,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        installation_id = installation.id,
                        "fallback sync: failed to fetch repos for installation"
                    );
                    continue;
                }
            };

            for repo in &repos {
                if let Some((owner, name)) = repo.full_name.split_once('/') {
                    if let Err(e) = boardflow_db::queries::repository::upsert(
                        &self.pool,
                        repo.id,
                        owner,
                        name,
                        installation.id as i64,
                    )
                    .await
                    {
                        tracing::warn!(
                            error = %e,
                            repo_id = repo.id,
                            full_name = %repo.full_name,
                            "fallback sync: failed to upsert repository"
                        );
                    }
                }
            }
        }

        // Update throttle cache
        let _ = boardflow_db::queries::github_api_cache::upsert_cache(
            &self.pool,
            user_id,
            SYNC_CACHE_TYPE,
            &serde_json::json!({"synced": true}),
            SYNC_TTL_SECONDS,
        )
        .await;
    }

    async fn fetch_user_installations(
        &self,
        client: &reqwest::Client,
        github_access_token: &str,
    ) -> Result<Vec<InstallationInfo>, String> {
        let mut all_installations = Vec::new();
        let mut page = 1u32;

        loop {
            let url = format!(
                "{}/user/installations?per_page=100&page={page}",
                self.github_api_base_url
            );

            let resp = client
                .get(&url)
                .header("Authorization", format!("Bearer {}", github_access_token))
                .header("User-Agent", "BoardFlow")
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send()
                .await
                .map_err(|e| format!("request failed: {e}"))?;

            if !resp.status().is_success() {
                return Err(format!("status {}", resp.status()));
            }

            let data: InstallationsResponse = resp
                .json()
                .await
                .map_err(|e| format!("parse failed: {e}"))?;

            let count = data.installations.len();
            all_installations.extend(data.installations);

            if count < 100 {
                break;
            }
            page += 1;
        }

        Ok(all_installations)
    }

    async fn fetch_installation_repos(
        &self,
        client: &reqwest::Client,
        github_access_token: &str,
        installation_id: u64,
    ) -> Result<Vec<InstallationRepo>, String> {
        let mut all_repos = Vec::new();
        let mut page = 1u32;

        loop {
            let url = format!(
                "{}/user/installations/{installation_id}/repositories?per_page=100&page={page}",
                self.github_api_base_url
            );

            let resp = client
                .get(&url)
                .header("Authorization", format!("Bearer {}", github_access_token))
                .header("User-Agent", "BoardFlow")
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send()
                .await
                .map_err(|e| format!("request failed: {e}"))?;

            if !resp.status().is_success() {
                return Err(format!("status {}", resp.status()));
            }

            let data: InstallationReposResponse = resp
                .json()
                .await
                .map_err(|e| format!("parse failed: {e}"))?;

            let count = data.repositories.len();
            all_repos.extend(data.repositories);

            if count < 100 {
                break;
            }
            page += 1;
        }

        Ok(all_repos)
    }
}
