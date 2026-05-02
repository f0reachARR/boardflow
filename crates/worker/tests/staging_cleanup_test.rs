//! Integration tests for staging bundle cleanup queries.
//!
//! Requires DATABASE_URL to be set to a PostgreSQL database with migrations applied.
//! Run with: `cargo test -p boardflow-worker --test staging_cleanup_test -- --ignored`

use sqlx::PgPool;
use uuid::Uuid;

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

/// Setup: create repository and board_project, return (repo_id, bp_id).
async fn setup_test_data(pool: &PgPool) -> (Uuid, Uuid) {
    let repo_id = Uuid::now_v7();
    let github_repository_id: i64 = rand::random::<i32>().unsigned_abs() as i64;
    let installation_id: i64 = 99999;

    sqlx::query(
        "INSERT INTO repositories (id, github_repository_id, owner, name, installation_id, created_at, updated_at) \
         VALUES ($1, $2, 'test-owner', 'test-repo', $3, NOW(), NOW())",
    )
    .bind(repo_id)
    .bind(github_repository_id)
    .bind(installation_id)
    .execute(pool)
    .await
    .unwrap();

    let bp_id = Uuid::now_v7();
    let project_path = format!("hardware/staging_test_{}/test.kicad_pro", bp_id);
    sqlx::query(
        "INSERT INTO board_projects (id, repository_id, project_path, project_dir, display_name, issue_sync_status, recreate_issue_on_update, created_at, updated_at) \
         VALUES ($1, $2, $3, 'hardware/test', 'test_board', 'pending', true, NOW(), NOW())",
    )
    .bind(bp_id)
    .bind(repo_id)
    .bind(&project_path)
    .execute(pool)
    .await
    .unwrap();

    (repo_id, bp_id)
}

/// Insert a board_run and artifact_bundle with specified delete_after offset.
async fn insert_bundle(
    pool: &PgPool,
    board_project_id: Uuid,
    staging_key: &str,
    hours_offset: i32, // negative = expired (in the past), positive = future
    status: &str,
) -> Uuid {
    let run_id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO board_runs
        (id, board_project_id, commit_sha, branch, ref, github_run_id, github_run_attempt,
         tree_hash, status, erc_errors, erc_warnings, drc_errors, drc_warnings,
         review_status, diff_status, created_at)
        VALUES ($1, $2, 'abc123', 'main', 'refs/heads/main', $3, 1,
                'treehash', 'completed', 0, 0, 0, 0, 'pending', 'pending', NOW())"#,
    )
    .bind(run_id)
    .bind(board_project_id)
    .bind(rand::random::<i32>().unsigned_abs() as i64)
    .execute(pool)
    .await
    .unwrap();

    let bundle_id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO artifact_bundles
        (id, board_run_id, intake_mode, staging_object_key, status, received_at, delete_after)
        VALUES ($1, $2, 'staging_s3', $3, $4, NOW(),
                NOW() + make_interval(hours => $5))"#,
    )
    .bind(bundle_id)
    .bind(run_id)
    .bind(staging_key)
    .bind(status)
    .bind(hours_offset)
    .execute(pool)
    .await
    .unwrap();

    bundle_id
}

/// Expired bundle with staging_object_key set is returned by find_expired_staging.
#[tokio::test]
#[ignore]
async fn test_find_expired_staging_returns_only_expired() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    let (_repo_id, bp_id) = setup_test_data(&pool).await;

    // Insert expired bundle (delete_after = 2 hours ago)
    let expired_id =
        insert_bundle(&pool, bp_id, "staging/expired/bundle.zip", -2, "completed").await;
    // Insert non-expired bundle (delete_after = 2 hours from now)
    let future_id =
        insert_bundle(&pool, bp_id, "staging/future/bundle.zip", 2, "completed").await;

    let expired = boardflow_db::queries::artifact_bundle::find_expired_staging(&pool)
        .await
        .unwrap();
    let expired_ids: Vec<Uuid> = expired.iter().map(|b| b.id).collect();
    assert!(expired_ids.contains(&expired_id));
    assert!(!expired_ids.contains(&future_id));
}

/// clear_staging_object_key sets staging_object_key to NULL.
#[tokio::test]
#[ignore]
async fn test_clear_staging_object_key() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    let (_repo_id, bp_id) = setup_test_data(&pool).await;

    let bundle_id =
        insert_bundle(&pool, bp_id, "staging/to_clear/bundle.zip", -1, "completed").await;

    boardflow_db::queries::artifact_bundle::clear_staging_object_key(&pool, bundle_id)
        .await
        .unwrap();

    let run_id: Uuid =
        sqlx::query_scalar::<_, Uuid>("SELECT board_run_id FROM artifact_bundles WHERE id = $1")
            .bind(bundle_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let bundle = boardflow_db::queries::artifact_bundle::find_by_board_run_id(&pool, run_id)
        .await
        .unwrap()
        .unwrap();

    assert!(bundle.staging_object_key.is_none());
}

/// After clearing staging_object_key, the bundle is no longer returned by find_expired_staging.
#[tokio::test]
#[ignore]
async fn test_cleared_bundle_not_returned_by_find_expired() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    let (_repo_id, bp_id) = setup_test_data(&pool).await;

    let bundle_id =
        insert_bundle(&pool, bp_id, "staging/cleared/bundle.zip", -1, "completed").await;

    // Clear the staging_object_key
    boardflow_db::queries::artifact_bundle::clear_staging_object_key(&pool, bundle_id)
        .await
        .unwrap();

    // Should not appear in expired list
    let expired = boardflow_db::queries::artifact_bundle::find_expired_staging(&pool)
        .await
        .unwrap();
    let expired_ids: Vec<Uuid> = expired.iter().map(|b| b.id).collect();
    assert!(!expired_ids.contains(&bundle_id));
}

/// Bundle with staging_object_key = NULL is not returned (already cleaned up).
#[tokio::test]
#[ignore]
async fn test_null_staging_key_not_returned() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    let (_repo_id, bp_id) = setup_test_data(&pool).await;

    // Insert an expired bundle and immediately clear its key
    let bundle_id =
        insert_bundle(&pool, bp_id, "staging/null_key/bundle.zip", -1, "completed").await;
    boardflow_db::queries::artifact_bundle::clear_staging_object_key(&pool, bundle_id)
        .await
        .unwrap();

    let expired = boardflow_db::queries::artifact_bundle::find_expired_staging(&pool)
        .await
        .unwrap();
    let expired_ids: Vec<Uuid> = expired.iter().map(|b| b.id).collect();
    assert!(!expired_ids.contains(&bundle_id));
}

/// Bundle belonging to a timed-out run gets delete_after set via set_delete_after_for_timed_out_runs.
#[tokio::test]
#[ignore]
async fn test_timed_out_run_bundle_gets_delete_after() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    let (_repo_id, bp_id) = setup_test_data(&pool).await;

    // Insert a board_run that will be "timed out"
    let run_id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO board_runs
        (id, board_project_id, commit_sha, branch, ref, github_run_id, github_run_attempt,
         tree_hash, status, erc_errors, erc_warnings, drc_errors, drc_warnings,
         review_status, diff_status, created_at)
        VALUES ($1, $2, 'abc123', 'main', 'refs/heads/main', $3, 1,
                'treehash', 'timed_out', 0, 0, 0, 0, 'pending', 'pending', NOW())"#,
    )
    .bind(run_id)
    .bind(bp_id)
    .bind(rand::random::<i32>().unsigned_abs() as i64)
    .execute(&pool)
    .await
    .unwrap();

    // Insert bundle with NO delete_after (simulates pre-timeout state)
    let bundle_id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO artifact_bundles
        (id, board_run_id, intake_mode, staging_object_key, status, received_at)
        VALUES ($1, $2, 'staging_s3', 'staging/timed_out/bundle.zip', 'pending', NOW())"#,
    )
    .bind(bundle_id)
    .bind(run_id)
    .execute(&pool)
    .await
    .unwrap();

    // Before: bundle should NOT appear in expired list (no delete_after)
    let expired = boardflow_db::queries::artifact_bundle::find_expired_staging(&pool)
        .await
        .unwrap();
    assert!(!expired.iter().any(|b| b.id == bundle_id));

    // Call set_delete_after_for_timed_out_runs
    let affected = boardflow_db::queries::artifact_bundle::set_delete_after_for_timed_out_runs(
        &pool,
        &[run_id],
    )
    .await
    .unwrap();
    assert_eq!(affected, 1);

    // Verify delete_after is set (it's 7 days in the future, so it won't appear in expired yet)
    let bundle: boardflow_domain::models::artifact_bundle::ArtifactBundle =
        sqlx::query_as("SELECT * FROM artifact_bundles WHERE id = $1")
            .bind(bundle_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(bundle.delete_after.is_some());
}
