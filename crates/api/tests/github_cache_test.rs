use serial_test::serial;

use boardflow_api::github_access::{
    AccessError, AllowAllGithubAccessChecker, CachedGithubAccessChecker, GithubAccessChecker,
    RateLimitedGithubAccessChecker,
};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod common;

use common::unique_i64 as rand_i64;

async fn setup_pool() -> Option<PgPool> {
    unsafe { std::env::set_var("BOARDFLOW_ARTIFACT_SECRET", "test-secret-for-tests") };
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

async fn create_test_user_with_token(pool: &PgPool, token: &str) -> Uuid {
    // Clean up any leftover users with this token from previous test runs
    sqlx::query("DELETE FROM users WHERE github_access_token = $1")
        .bind(token)
        .execute(pool)
        .await
        .unwrap();

    let id = Uuid::now_v7();
    let github_user_id = rand_i64();
    sqlx::query(
        "INSERT INTO users (id, github_user_id, github_login, github_avatar_url, github_access_token, created_at, updated_at) \
         VALUES ($1, $2, 'cacheuser', 'https://avatar.example.com/test', $3, NOW(), NOW())",
    )
    .bind(id)
    .bind(github_user_id)
    .bind(token)
    .execute(pool)
    .await
    .unwrap();
    id
}

// ─── DB query tests ──────────────────────────────────────────────────────────

/// Test: upsert_cache inserts a new cache entry and get_valid_cache retrieves it
#[tokio::test]
#[serial]
async fn test_upsert_and_get_valid_cache() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let user_id = create_test_user_with_token(&pool, "gho_cache_test_1").await;

    let value = serde_json::json!([1, 2, 3]);
    boardflow_db::queries::github_api_cache::upsert_cache(
        &pool,
        user_id,
        "test_type",
        &value,
        600, // 10 min TTL
    )
    .await
    .unwrap();

    let cached =
        boardflow_db::queries::github_api_cache::get_valid_cache(&pool, user_id, "test_type")
            .await
            .unwrap();
    assert_eq!(cached, Some(value));
}

/// Test: get_valid_cache returns None for expired cache
#[tokio::test]
#[serial]
async fn test_get_valid_cache_returns_none_when_expired() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let user_id = create_test_user_with_token(&pool, "gho_cache_test_2").await;

    // Insert with negative TTL (already expired)
    sqlx::query(
        "INSERT INTO github_api_cache (user_id, cache_type, value_json, expires_at, created_at, updated_at) \
         VALUES ($1, 'expired_type', '[4,5,6]'::jsonb, NOW() - INTERVAL '1 minute', NOW(), NOW())",
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    let cached =
        boardflow_db::queries::github_api_cache::get_valid_cache(&pool, user_id, "expired_type")
            .await
            .unwrap();
    assert_eq!(cached, None);
}

/// Test: get_stale_cache returns recently-expired cache within max_stale_duration
#[tokio::test]
#[serial]
async fn test_get_stale_cache_returns_recently_expired() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let user_id = create_test_user_with_token(&pool, "gho_cache_test_3").await;

    // Insert expired 5 minutes ago
    sqlx::query(
        "INSERT INTO github_api_cache (user_id, cache_type, value_json, expires_at, created_at, updated_at) \
         VALUES ($1, 'stale_type', '[7,8,9]'::jsonb, NOW() - INTERVAL '5 minutes', NOW(), NOW()) \
         ON CONFLICT (user_id, cache_type) DO UPDATE SET value_json = EXCLUDED.value_json, expires_at = EXCLUDED.expires_at",
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    // Stale window of 1 hour should include this entry
    let stale = boardflow_db::queries::github_api_cache::get_stale_cache(
        &pool,
        user_id,
        "stale_type",
        chrono::Duration::hours(1),
    )
    .await
    .unwrap();
    assert_eq!(stale, Some(serde_json::json!([7, 8, 9])));
}

/// Test: get_stale_cache returns None for cache expired beyond max_stale_duration
#[tokio::test]
#[serial]
async fn test_get_stale_cache_returns_none_for_very_old_cache() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let user_id = create_test_user_with_token(&pool, "gho_cache_test_4").await;

    // Insert expired 2 hours ago
    sqlx::query(
        "INSERT INTO github_api_cache (user_id, cache_type, value_json, expires_at, created_at, updated_at) \
         VALUES ($1, 'old_type', '[10]'::jsonb, NOW() - INTERVAL '2 hours', NOW(), NOW()) \
         ON CONFLICT (user_id, cache_type) DO UPDATE SET value_json = EXCLUDED.value_json, expires_at = EXCLUDED.expires_at",
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    // Stale window of 1 hour should NOT include this
    let stale = boardflow_db::queries::github_api_cache::get_stale_cache(
        &pool,
        user_id,
        "old_type",
        chrono::Duration::hours(1),
    )
    .await
    .unwrap();
    assert_eq!(stale, None);
}

/// Test: upsert_cache updates existing entry (ON CONFLICT DO UPDATE)
#[tokio::test]
#[serial]
async fn test_upsert_cache_updates_existing() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let user_id = create_test_user_with_token(&pool, "gho_cache_test_5").await;

    let value1 = serde_json::json!([100, 200]);
    boardflow_db::queries::github_api_cache::upsert_cache(
        &pool,
        user_id,
        "upsert_type",
        &value1,
        600,
    )
    .await
    .unwrap();

    let value2 = serde_json::json!([300, 400, 500]);
    boardflow_db::queries::github_api_cache::upsert_cache(
        &pool,
        user_id,
        "upsert_type",
        &value2,
        600,
    )
    .await
    .unwrap();

    let cached =
        boardflow_db::queries::github_api_cache::get_valid_cache(&pool, user_id, "upsert_type")
            .await
            .unwrap();
    assert_eq!(cached, Some(value2));
}

/// Test: delete_cache removes specific cache entry
#[tokio::test]
#[serial]
async fn test_delete_cache_removes_specific_entry() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let user_id = create_test_user_with_token(&pool, "gho_cache_test_6").await;

    let value = serde_json::json!([1]);
    boardflow_db::queries::github_api_cache::upsert_cache(
        &pool,
        user_id,
        "delete_type",
        &value,
        600,
    )
    .await
    .unwrap();

    boardflow_db::queries::github_api_cache::delete_cache(&pool, user_id, "delete_type")
        .await
        .unwrap();

    let cached =
        boardflow_db::queries::github_api_cache::get_valid_cache(&pool, user_id, "delete_type")
            .await
            .unwrap();
    assert_eq!(cached, None);
}

/// Test: delete_cache_by_user removes all cache entries for a user
#[tokio::test]
#[serial]
async fn test_delete_cache_by_user_removes_all() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let user_id = create_test_user_with_token(&pool, "gho_cache_test_7").await;

    let value = serde_json::json!([1]);
    boardflow_db::queries::github_api_cache::upsert_cache(&pool, user_id, "type_a", &value, 600)
        .await
        .unwrap();
    boardflow_db::queries::github_api_cache::upsert_cache(&pool, user_id, "type_b", &value, 600)
        .await
        .unwrap();

    boardflow_db::queries::github_api_cache::delete_cache_by_user(&pool, user_id)
        .await
        .unwrap();

    let a = boardflow_db::queries::github_api_cache::get_valid_cache(&pool, user_id, "type_a")
        .await
        .unwrap();
    let b = boardflow_db::queries::github_api_cache::get_valid_cache(&pool, user_id, "type_b")
        .await
        .unwrap();
    assert_eq!(a, None);
    assert_eq!(b, None);
}

/// Test: cleanup_expired_cache removes old expired entries
#[tokio::test]
#[serial]
async fn test_cleanup_expired_cache() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let user_id = create_test_user_with_token(&pool, "gho_cache_test_8").await;

    // Issue #105: cleanup preserves entries up to 24h past expiration so they
    // remain available as stale-while-error fallbacks. Insert expired >24h ago.
    sqlx::query(
        "INSERT INTO github_api_cache (user_id, cache_type, value_json, expires_at, created_at, updated_at) \
         VALUES ($1, 'cleanup_type', '[1]'::jsonb, NOW() - INTERVAL '36 hours', NOW(), NOW()) \
         ON CONFLICT (user_id, cache_type) DO UPDATE SET value_json = EXCLUDED.value_json, expires_at = EXCLUDED.expires_at",
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    let deleted = boardflow_db::queries::github_api_cache::cleanup_expired_cache(&pool)
        .await
        .unwrap();
    assert!(deleted >= 1);
}

#[tokio::test]
#[serial]
async fn test_cleanup_preserves_recent_stale_cache() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let user_id = create_test_user_with_token(&pool, "gho_cache_cleanup_preserve").await;

    // Expired 2 hours ago — within the 24h stale window, must survive cleanup.
    sqlx::query(
        "INSERT INTO github_api_cache (user_id, cache_type, value_json, expires_at, created_at, updated_at) \
         VALUES ($1, 'preserve_type', '[2]'::jsonb, NOW() - INTERVAL '2 hours', NOW(), NOW()) \
         ON CONFLICT (user_id, cache_type) DO UPDATE SET value_json = EXCLUDED.value_json, expires_at = EXCLUDED.expires_at",
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    boardflow_db::queries::github_api_cache::cleanup_expired_cache(&pool)
        .await
        .unwrap();

    let stale = boardflow_db::queries::github_api_cache::get_stale_cache(
        &pool,
        user_id,
        "preserve_type",
        chrono::Duration::hours(24),
    )
    .await
    .unwrap();
    assert_eq!(stale, Some(serde_json::json!([2])));
}

// ─── CachedGithubAccessChecker integration tests ─────────────────────────────

/// Test: CachedGithubAccessChecker.invalidate_cache removes all user's cache
#[tokio::test]
#[serial]
async fn test_cached_checker_invalidate_cache() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let token = "gho_cached_checker_test_1";
    let user_id = create_test_user_with_token(&pool, token).await;

    // Seed cache directly
    let value = serde_json::json!([111, 222]);
    boardflow_db::queries::github_api_cache::upsert_cache(
        &pool,
        user_id,
        "accessible_repo_ids",
        &value,
        600,
    )
    .await
    .unwrap();

    let checker = CachedGithubAccessChecker::new(pool.clone(), None);
    checker.invalidate_cache(user_id).await.unwrap();

    let cached = boardflow_db::queries::github_api_cache::get_valid_cache(
        &pool,
        user_id,
        "accessible_repo_ids",
    )
    .await
    .unwrap();
    assert_eq!(cached, None);
}

/// Test: CachedGithubAccessChecker returns cached data on second call (cache hit)
#[tokio::test]
#[serial]
async fn test_cached_checker_returns_cached_repo_ids() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let token = "gho_cached_checker_test_2";
    let user_id = create_test_user_with_token(&pool, token).await;

    // Pre-seed cache with known IDs
    let value = serde_json::json!([1001, 1002, 1003]);
    boardflow_db::queries::github_api_cache::upsert_cache(
        &pool,
        user_id,
        "accessible_repo_ids",
        &value,
        600,
    )
    .await
    .unwrap();

    let inner: Arc<dyn GithubAccessChecker> = Arc::new(AllowAllGithubAccessChecker);
    let checker = CachedGithubAccessChecker::with_inner(inner, pool.clone(), None);
    let result = checker.list_accessible_repo_ids(token).await.unwrap();
    assert_eq!(result, Some(vec![1001, 1002, 1003]));
}

/// Test: CachedGithubAccessChecker for unknown token passes through without caching
/// (token not in users table → delegates to inner directly)
#[tokio::test]
#[serial]
async fn test_cached_checker_unknown_token_passes_through() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    // This token doesn't exist in users table
    // The inner (RealGithubAccessChecker) will get a 401 from GitHub
    let checker = CachedGithubAccessChecker::new(pool.clone(), None);
    let result = checker
        .list_accessible_repo_ids("gho_definitely_not_in_db")
        .await;
    // Should error (TokenExpired or Upstream) since it's not a real token
    assert!(result.is_err());
}

/// Test: CachedGithubAccessChecker uses stale cache on rate-limit error
/// We simulate this by seeding an expired cache entry and using a token
/// that will trigger a rate-limit from the inner checker
#[tokio::test]
#[serial]
async fn test_cached_checker_stale_fallback_on_rate_limit() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let token = "gho_cached_checker_test_stale";
    let user_id = create_test_user_with_token(&pool, token).await;

    // Seed an expired (but recent) cache entry - expired 5 minutes ago
    sqlx::query(
        "INSERT INTO github_api_cache (user_id, cache_type, value_json, expires_at, created_at, updated_at) \
         VALUES ($1, 'accessible_repo_ids', '[9001, 9002]'::jsonb, NOW() - INTERVAL '5 minutes', NOW(), NOW()) \
         ON CONFLICT (user_id, cache_type) DO UPDATE SET value_json = EXCLUDED.value_json, expires_at = EXCLUDED.expires_at",
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    // The CachedGithubAccessChecker uses RealGithubAccessChecker internally.
    // With an invalid token, GitHub will return 401 (TokenExpired), not 429.
    // For a true stale-while-error test we'd need to mock the inner checker.
    // Here we verify the cache miss path correctly attempts the stale lookup by
    // checking the DB state is correct - see the unit test below for full mock.
    let cached = boardflow_db::queries::github_api_cache::get_stale_cache(
        &pool,
        user_id,
        "accessible_repo_ids",
        chrono::Duration::hours(1),
    )
    .await
    .unwrap();
    assert_eq!(cached, Some(serde_json::json!([9001, 9002])));
}

// ─── Mock-inner tests (stale fallback & error propagation) ───────────────────

/// Test: RateLimited from inner triggers stale cache fallback
#[tokio::test]
#[serial]
async fn test_cached_checker_stale_fallback_with_mock_inner_rate_limited() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let token = "gho_mock_rate_limit_test";
    let user_id = create_test_user_with_token(&pool, token).await;

    // Seed an expired (but recent) cache entry - expired 5 minutes ago
    sqlx::query(
        "INSERT INTO github_api_cache (user_id, cache_type, value_json, expires_at, created_at, updated_at) \
         VALUES ($1, 'accessible_repo_ids', '[5001, 5002, 5003]'::jsonb, NOW() - INTERVAL '5 minutes', NOW(), NOW()) \
         ON CONFLICT (user_id, cache_type) DO UPDATE SET value_json = EXCLUDED.value_json, expires_at = EXCLUDED.expires_at",
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    // Use RateLimitedGithubAccessChecker as inner → always returns RateLimited
    let inner: Arc<dyn GithubAccessChecker> = Arc::new(RateLimitedGithubAccessChecker);
    let checker = CachedGithubAccessChecker::with_inner(inner, pool.clone(), None);

    let result = checker.list_accessible_repo_ids(token).await;
    // Should return stale cache instead of error
    assert_eq!(result, Ok(Some(vec![5001, 5002, 5003])));
}

/// Issue #105: TokenExpired from inner falls back to stale cache so a
/// transient GitHub token failure does not break the repository list.
#[tokio::test]
#[serial]
async fn test_cached_checker_token_expired_stale_fallback() {
    use boardflow_api::github_access::TokenExpiredGithubAccessChecker;

    let Some(pool) = setup_pool().await else {
        return;
    };
    let token = "gho_mock_token_expired_test";
    let user_id = create_test_user_with_token(&pool, token).await;

    // Seed an expired (but recent) cache entry
    sqlx::query(
        "INSERT INTO github_api_cache (user_id, cache_type, value_json, expires_at, created_at, updated_at) \
         VALUES ($1, 'accessible_repo_ids', '[6001, 6002]'::jsonb, NOW() - INTERVAL '5 minutes', NOW(), NOW()) \
         ON CONFLICT (user_id, cache_type) DO UPDATE SET value_json = EXCLUDED.value_json, expires_at = EXCLUDED.expires_at",
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    // Use TokenExpiredGithubAccessChecker as inner → always returns TokenExpired
    let inner: Arc<dyn GithubAccessChecker> = Arc::new(TokenExpiredGithubAccessChecker);
    let checker = CachedGithubAccessChecker::with_inner(inner, pool.clone(), None);

    let result = checker.list_accessible_repo_ids(token).await;
    assert_eq!(result, Ok(Some(vec![6001, 6002])));
}

/// Issue #105: Upstream error from inner falls back to stale cache.
#[tokio::test]
#[serial]
async fn test_cached_checker_upstream_error_stale_fallback() {
    use boardflow_api::github_access::UpstreamErrorGithubAccessChecker;

    let Some(pool) = setup_pool().await else {
        return;
    };
    let token = "gho_mock_upstream_error_test";
    let user_id = create_test_user_with_token(&pool, token).await;

    // Seed an expired (but recent) cache entry
    sqlx::query(
        "INSERT INTO github_api_cache (user_id, cache_type, value_json, expires_at, created_at, updated_at) \
         VALUES ($1, 'accessible_repo_ids', '[7001, 7002]'::jsonb, NOW() - INTERVAL '5 minutes', NOW(), NOW()) \
         ON CONFLICT (user_id, cache_type) DO UPDATE SET value_json = EXCLUDED.value_json, expires_at = EXCLUDED.expires_at",
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    // Use UpstreamErrorGithubAccessChecker as inner
    let inner: Arc<dyn GithubAccessChecker> = Arc::new(UpstreamErrorGithubAccessChecker);
    let checker = CachedGithubAccessChecker::with_inner(inner, pool.clone(), None);

    let result = checker.list_accessible_repo_ids(token).await;
    assert_eq!(result, Ok(Some(vec![7001, 7002])));
}

/// Issue #105: TokenExpired with no stale cache still propagates the error
/// (the caller decides — e.g. list_repositories — but the cache layer must
/// not fabricate an empty list).
#[tokio::test]
#[serial]
async fn test_cached_checker_token_expired_no_stale_returns_error() {
    use boardflow_api::github_access::TokenExpiredGithubAccessChecker;

    let Some(pool) = setup_pool().await else {
        return;
    };
    let token = "gho_mock_token_expired_no_stale";
    let _user_id = create_test_user_with_token(&pool, token).await;

    let inner: Arc<dyn GithubAccessChecker> = Arc::new(TokenExpiredGithubAccessChecker);
    let checker = CachedGithubAccessChecker::with_inner(inner, pool.clone(), None);

    let result = checker.list_accessible_repo_ids(token).await;
    assert_eq!(result, Err(AccessError::TokenExpired));
}

/// Issue #105: Upstream error with no stale cache still propagates the error.
#[tokio::test]
#[serial]
async fn test_cached_checker_upstream_error_no_stale_returns_error() {
    use boardflow_api::github_access::UpstreamErrorGithubAccessChecker;

    let Some(pool) = setup_pool().await else {
        return;
    };
    let token = "gho_mock_upstream_error_no_stale";
    let _user_id = create_test_user_with_token(&pool, token).await;

    let inner: Arc<dyn GithubAccessChecker> = Arc::new(UpstreamErrorGithubAccessChecker);
    let checker = CachedGithubAccessChecker::with_inner(inner, pool.clone(), None);

    let result = checker.list_accessible_repo_ids(token).await;
    assert!(matches!(result, Err(AccessError::Upstream(_))));
}

/// Test: invalidate_repo_cache is callable via trait object (DynGithubAccessChecker)
#[tokio::test]
#[serial]
async fn test_invalidate_repo_cache_via_trait() {
    use boardflow_api::github_access::DynGithubAccessChecker;

    let Some(pool) = setup_pool().await else {
        return;
    };
    let token = "gho_invalidate_trait_test";
    let user_id = create_test_user_with_token(&pool, token).await;

    // Seed a valid cache entry
    let value = serde_json::json!([8001, 8002]);
    boardflow_db::queries::github_api_cache::upsert_cache(
        &pool,
        user_id,
        "accessible_repo_ids",
        &value,
        600,
    )
    .await
    .unwrap();

    // Create checker as DynGithubAccessChecker (trait object)
    let checker: DynGithubAccessChecker =
        Arc::new(CachedGithubAccessChecker::new(pool.clone(), None));

    // Call invalidate_repo_cache via trait
    checker.invalidate_repo_cache(user_id).await.unwrap();

    // Verify cache was removed
    let cached = boardflow_db::queries::github_api_cache::get_valid_cache(
        &pool,
        user_id,
        "accessible_repo_ids",
    )
    .await
    .unwrap();
    assert_eq!(cached, None);
}

/// Test: RateLimited with no stale cache returns RateLimited error
#[tokio::test]
#[serial]
async fn test_cached_checker_rate_limited_no_stale_returns_error() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let token = "gho_rate_limit_no_stale_test";
    let _user_id = create_test_user_with_token(&pool, token).await;

    // No cache entry seeded → stale fallback has nothing to return
    let inner: Arc<dyn GithubAccessChecker> = Arc::new(RateLimitedGithubAccessChecker);
    let checker = CachedGithubAccessChecker::with_inner(inner, pool.clone(), None);

    let result = checker.list_accessible_repo_ids(token).await;
    // Should return RateLimited since no stale cache exists
    assert_eq!(result, Err(AccessError::RateLimited));
}

// ─── Fallback sync tests ─────────────────────────────────────────────────────

/// Test: find_existing_github_ids returns only IDs that exist in DB
#[tokio::test]
#[serial]
async fn test_find_existing_github_ids() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let repo_id_1 = rand_i64();
    let repo_id_2 = rand_i64();
    let repo_id_3 = rand_i64();

    // Insert two repos, skip the third
    let uuid1 = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO repositories (id, github_repository_id, owner, name, installation_id, created_at, updated_at) \
         VALUES ($1, $2, 'owner1', 'repo1', 1001, NOW(), NOW()) \
         ON CONFLICT (github_repository_id) DO NOTHING",
    )
    .bind(uuid1)
    .bind(repo_id_1)
    .execute(&pool)
    .await
    .unwrap();

    let uuid2 = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO repositories (id, github_repository_id, owner, name, installation_id, created_at, updated_at) \
         VALUES ($1, $2, 'owner2', 'repo2', 1001, NOW(), NOW()) \
         ON CONFLICT (github_repository_id) DO NOTHING",
    )
    .bind(uuid2)
    .bind(repo_id_2)
    .execute(&pool)
    .await
    .unwrap();

    let existing = boardflow_db::queries::repository::find_existing_github_ids(
        &pool,
        &[repo_id_1, repo_id_2, repo_id_3],
    )
    .await
    .unwrap();

    assert_eq!(existing.len(), 2);
    assert!(existing.contains(&repo_id_1));
    assert!(existing.contains(&repo_id_2));
    assert!(!existing.contains(&repo_id_3));
}

/// Test: find_existing_github_ids with empty input returns empty
#[tokio::test]
#[serial]
async fn test_find_existing_github_ids_empty_input() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let existing = boardflow_db::queries::repository::find_existing_github_ids(&pool, &[])
        .await
        .unwrap();
    assert!(existing.is_empty());
}

/// Test: Fallback sync skipped when github_app_id is None
#[tokio::test]
#[serial]
async fn test_fallback_sync_skipped_when_app_id_none() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let token = "gho_fallback_no_appid_test";
    let user_id = create_test_user_with_token(&pool, token).await;

    // Seed cache with repo IDs that don't exist in repositories table
    let nonexistent_id = rand_i64();
    let value = serde_json::json!([nonexistent_id]);
    boardflow_db::queries::github_api_cache::upsert_cache(
        &pool,
        user_id,
        "accessible_repo_ids",
        &value,
        600,
    )
    .await
    .unwrap();

    // Create checker with github_app_id = None
    let inner: Arc<dyn GithubAccessChecker> = Arc::new(AllowAllGithubAccessChecker);
    let checker = CachedGithubAccessChecker::with_inner(inner, pool.clone(), None);

    let result = checker.list_accessible_repo_ids(token).await.unwrap();
    assert_eq!(result, Some(vec![nonexistent_id]));

    // Verify no sync throttle cache was written (sync was skipped)
    let sync_cache = boardflow_db::queries::github_api_cache::get_valid_cache(
        &pool,
        user_id,
        "installation_repos_sync",
    )
    .await
    .unwrap();
    assert_eq!(sync_cache, None);
}

/// Test: Fallback sync skipped when all repos already exist in DB
#[tokio::test]
#[serial]
async fn test_fallback_sync_skipped_when_all_repos_exist() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let token = "gho_fallback_all_exist_test";
    let user_id = create_test_user_with_token(&pool, token).await;

    // Create a repo in DB
    let repo_id = rand_i64();
    let uuid = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO repositories (id, github_repository_id, owner, name, installation_id, created_at, updated_at) \
         VALUES ($1, $2, 'testowner', 'testrepo', 1001, NOW(), NOW()) \
         ON CONFLICT (github_repository_id) DO NOTHING",
    )
    .bind(uuid)
    .bind(repo_id)
    .execute(&pool)
    .await
    .unwrap();

    // Seed cache with the existing repo ID
    let value = serde_json::json!([repo_id]);
    boardflow_db::queries::github_api_cache::upsert_cache(
        &pool,
        user_id,
        "accessible_repo_ids",
        &value,
        600,
    )
    .await
    .unwrap();

    // Create checker with a real app_id (sync should still be skipped since no missing repos)
    let inner: Arc<dyn GithubAccessChecker> = Arc::new(AllowAllGithubAccessChecker);
    let checker = CachedGithubAccessChecker::with_inner(inner, pool.clone(), Some(12345));

    let result = checker.list_accessible_repo_ids(token).await.unwrap();
    assert_eq!(result, Some(vec![repo_id]));

    // Verify no sync throttle cache was written (sync was skipped - no missing repos)
    let sync_cache = boardflow_db::queries::github_api_cache::get_valid_cache(
        &pool,
        user_id,
        "installation_repos_sync",
    )
    .await
    .unwrap();
    assert_eq!(sync_cache, None);
}

/// Test: Fallback sync skipped when throttle is active
#[tokio::test]
#[serial]
async fn test_fallback_sync_skipped_when_throttled() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let token = "gho_fallback_throttled_test";
    let user_id = create_test_user_with_token(&pool, token).await;

    // Seed cache with a repo ID that doesn't exist in DB
    let nonexistent_id = rand_i64();
    let value = serde_json::json!([nonexistent_id]);
    boardflow_db::queries::github_api_cache::upsert_cache(
        &pool,
        user_id,
        "accessible_repo_ids",
        &value,
        600,
    )
    .await
    .unwrap();

    // Seed the sync throttle cache (indicates sync was done recently)
    boardflow_db::queries::github_api_cache::upsert_cache(
        &pool,
        user_id,
        "installation_repos_sync",
        &serde_json::json!({"synced": true}),
        600,
    )
    .await
    .unwrap();

    // Create checker with a real app_id
    let inner: Arc<dyn GithubAccessChecker> = Arc::new(AllowAllGithubAccessChecker);
    let checker = CachedGithubAccessChecker::with_inner(inner, pool.clone(), Some(12345));

    let result = checker.list_accessible_repo_ids(token).await.unwrap();
    assert_eq!(result, Some(vec![nonexistent_id]));

    // The repo should NOT have been created (sync was throttled)
    let existing =
        boardflow_db::queries::repository::find_existing_github_ids(&pool, &[nonexistent_id])
            .await
            .unwrap();
    assert!(existing.is_empty());
}

// ─── Success-path fallback sync tests (with wiremock) ────────────────────────

/// Test: Successful fallback sync fetches installations, filters by app_id,
/// fetches repos, and upserts them into DB
#[tokio::test]
#[serial]
async fn test_fallback_sync_success_upserts_repos() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let token = "gho_fallback_success_test";
    let user_id = create_test_user_with_token(&pool, token).await;

    let mock_server = MockServer::start().await;
    let app_id: u64 = 99999;
    let repo_github_id: i64 = rand_i64();

    // Mock GET /user/installations
    Mock::given(method("GET"))
        .and(path_regex(r"^/user/installations$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total_count": 1,
            "installations": [
                {
                    "id": 5001,
                    "app_id": app_id,
                    "suspended_at": null
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    // Mock GET /user/installations/5001/repositories
    Mock::given(method("GET"))
        .and(path_regex(r"^/user/installations/5001/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total_count": 1,
            "repositories": [
                {
                    "id": repo_github_id,
                    "full_name": "testorg/testrepo-synced"
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    // Use AllowAll inner that returns the repo_github_id as accessible
    // (simulating GitHub /user/repos returning this ID)
    let inner: Arc<dyn GithubAccessChecker> = Arc::new(FixedIdsGithubAccessChecker {
        ids: vec![repo_github_id],
    });
    let checker = CachedGithubAccessChecker::with_base_url(
        inner,
        pool.clone(),
        Some(app_id),
        mock_server.uri(),
    );

    let result = checker.list_accessible_repo_ids(token).await.unwrap();
    assert_eq!(result, Some(vec![repo_github_id]));

    // Verify the repo was upserted into DB
    let existing =
        boardflow_db::queries::repository::find_existing_github_ids(&pool, &[repo_github_id])
            .await
            .unwrap();
    assert_eq!(existing, vec![repo_github_id]);

    // Verify throttle cache was set
    let sync_cache = boardflow_db::queries::github_api_cache::get_valid_cache(
        &pool,
        user_id,
        "installation_repos_sync",
    )
    .await
    .unwrap();
    assert!(sync_cache.is_some());

    // Cleanup
    sqlx::query("DELETE FROM repositories WHERE github_repository_id = $1")
        .bind(repo_github_id)
        .execute(&pool)
        .await
        .unwrap();
}

/// Test: Fallback sync filters out installations with wrong app_id
#[tokio::test]
#[serial]
async fn test_fallback_sync_filters_wrong_app_id() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let token = "gho_fallback_appid_filter_test";
    let user_id = create_test_user_with_token(&pool, token).await;

    let mock_server = MockServer::start().await;
    let our_app_id: u64 = 88888;
    let other_app_id: u64 = 77777;
    let repo_github_id: i64 = rand_i64();

    // Mock installations: only "other" app_id present
    Mock::given(method("GET"))
        .and(path_regex(r"^/user/installations$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total_count": 1,
            "installations": [
                {
                    "id": 6001,
                    "app_id": other_app_id,
                    "suspended_at": null
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let inner: Arc<dyn GithubAccessChecker> = Arc::new(FixedIdsGithubAccessChecker {
        ids: vec![repo_github_id],
    });
    let checker = CachedGithubAccessChecker::with_base_url(
        inner,
        pool.clone(),
        Some(our_app_id),
        mock_server.uri(),
    );

    let result = checker.list_accessible_repo_ids(token).await.unwrap();
    assert_eq!(result, Some(vec![repo_github_id]));

    // Repo should NOT be upserted (wrong app_id filtered out)
    let existing =
        boardflow_db::queries::repository::find_existing_github_ids(&pool, &[repo_github_id])
            .await
            .unwrap();
    assert!(existing.is_empty());

    // Throttle should still be set (sync was attempted)
    let sync_cache = boardflow_db::queries::github_api_cache::get_valid_cache(
        &pool,
        user_id,
        "installation_repos_sync",
    )
    .await
    .unwrap();
    assert!(sync_cache.is_some());
}

/// Test: Fallback sync filters out suspended installations
#[tokio::test]
#[serial]
async fn test_fallback_sync_filters_suspended_installations() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let token = "gho_fallback_suspended_test";
    let user_id = create_test_user_with_token(&pool, token).await;

    let mock_server = MockServer::start().await;
    let app_id: u64 = 66666;
    let repo_github_id: i64 = rand_i64();

    // Mock installations: matching app_id but suspended
    Mock::given(method("GET"))
        .and(path_regex(r"^/user/installations$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total_count": 1,
            "installations": [
                {
                    "id": 7001,
                    "app_id": app_id,
                    "suspended_at": "2024-01-01T00:00:00Z"
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let inner: Arc<dyn GithubAccessChecker> = Arc::new(FixedIdsGithubAccessChecker {
        ids: vec![repo_github_id],
    });
    let checker = CachedGithubAccessChecker::with_base_url(
        inner,
        pool.clone(),
        Some(app_id),
        mock_server.uri(),
    );

    let result = checker.list_accessible_repo_ids(token).await.unwrap();
    assert_eq!(result, Some(vec![repo_github_id]));

    // Repo should NOT be upserted (installation is suspended)
    let existing =
        boardflow_db::queries::repository::find_existing_github_ids(&pool, &[repo_github_id])
            .await
            .unwrap();
    assert!(existing.is_empty());

    // Throttle should still be set
    let sync_cache = boardflow_db::queries::github_api_cache::get_valid_cache(
        &pool,
        user_id,
        "installation_repos_sync",
    )
    .await
    .unwrap();
    assert!(sync_cache.is_some());
}

/// Test: Fallback sync handles GitHub API error gracefully (best-effort)
#[tokio::test]
#[serial]
async fn test_fallback_sync_github_api_error_graceful() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let token = "gho_fallback_api_error_test";
    let _user_id = create_test_user_with_token(&pool, token).await;

    let mock_server = MockServer::start().await;
    let repo_github_id: i64 = rand_i64();

    // Mock installations returns 500
    Mock::given(method("GET"))
        .and(path_regex(r"^/user/installations"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let inner: Arc<dyn GithubAccessChecker> = Arc::new(FixedIdsGithubAccessChecker {
        ids: vec![repo_github_id],
    });
    let checker = CachedGithubAccessChecker::with_base_url(
        inner,
        pool.clone(),
        Some(12345),
        mock_server.uri(),
    );

    // Should return successfully despite API error (best-effort sync)
    let result = checker.list_accessible_repo_ids(token).await.unwrap();
    assert_eq!(result, Some(vec![repo_github_id]));

    // Repo should NOT be in DB (sync failed gracefully)
    let existing =
        boardflow_db::queries::repository::find_existing_github_ids(&pool, &[repo_github_id])
            .await
            .unwrap();
    assert!(existing.is_empty());
}

// ─── Test helper: fixed IDs access checker ──────────────────────────────────

/// Mock that returns a fixed set of repo IDs (simulating successful /user/repos)
struct FixedIdsGithubAccessChecker {
    ids: Vec<i64>,
}

#[async_trait::async_trait]
impl GithubAccessChecker for FixedIdsGithubAccessChecker {
    async fn check_access(
        &self,
        _token: &str,
        _owner: &str,
        _name: &str,
    ) -> boardflow_api::github_access::AccessResult {
        boardflow_api::github_access::AccessResult::Allowed
    }

    async fn list_accessible_repo_ids(
        &self,
        _token: &str,
    ) -> Result<Option<Vec<i64>>, AccessError> {
        Ok(Some(self.ids.clone()))
    }
}
