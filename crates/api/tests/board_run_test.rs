use axum::body::Body;
use axum::http::{Request, StatusCode};
use boardflow_api::create_app;
use http_body_util::BodyExt;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn setup_pool() -> Option<PgPool> {
    // Ensure artifact secret is set for create_app
    // SAFETY: tests run sequentially via #[tokio::test] default single-thread;
    // no other thread reads this var concurrently at this point.
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

async fn create_test_token(pool: &PgPool, repository_id: Uuid, installation_id: i64) -> String {
    let raw_token = format!("test_token_{}", Uuid::now_v7());
    let mut hasher = Sha256::new();
    hasher.update(raw_token.as_bytes());
    let token_hash = format!("{:x}", hasher.finalize());
    let token_id = Uuid::now_v7();

    sqlx::query(
        "INSERT INTO boardflow_api_tokens (id, installation_id, repository_id, name, token_hash, created_at) \
         VALUES ($1, $2, $3, $4, $5, NOW())",
    )
    .bind(token_id)
    .bind(installation_id)
    .bind(repository_id)
    .bind("test-token")
    .bind(&token_hash)
    .execute(pool)
    .await
    .unwrap();

    raw_token
}

async fn create_test_repository(pool: &PgPool, github_repository_id: i64, installation_id: i64) -> Uuid {
    let id = Uuid::now_v7();
    let actual_id: Uuid = sqlx::query_scalar(
        "INSERT INTO repositories (id, github_repository_id, owner, name, installation_id, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, NOW(), NOW()) \
         ON CONFLICT (github_repository_id) DO UPDATE SET updated_at = NOW() \
         RETURNING id",
    )
    .bind(id)
    .bind(github_repository_id)
    .bind("test-owner")
    .bind("test-repo")
    .bind(installation_id)
    .fetch_one(pool)
    .await
    .unwrap();
    actual_id
}

async fn create_test_board_project(pool: &PgPool, repository_id: Uuid) -> Uuid {
    let id = Uuid::now_v7();
    let actual_id: Uuid = sqlx::query_scalar(
        "INSERT INTO board_projects (id, repository_id, project_path, project_dir, display_name, \
         issue_sync_status, recreate_issue_on_update, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, 'pending', true, NOW(), NOW()) \
         ON CONFLICT (repository_id, project_path) DO UPDATE SET updated_at = NOW() \
         RETURNING id",
    )
    .bind(id)
    .bind(repository_id)
    .bind(format!("hardware/Project_{}.kicad_pro", id))
    .bind("hardware")
    .bind("TestProject")
    .fetch_one(pool)
    .await
    .unwrap();
    actual_id
}

async fn create_test_board_run(pool: &PgPool, board_project_id: Uuid, status: &str) -> Uuid {
    let id = Uuid::now_v7();
    let run_id = rand_i64();
    sqlx::query(
        "INSERT INTO board_runs (id, board_project_id, commit_sha, branch, ref, github_run_id, github_run_attempt, \
         tree_hash, status, erc_errors, erc_warnings, drc_errors, drc_warnings, review_status, diff_status, created_at, completed_at) \
         VALUES ($1, $2, 'abc123', 'main', 'refs/heads/main', $3, 1, 'treehash', $4, 0, 0, 0, 0, 'pending', 'pending', NOW(), \
         CASE WHEN $4 IN ('failed', 'completed', 'timed_out') THEN NOW() ELSE NULL END)",
    )
    .bind(id)
    .bind(board_project_id)
    .bind(run_id)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
    id
}

fn rand_i64() -> i64 {
    let uuid = Uuid::now_v7();
    let bytes = uuid.as_bytes();
    i64::from_be_bytes(bytes[0..8].try_into().unwrap()).abs()
}

// ─── POST /api/v1/board-runs ─────────────────────────────────────────────────

/// 正常系: board run 作成成功、presigned URL 取得
#[tokio::test]
async fn test_create_board_run_success() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2001;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "board_project_id": format!("bp_{}", bp_id),
        "project_path": "hardware/Test.kicad_pro",
        "tree_hash": "deadbeef123",
        "commit_sha": "abc123",
        "branch": "main",
        "ref": "refs/heads/main",
        "github_run_id": "99999",
        "github_run_attempt": "1"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/board-runs")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert!(json["board_run_id"].as_str().unwrap().starts_with("br_"));
    assert_eq!(json["status"], "created");
    assert!(json["artifact_bundle"].is_object());
    assert_eq!(json["artifact_bundle"]["upload_mode"], "staging_s3");
    assert_eq!(json["artifact_bundle"]["method"], "PUT");
    assert!(json["artifact_bundle"]["upload_url"].as_str().unwrap().contains("presigned=test"));
}

/// 冪等性: 同じ run_id + attempt で再送すると同じ結果
#[tokio::test]
async fn test_create_board_run_idempotent() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2002;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;

    let run_id_val = rand_i64().to_string();
    let body = serde_json::json!({
        "board_project_id": format!("bp_{}", bp_id),
        "project_path": "hardware/Test.kicad_pro",
        "tree_hash": "deadbeef123",
        "commit_sha": "abc123",
        "branch": "main",
        "ref": "refs/heads/main",
        "github_run_id": run_id_val,
        "github_run_attempt": "1"
    });

    // First request
    let app = create_app(pool.clone(), None);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/board-runs")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json1: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    // Second request (same payload)
    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/board-runs")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json2: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json1["board_run_id"], json2["board_run_id"]);
}

/// 認証なし → 401
#[tokio::test]
async fn test_create_board_run_unauthorized() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "board_project_id": "bp_00000000-0000-0000-0000-000000000000",
        "project_path": "hardware/Test.kicad_pro",
        "tree_hash": "deadbeef",
        "commit_sha": "abc",
        "branch": "main",
        "ref": "refs/heads/main",
        "github_run_id": "1",
        "github_run_attempt": "1"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/board-runs")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// 他リポジトリの project → 403
#[tokio::test]
async fn test_create_board_run_forbidden() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2004;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    // Create board_project belonging to a DIFFERENT repository
    let other_repo_id = create_test_repository(&pool, rand_i64(), 9999).await;
    let other_bp_id = create_test_board_project(&pool, other_repo_id).await;

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "board_project_id": format!("bp_{}", other_bp_id),
        "project_path": "hardware/Test.kicad_pro",
        "tree_hash": "deadbeef",
        "commit_sha": "abc",
        "branch": "main",
        "ref": "refs/heads/main",
        "github_run_id": "1",
        "github_run_attempt": "1"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/board-runs")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// ─── POST /api/v1/board-runs/:id/fail ────────────────────────────────────────

/// 正常系: fail 成功
#[tokio::test]
async fn test_fail_board_run_success() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2005;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "created").await;

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "status": "failed",
        "error": {
            "message": "KiCad export failed",
            "details": null
        }
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/board-runs/br_{}/fail", br_id))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["board_run_id"], format!("br_{}", br_id));
    assert_eq!(json["status"], "failed");
    assert!(json["failed_at"].as_str().is_some());
}

/// 冪等性: 既に failed → 同じ結果を返す
#[tokio::test]
async fn test_fail_board_run_idempotent() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2006;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "failed").await;

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "status": "failed",
        "error": { "message": "already failed" }
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/board-runs/br_{}/fail", br_id))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["status"], "failed");
}

/// completed run を fail → 409 Conflict
#[tokio::test]
async fn test_fail_board_run_conflict() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2007;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "status": "failed",
        "error": { "message": "cannot fail" }
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/board-runs/br_{}/fail", br_id))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

/// timed_out run を fail → 410 Gone
#[tokio::test]
async fn test_fail_board_run_gone() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2008;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "timed_out").await;

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "status": "failed",
        "error": { "message": "too late" }
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/board-runs/br_{}/fail", br_id))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::GONE);
}

// ─── POST /api/v1/board-runs/:id/artifact-bundles/import ─────────────────────

/// 正常系: import 成功
#[tokio::test]
async fn test_import_artifact_bundle_success() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2009;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "created").await;

    // Create an artifact_bundle for this run (as would be done by create_board_run)
    let bundle_id = Uuid::now_v7();
    let object_key = format!("staging/runs/br_{}/bundle.zip", br_id);
    sqlx::query(
        "INSERT INTO artifact_bundles (id, board_run_id, intake_mode, staging_object_key, status, received_at) \
         VALUES ($1, $2, 'staging_s3', $3, 'pending', NOW())",
    )
    .bind(bundle_id)
    .bind(br_id)
    .bind(&object_key)
    .execute(&pool)
    .await
    .unwrap();

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "staging_object_key": object_key,
        "bundle_sha256": "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
        "bundle_size_bytes": 12345
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/board-runs/br_{}/artifact-bundles/import", br_id))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert!(json["bundle_id"].as_str().unwrap().starts_with("ab_"));
    assert_eq!(json["status"], "queued");
}

/// 冪等性: 同じ key + sha256 で再送
#[tokio::test]
async fn test_import_artifact_bundle_idempotent() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2010;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "created").await;

    let object_key = format!("staging/runs/br_{}/bundle.zip", br_id);
    let sha256 = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";

    // Pre-create an artifact_bundle with sha256 already set (simulating prior import)
    let bundle_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO artifact_bundles (id, board_run_id, intake_mode, staging_object_key, sha256, size_bytes, status, received_at) \
         VALUES ($1, $2, 'staging_s3', $3, $4, 12345, 'pending', NOW())",
    )
    .bind(bundle_id)
    .bind(br_id)
    .bind(&object_key)
    .bind(sha256)
    .execute(&pool)
    .await
    .unwrap();

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "staging_object_key": object_key,
        "bundle_sha256": sha256,
        "bundle_size_bytes": 12345
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/board-runs/br_{}/artifact-bundles/import", br_id))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["bundle_id"], format!("ab_{}", bundle_id));
    assert_eq!(json["status"], "queued");
}

/// 異なる sha256 → 409 Conflict
#[tokio::test]
async fn test_import_artifact_bundle_conflict() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2011;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "created").await;

    let object_key = format!("staging/runs/br_{}/bundle.zip", br_id);

    // Pre-create bundle with a DIFFERENT sha256
    let bundle_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO artifact_bundles (id, board_run_id, intake_mode, staging_object_key, sha256, size_bytes, status, received_at) \
         VALUES ($1, $2, 'staging_s3', $3, 'different_sha256_value', 999, 'pending', NOW())",
    )
    .bind(bundle_id)
    .bind(br_id)
    .bind(&object_key)
    .execute(&pool)
    .await
    .unwrap();

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "staging_object_key": object_key,
        "bundle_sha256": "new_sha256_that_differs",
        "bundle_size_bytes": 12345
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/board-runs/br_{}/artifact-bundles/import", br_id))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

/// failed run に import → 410 Gone
#[tokio::test]
async fn test_import_artifact_bundle_gone() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2012;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "failed").await;

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "staging_object_key": "uploads/test.zip",
        "bundle_sha256": "abc123",
        "bundle_size_bytes": 100
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/board-runs/br_{}/artifact-bundles/import", br_id))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::GONE);
}

/// completed run への import → 既存 bundle 状態を返し、新 job を作らない
#[tokio::test]
async fn test_import_artifact_bundle_completed_run() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2013;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    // Pre-create bundle for the completed run
    let bundle_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO artifact_bundles (id, board_run_id, intake_mode, staging_object_key, sha256, size_bytes, status, received_at) \
         VALUES ($1, $2, 'staging_s3', $3, $4, 12345, 'completed', NOW())",
    )
    .bind(bundle_id)
    .bind(br_id)
    .bind("staging/runs/br_test/bundle.zip")
    .bind("sha256_completed")
    .execute(&pool)
    .await
    .unwrap();

    let app = create_app(pool.clone(), None);
    let body = serde_json::json!({
        "staging_object_key": "staging/runs/br_test/bundle.zip",
        "bundle_sha256": "sha256_completed",
        "bundle_size_bytes": 12345
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/board-runs/br_{}/artifact-bundles/import", br_id))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["bundle_id"], format!("ab_{}", bundle_id));
    assert_eq!(json["status"], "completed");

    // Verify no new job was created
    let job_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM github_jobs WHERE board_run_id = $1 AND type = 'artifact_bundle_import'",
    )
    .bind(br_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(job_count.0, 0);
}

/// 同一 run に異なる staging_object_key で 409 Conflict
#[tokio::test]
async fn test_import_artifact_bundle_different_staging_key_conflict() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2014;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "created").await;

    // Pre-create bundle with a specific staging_object_key and sha256
    let bundle_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO artifact_bundles (id, board_run_id, intake_mode, staging_object_key, sha256, size_bytes, status, received_at) \
         VALUES ($1, $2, 'staging_s3', $3, $4, 999, 'pending', NOW())",
    )
    .bind(bundle_id)
    .bind(br_id)
    .bind("staging/runs/br_original/bundle.zip")
    .bind("original_sha256")
    .execute(&pool)
    .await
    .unwrap();

    // Send request with DIFFERENT staging_object_key
    let app = create_app(pool, None);
    let body = serde_json::json!({
        "staging_object_key": "staging/runs/br_different/bundle.zip",
        "bundle_sha256": "different_sha256",
        "bundle_size_bytes": 12345
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/board-runs/br_{}/artifact-bundles/import", br_id))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

/// status != "failed" で 400 validation_failed
#[tokio::test]
async fn test_fail_board_run_invalid_status() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2015;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "created").await;

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "status": "completed",
        "error": {
            "message": "wrong status value"
        }
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/board-runs/br_{}/fail", br_id))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// 存在しない board_project_id で 404
#[tokio::test]
async fn test_create_board_run_not_found_project() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2016;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    // Use a valid-format but non-existent board_project_id
    let fake_bp_id = Uuid::now_v7();

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "board_project_id": format!("bp_{}", fake_bp_id),
        "project_path": "hardware/Test.kicad_pro",
        "tree_hash": "deadbeef123",
        "commit_sha": "abc123",
        "branch": "main",
        "ref": "refs/heads/main",
        "github_run_id": "99999",
        "github_run_attempt": "1"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/board-runs")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// 存在しない board_run_id で fail → 404
#[tokio::test]
async fn test_fail_board_run_not_found() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2017;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    let fake_br_id = Uuid::now_v7();

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "status": "failed",
        "error": { "message": "not found test" }
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/board-runs/br_{}/fail", fake_br_id))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// 存在しない board_run_id で import → 404
#[tokio::test]
async fn test_import_artifact_bundle_not_found() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2018;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    let fake_br_id = Uuid::now_v7();

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "staging_object_key": "staging/runs/br_fake/bundle.zip",
        "bundle_sha256": "abc123",
        "bundle_size_bytes": 100
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/board-runs/br_{}/artifact-bundles/import", fake_br_id))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// Race condition: 先着リクエスト成功後、後着が異なる sha256 で 409 になることを確認
/// (トランザクション内で conflict 判定が行われることを順次実行で擬似テスト)
#[tokio::test]
async fn test_import_artifact_bundle_update_conflict() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let installation_id: i64 = 2019;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "created").await;

    // Create an artifact_bundle for this run (sha256 is NULL initially)
    let bundle_id = Uuid::now_v7();
    let object_key = format!("staging/runs/br_{}/bundle.zip", br_id);
    sqlx::query(
        "INSERT INTO artifact_bundles (id, board_run_id, intake_mode, staging_object_key, status, received_at) \
         VALUES ($1, $2, 'staging_s3', $3, 'pending', NOW())",
    )
    .bind(bundle_id)
    .bind(br_id)
    .bind(&object_key)
    .execute(&pool)
    .await
    .unwrap();

    // First request: import succeeds with sha256_a
    let app = create_app(pool.clone(), None);
    let body1 = serde_json::json!({
        "staging_object_key": object_key,
        "bundle_sha256": "aaaa1234567890abcdef1234567890abcdef1234567890abcdef1234567890aa",
        "bundle_size_bytes": 10000
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v1/board-runs/br_{}/artifact-bundles/import",
                    br_id
                ))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body1).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["status"], "queued");

    // Second request: different sha256 → should get 409 Conflict
    // (The bundle already has sha256 set from the first request)
    let app = create_app(pool.clone(), None);
    let body2 = serde_json::json!({
        "staging_object_key": object_key,
        "bundle_sha256": "bbbb1234567890abcdef1234567890abcdef1234567890abcdef1234567890bb",
        "bundle_size_bytes": 20000
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v1/board-runs/br_{}/artifact-bundles/import",
                    br_id
                ))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body2).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}
