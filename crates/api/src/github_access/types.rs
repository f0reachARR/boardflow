use std::sync::Arc;

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

pub type DynGithubAccessChecker = Arc<dyn GithubAccessChecker>;
