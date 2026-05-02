//! Integration tests for BoardRun timeout sweep.
//!
//! Requires DATABASE_URL to be set to a PostgreSQL database with migrations applied.
//! Run with: `cargo test -p boardflow-worker --test timeout_sweep_test -- --ignored`

use sqlx::PgPool;
use uuid::Uuid;
use serial_test::serial;

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

/// Insert a board_run with a specific created_at offset (hours ago) and status.
async fn insert_board_run(
    pool: &PgPool,
    board_project_id: Uuid,
    status: &str,
    hours_ago: i32,
) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO board_runs
        (id, board_project_id, commit_sha, branch, ref, github_run_id, github_run_attempt,
         tree_hash, status, erc_errors, erc_warnings, drc_errors, drc_warnings,
         review_status, diff_status, created_at)
        VALUES ($1, $2, 'abc123', 'main', 'refs/heads/main', $3, 1,
                'treehash', $4, 0, 0, 0, 0, 'pending', 'pending',
                NOW() - make_interval(hours => $5))"#,
    )
    .bind(id)
    .bind(board_project_id)
    .bind(rand::random::<i32>().unsigned_abs() as i64)
    .bind(status)
    .bind(hours_ago)
    .execute(pool)
    .await
    .unwrap();
    id
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
    let project_path = format!("hardware/timeout_test_{}/test.kicad_pro", bp_id);
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

/// Cleanup test data.
async fn cleanup(pool: &PgPool, repo_id: Uuid, bp_id: Uuid) {
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

/// Get the status and timed_out_at for a board_run.
async fn get_run_status(pool: &PgPool, id: Uuid) -> (String, bool) {
    let row = sqlx::query_as::<_, (String, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT status, timed_out_at FROM board_runs WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    (row.0, row.1.is_some())
}

#[tokio::test]
#[ignore]
#[serial]
async fn test_sweep_marks_stale_runs_as_timed_out() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id) = setup_test_data(&pool).await;

    // Insert runs 13 hours ago in each non-terminal status
    let run_created = insert_board_run(&pool, bp_id, "created", 13).await;
    let run_uploading = insert_board_run(&pool, bp_id, "uploading", 13).await;
    let run_importing = insert_board_run(&pool, bp_id, "importing", 13).await;

    // Execute sweep
    let _ids = boardflow_db::queries::board_run::sweep_timed_out(&pool)
        .await
        .unwrap();

    // Verify status and timed_out_at are updated
    let (status, has_timed_out_at) = get_run_status(&pool, run_created).await;
    assert_eq!(status, "timed_out", "created run should be timed_out");
    assert!(
        has_timed_out_at,
        "timed_out_at should be set for created run"
    );

    let (status, has_timed_out_at) = get_run_status(&pool, run_uploading).await;
    assert_eq!(status, "timed_out", "uploading run should be timed_out");
    assert!(
        has_timed_out_at,
        "timed_out_at should be set for uploading run"
    );

    let (status, has_timed_out_at) = get_run_status(&pool, run_importing).await;
    assert_eq!(status, "timed_out", "importing run should be timed_out");
    assert!(
        has_timed_out_at,
        "timed_out_at should be set for importing run"
    );

    cleanup(&pool, repo_id, bp_id).await;
}

#[tokio::test]
#[ignore]
#[serial]
async fn test_sweep_does_not_affect_recent_runs() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id) = setup_test_data(&pool).await;

    // Insert runs 11 hours ago (under the 12-hour threshold)
    let run_created = insert_board_run(&pool, bp_id, "created", 11).await;
    let run_uploading = insert_board_run(&pool, bp_id, "uploading", 11).await;
    let run_importing = insert_board_run(&pool, bp_id, "importing", 11).await;

    // Execute sweep
    let ids = boardflow_db::queries::board_run::sweep_timed_out(&pool)
        .await
        .unwrap();

    // None of the recent runs should be swept
    assert!(
        !ids.contains(&run_created),
        "recent created run should not be swept"
    );
    assert!(
        !ids.contains(&run_uploading),
        "recent uploading run should not be swept"
    );
    assert!(
        !ids.contains(&run_importing),
        "recent importing run should not be swept"
    );

    // Verify status unchanged
    let (status, _) = get_run_status(&pool, run_created).await;
    assert_eq!(status, "created");

    let (status, _) = get_run_status(&pool, run_uploading).await;
    assert_eq!(status, "uploading");

    let (status, _) = get_run_status(&pool, run_importing).await;
    assert_eq!(status, "importing");

    cleanup(&pool, repo_id, bp_id).await;
}

#[tokio::test]
#[ignore]
#[serial]
async fn test_sweep_does_not_affect_terminal_states() {
    let Some(pool) = get_pool().await else { return };
    let (repo_id, bp_id) = setup_test_data(&pool).await;

    // Insert runs 13 hours ago in terminal states
    let run_completed = insert_board_run(&pool, bp_id, "completed", 13).await;
    let run_failed = insert_board_run(&pool, bp_id, "failed", 13).await;
    let run_timed_out = insert_board_run(&pool, bp_id, "timed_out", 13).await;

    // Execute sweep
    let ids = boardflow_db::queries::board_run::sweep_timed_out(&pool)
        .await
        .unwrap();

    // Terminal state runs should not be affected
    assert!(
        !ids.contains(&run_completed),
        "completed run should not be swept"
    );
    assert!(!ids.contains(&run_failed), "failed run should not be swept");
    assert!(
        !ids.contains(&run_timed_out),
        "already timed_out run should not be swept"
    );

    // Verify status unchanged
    let (status, _) = get_run_status(&pool, run_completed).await;
    assert_eq!(status, "completed");

    let (status, _) = get_run_status(&pool, run_failed).await;
    assert_eq!(status, "failed");

    let (status, _) = get_run_status(&pool, run_timed_out).await;
    assert_eq!(status, "timed_out");

    cleanup(&pool, repo_id, bp_id).await;
}
