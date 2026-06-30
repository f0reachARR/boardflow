use std::sync::Arc;

use super::real::RealGithubAccessChecker;
use super::types::{AccessError, AccessResult, GithubAccessChecker};

// ─── Cached implementation ───────────────────────────────────────────────────

pub(super) const GITHUB_API_BASE_URL: &str = "https://api.github.com";

pub(super) const CACHE_TYPE_REPO_IDS: &str = "accessible_repo_ids";
// Issue #105: relaxed cache lifetimes. The repositories table is reconciled
// independently by webhook + worker sync, so we can serve cached user→repo
// mappings for longer without risking long-term staleness.
pub(super) const CACHE_TTL_SECONDS: i64 = 3600; // 1 hour (valid)
pub(super) const STALE_MAX_SECONDS: i64 = 86_400; // 24 hours (stale-while-error)
pub(super) const SYNC_CACHE_TYPE: &str = "installation_repos_sync";
pub(super) const SYNC_TTL_SECONDS: i64 = 1800; // 30 minutes

/// Caching decorator that stores `list_accessible_repo_ids` results in PostgreSQL.
/// Falls back to stale cache on rate-limit errors (stale-while-error).
pub struct CachedGithubAccessChecker {
    pub(super) inner: Arc<dyn GithubAccessChecker>,
    pub(super) pool: sqlx::PgPool,
    pub(super) github_app_id: Option<u64>,
    pub(super) github_api_base_url: String,
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
            Err(e) => {
                // 5. On any error – try stale cache so a transient GitHub failure
                // (rate limit, expired user token, upstream blip) does not break the
                // repository list. The repositories table itself is reconciled by
                // webhook + worker sync, so the cached id set is still meaningful.
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
                            "using stale cache for accessible_repo_ids after GitHub error"
                        );
                        return Ok(Some(ids));
                    }
                }
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
