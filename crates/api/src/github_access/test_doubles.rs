use super::types::{AccessError, AccessResult, GithubAccessChecker};

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
