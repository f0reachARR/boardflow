use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use boardflow_api::artifact_token::generate_artifact_token;
use boardflow_api::create_app_with_config;
use boardflow_api::github_access::{AllowAllGithubAccessChecker, DynGithubAccessChecker};
use http_body_util::BodyExt;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_SECRET: &[u8] = b"test-secret-for-proxy-tests";

async fn setup_pool() -> Option<PgPool> {
    unsafe { std::env::set_var("BOARDFLOW_ARTIFACT_SECRET", "test-secret-for-proxy-tests") };
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

fn create_proxy_test_app(pool: PgPool) -> axum::Router {
    let checker: DynGithubAccessChecker = Arc::new(AllowAllGithubAccessChecker);
    create_app_with_config(
        pool,
        None, // No S3 client in tests
        None,
        Some(TEST_SECRET.to_vec()),
        Some(checker),
        Some("test-bucket".to_string()),
    )
}

fn rand_i64() -> i64 {
    let uuid = Uuid::now_v7();
    let bytes = uuid.as_bytes();
    i64::from_be_bytes(bytes[0..8].try_into().unwrap()).abs()
}

async fn create_test_user(pool: &PgPool) -> Uuid {
    let id = Uuid::now_v7();
    let github_user_id = rand_i64();
    sqlx::query(
        "INSERT INTO users (id, github_user_id, github_login, github_avatar_url, github_access_token, created_at, updated_at) \
         VALUES ($1, $2, 'proxyuser', 'https://avatar.example.com/proxy', 'gho_proxytoken', NOW(), NOW())",
    )
    .bind(id)
    .bind(github_user_id)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn create_test_repository(pool: &PgPool) -> Uuid {
    let id = Uuid::now_v7();
    let github_repository_id = rand_i64();
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO repositories (id, github_repository_id, owner, name, installation_id, created_at, updated_at) \
         VALUES ($1, $2, 'test-owner', 'test-repo', 1001, NOW(), NOW()) \
         ON CONFLICT (github_repository_id) DO UPDATE SET updated_at = NOW() \
         RETURNING id",
    )
    .bind(id)
    .bind(github_repository_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn create_test_board_project(pool: &PgPool, repository_id: Uuid) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO board_projects (id, repository_id, project_path, project_dir, display_name, \
         issue_sync_status, recreate_issue_on_update, created_at, updated_at) \
         VALUES ($1, $2, $3, 'hardware', 'TestProxyProject', 'pending', true, NOW(), NOW()) \
         ON CONFLICT (repository_id, project_path) DO UPDATE SET updated_at = NOW() \
         RETURNING id",
    )
    .bind(id)
    .bind(repository_id)
    .bind(format!("hardware/Proxy_{}.kicad_pro", id))
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn create_test_board_run(pool: &PgPool, board_project_id: Uuid) -> Uuid {
    let id = Uuid::now_v7();
    let run_id = rand_i64();
    sqlx::query(
        "INSERT INTO board_runs (id, board_project_id, commit_sha, branch, ref, github_run_id, github_run_attempt, \
         tree_hash, status, erc_errors, erc_warnings, drc_errors, drc_warnings, review_status, diff_status, created_at, completed_at) \
         VALUES ($1, $2, 'abc123', 'main', 'refs/heads/main', $3, 1, 'treehash', 'completed', 0, 0, 0, 0, 'pending', 'pending', NOW(), NOW())",
    )
    .bind(id)
    .bind(board_project_id)
    .bind(run_id)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn create_test_artifact(pool: &PgPool, board_run_id: Uuid, status: &str, artifact_type: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO artifacts (id, board_run_id, type, status, filename, content_type, storage_key, sha256, size_bytes, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())",
    )
    .bind(id)
    .bind(board_run_id)
    .bind(artifact_type)
    .bind(status)
    .bind(if status == "available" { Some(format!("{artifact_type}.file")) } else { None })
    .bind(if status == "available" { Some("application/octet-stream") } else { None })
    .bind(if status == "available" { Some(format!("final/{id}")) } else { None })
    .bind(if status == "available" { Some("sha256:abc123") } else { None })
    .bind(if status == "available" { Some(1024_i64) } else { None })
    .execute(pool)
    .await
    .unwrap();
    id
}

// ─── Test: missing token → 401 ──────────────────────────────────────────────

#[tokio::test]
async fn test_proxy_missing_token_returns_401() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_proxy_test_app(pool.clone());

    let artifact_id = Uuid::now_v7();
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/proxy/artifacts/{artifact_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ─── Test: invalid token → 401 ──────────────────────────────────────────────

#[tokio::test]
async fn test_proxy_invalid_token_returns_401() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_proxy_test_app(pool.clone());

    let artifact_id = Uuid::now_v7();
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/proxy/artifacts/{artifact_id}?token=invalid-garbage"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ─── Test: token signed with wrong secret → 401 ─────────────────────────────

#[tokio::test]
async fn test_proxy_wrong_secret_token_returns_401() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_proxy_test_app(pool.clone());

    let artifact_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let wrong_secret = b"completely-wrong-secret";
    let token = generate_artifact_token(artifact_id, user_id, wrong_secret);

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/proxy/artifacts/{artifact_id}?token={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ─── Test: token artifact_id mismatch → 401 ─────────────────────────────────

#[tokio::test]
async fn test_proxy_token_artifact_mismatch_returns_401() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_proxy_test_app(pool.clone());

    let artifact_id = Uuid::now_v7();
    let other_artifact_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let token = generate_artifact_token(other_artifact_id, user_id, TEST_SECRET);

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/proxy/artifacts/{artifact_id}?token={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ─── Test: valid token but artifact not in DB → 404 ──────────────────────────

#[tokio::test]
async fn test_proxy_artifact_not_found_returns_404() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_proxy_test_app(pool.clone());

    let artifact_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let token = generate_artifact_token(artifact_id, user_id, TEST_SECRET);

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/proxy/artifacts/{artifact_id}?token={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── Test: valid token, artifact exists but status != available → 404 ────────

#[tokio::test]
async fn test_proxy_artifact_not_available_returns_404() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_proxy_test_app(pool.clone());

    let user_id = create_test_user(&pool).await;
    let repo_id = create_test_repository(&pool).await;
    let project_id = create_test_board_project(&pool, repo_id).await;
    let run_id = create_test_board_run(&pool, project_id).await;
    let artifact_id = create_test_artifact(&pool, run_id, "failed", "schematic_pdf").await;

    let token = generate_artifact_token(artifact_id, user_id, TEST_SECRET);

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/proxy/artifacts/{artifact_id}?token={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── Test: valid token, available artifact, but no S3 client → 500 ──────────

#[tokio::test]
async fn test_proxy_no_s3_client_returns_500() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_proxy_test_app(pool.clone());

    let user_id = create_test_user(&pool).await;
    let repo_id = create_test_repository(&pool).await;
    let project_id = create_test_board_project(&pool, repo_id).await;
    let run_id = create_test_board_run(&pool, project_id).await;
    let artifact_id = create_test_artifact(&pool, run_id, "available", "schematic_pdf").await;

    let token = generate_artifact_token(artifact_id, user_id, TEST_SECRET);

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/proxy/artifacts/{artifact_id}?token={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "internal_error");
}

// ─── Test: invalid UUID in path → 404 ───────────────────────────────────────

#[tokio::test]
async fn test_proxy_invalid_uuid_path_returns_404() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_proxy_test_app(pool.clone());

    // Generate token for a valid artifact_id but request with invalid path
    let artifact_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let token = generate_artifact_token(artifact_id, user_id, TEST_SECRET);

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/proxy/artifacts/not-a-uuid?token={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Invalid UUID causes not_found (since it can't match the token's artifact_id)
    assert!(
        response.status() == StatusCode::NOT_FOUND
            || response.status() == StatusCode::UNAUTHORIZED
    );
}

// ─── Test: empty token query param → 401 ────────────────────────────────────

#[tokio::test]
async fn test_proxy_empty_token_returns_401() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_proxy_test_app(pool.clone());

    let artifact_id = Uuid::now_v7();
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/proxy/artifacts/{artifact_id}?token="))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
