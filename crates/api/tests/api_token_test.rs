use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use boardflow_api::create_app_with_config;
use boardflow_api::github_access::{AllowAllGithubAccessChecker, DenyAllGithubAccessChecker, DynGithubAccessChecker};
use http_body_util::BodyExt;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

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

fn create_test_app(pool: PgPool) -> axum::Router {
    let checker: DynGithubAccessChecker = Arc::new(AllowAllGithubAccessChecker);
    create_app_with_config(pool, None, None, None, Some(checker), None, None, None)
}

fn create_deny_app(pool: PgPool) -> axum::Router {
    let checker: DynGithubAccessChecker = Arc::new(DenyAllGithubAccessChecker);
    create_app_with_config(pool, None, None, None, Some(checker), None, None, None)
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
         VALUES ($1, $2, 'testuser', 'https://avatar.example.com/test', 'gho_testtoken', NOW(), NOW())",
    )
    .bind(id)
    .bind(github_user_id)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn create_test_session(pool: &PgPool, user_id: Uuid) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at, created_at) \
         VALUES ($1, $2, NOW() + INTERVAL '7 days', NOW())",
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
    id
}

fn session_cookie(session_id: Uuid) -> String {
    format!("boardflow_session={session_id}")
}

async fn create_test_repository(pool: &PgPool, github_repository_id: i64) -> Uuid {
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
    .bind(1001_i64)
    .fetch_one(pool)
    .await
    .unwrap();
    actual_id
}

// ─── Test: token creation returns 201 with plaintext and correct hash ────────

#[tokio::test]
async fn test_create_api_token_success() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_test_app(pool.clone());

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    create_test_repository(&pool, github_repo_id).await;

    let body = serde_json::json!({ "name": "My Token" });
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/repositories/{github_repo_id}/api-tokens"))
        .header("Content-Type", "application/json")
        .header("Cookie", session_cookie(session_id))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    // Check response structure
    assert!(json["id"].is_string());
    assert_eq!(json["name"], "My Token");
    assert!(json["created_at"].is_string());

    // Check token format: bft_ prefix + 64 hex chars = 68 chars total
    let token = json["token"].as_str().unwrap();
    assert!(token.starts_with("bft_"));
    assert_eq!(token.len(), 68);

    // Verify hash stored in DB matches SHA-256 of plaintext
    let token_id = Uuid::parse_str(json["id"].as_str().unwrap()).unwrap();
    let db_token = sqlx::query_scalar::<_, String>(
        "SELECT token_hash FROM boardflow_api_tokens WHERE id = $1",
    )
    .bind(token_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let expected_hash = hex::encode(hasher.finalize());
    assert_eq!(db_token, expected_hash);
}

// ─── Test: token list returns created tokens without hash ────────────────────

#[tokio::test]
async fn test_list_api_tokens() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_test_app(pool.clone());

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;

    // Insert a token directly
    let token_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO boardflow_api_tokens (id, installation_id, repository_id, name, token_hash, created_at) \
         VALUES ($1, 1001, $2, 'List Test Token', 'fakehash123', NOW())",
    )
    .bind(token_id)
    .bind(repo_id)
    .execute(&pool)
    .await
    .unwrap();

    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/repositories/{github_repo_id}/api-tokens"))
        .header("Cookie", session_cookie(session_id))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    let items = json["items"].as_array().unwrap();
    assert!(!items.is_empty());

    // Find our token
    let item = items.iter().find(|i| i["id"] == token_id.to_string()).unwrap();
    assert_eq!(item["name"], "List Test Token");
    assert!(item["created_at"].is_string());

    // Hash must NOT be present in response
    assert!(item.get("token_hash").is_none());
    assert!(item.get("token").is_none());
}

// ─── Test: revoke sets revoked_at ────────────────────────────────────────────

#[tokio::test]
async fn test_revoke_api_token() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_test_app(pool.clone());

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;

    // Insert a token
    let token_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO boardflow_api_tokens (id, installation_id, repository_id, name, token_hash, created_at) \
         VALUES ($1, 1001, $2, 'Revoke Test', 'hash_to_revoke', NOW())",
    )
    .bind(token_id)
    .bind(repo_id)
    .execute(&pool)
    .await
    .unwrap();

    let req = Request::builder()
        .method("POST")
        .uri(format!(
            "/api/v1/repositories/{github_repo_id}/api-tokens/{token_id}/revoke"
        ))
        .header("Cookie", session_cookie(session_id))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["id"], token_id.to_string());
    assert_eq!(json["name"], "Revoke Test");
    assert!(json["revoked_at"].is_string());
}

// ─── Test: re-revoking is idempotent (preserves original revoked_at) ─────────

#[tokio::test]
async fn test_revoke_idempotent() {
    let Some(pool) = setup_pool().await else { return };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;

    // Insert an already-revoked token
    let token_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO boardflow_api_tokens (id, installation_id, repository_id, name, token_hash, created_at, revoked_at) \
         VALUES ($1, 1001, $2, 'Already Revoked', 'hash_already', NOW(), '2025-01-01T00:00:00Z')",
    )
    .bind(token_id)
    .bind(repo_id)
    .execute(&pool)
    .await
    .unwrap();

    let app = create_test_app(pool.clone());
    let req = Request::builder()
        .method("POST")
        .uri(format!(
            "/api/v1/repositories/{github_repo_id}/api-tokens/{token_id}/revoke"
        ))
        .header("Cookie", session_cookie(session_id))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    // Should preserve original revoked_at, not set a new one
    let revoked_at = json["revoked_at"].as_str().unwrap();
    assert!(revoked_at.starts_with("2025-01-01"));
}

// ─── Test: unauthenticated request returns 401 ───────────────────────────────

#[tokio::test]
async fn test_create_api_token_unauthenticated() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_test_app(pool.clone());

    let github_repo_id = rand_i64();
    create_test_repository(&pool, github_repo_id).await;

    let body = serde_json::json!({ "name": "No Auth Token" });
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/repositories/{github_repo_id}/api-tokens"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ─── Test: non-existent repository returns 404 ───────────────────────────────

#[tokio::test]
async fn test_create_api_token_repo_not_found() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_test_app(pool.clone());

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;

    let body = serde_json::json!({ "name": "Ghost Repo Token" });
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/repositories/99999999/api-tokens")
        .header("Content-Type", "application/json")
        .header("Cookie", session_cookie(session_id))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ─── Test: access denied returns 404 (information hiding) ────────────────────

#[tokio::test]
async fn test_create_api_token_access_denied() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_deny_app(pool.clone());

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    create_test_repository(&pool, github_repo_id).await;

    let body = serde_json::json!({ "name": "Denied Token" });
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/repositories/{github_repo_id}/api-tokens"))
        .header("Content-Type", "application/json")
        .header("Cookie", session_cookie(session_id))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ─── Test: name validation (empty name) ──────────────────────────────────────

#[tokio::test]
async fn test_create_api_token_empty_name() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_test_app(pool.clone());

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    create_test_repository(&pool, github_repo_id).await;

    let body = serde_json::json!({ "name": "   " });
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/repositories/{github_repo_id}/api-tokens"))
        .header("Content-Type", "application/json")
        .header("Cookie", session_cookie(session_id))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ─── Test: name validation (too long) ────────────────────────────────────────

#[tokio::test]
async fn test_create_api_token_name_too_long() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_test_app(pool.clone());

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    create_test_repository(&pool, github_repo_id).await;

    let long_name = "x".repeat(101);
    let body = serde_json::json!({ "name": long_name });
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/repositories/{github_repo_id}/api-tokens"))
        .header("Content-Type", "application/json")
        .header("Cookie", session_cookie(session_id))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ─── Test: revoke token from different repo returns 404 ──────────────────────

#[tokio::test]
async fn test_revoke_token_wrong_repo() {
    let Some(pool) = setup_pool().await else { return };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;

    // Create two repos
    let github_repo_id_1 = rand_i64();
    let repo_id_1 = create_test_repository(&pool, github_repo_id_1).await;
    let github_repo_id_2 = rand_i64();
    let _repo_id_2 = create_test_repository(&pool, github_repo_id_2).await;

    // Token belongs to repo 1
    let token_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO boardflow_api_tokens (id, installation_id, repository_id, name, token_hash, created_at) \
         VALUES ($1, 1001, $2, 'Cross Repo', 'hash_cross', NOW())",
    )
    .bind(token_id)
    .bind(repo_id_1)
    .execute(&pool)
    .await
    .unwrap();

    // Try to revoke via repo 2's endpoint
    let app = create_test_app(pool.clone());
    let req = Request::builder()
        .method("POST")
        .uri(format!(
            "/api/v1/repositories/{github_repo_id_2}/api-tokens/{token_id}/revoke"
        ))
        .header("Cookie", session_cookie(session_id))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ─── Test: cursor pagination (multi-page) ────────────────────────────────────

#[tokio::test]
async fn test_list_api_tokens_cursor_pagination() {
    let Some(pool) = setup_pool().await else { return };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;

    // Insert 3 tokens with distinct created_at
    for i in 0..3 {
        let token_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO boardflow_api_tokens (id, installation_id, repository_id, name, token_hash, created_at) \
             VALUES ($1, 1001, $2, $3, $4, NOW() + make_interval(secs => $5))",
        )
        .bind(token_id)
        .bind(repo_id)
        .bind(format!("Token {i}"))
        .bind(format!("hash_{i}"))
        .bind(i as f64)
        .execute(&pool)
        .await
        .unwrap();
    }

    // Page 1: limit=2
    let app = create_test_app(pool.clone());
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/v1/repositories/{github_repo_id}/api-tokens?limit=2"
        ))
        .header("Cookie", session_cookie(session_id))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(json["has_more"], true);
    let next_cursor = json["next_cursor"].as_str().unwrap();
    assert!(!next_cursor.is_empty());

    // Page 2: use cursor
    let app = create_test_app(pool.clone());
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/v1/repositories/{github_repo_id}/api-tokens?limit=2&cursor={next_cursor}"
        ))
        .header("Cookie", session_cookie(session_id))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(json["has_more"], false);
    assert!(json["next_cursor"].is_null());
}

// ─── Test: list access denied returns 404 ────────────────────────────────────

#[tokio::test]
async fn test_list_api_tokens_access_denied() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_deny_app(pool.clone());

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    create_test_repository(&pool, github_repo_id).await;

    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/repositories/{github_repo_id}/api-tokens"))
        .header("Cookie", session_cookie(session_id))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ─── Test: revoke access denied returns 404 ──────────────────────────────────

#[tokio::test]
async fn test_revoke_api_token_access_denied() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_deny_app(pool.clone());

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;

    let token_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO boardflow_api_tokens (id, installation_id, repository_id, name, token_hash, created_at) \
         VALUES ($1, 1001, $2, 'Deny Revoke', 'hash_deny', NOW())",
    )
    .bind(token_id)
    .bind(repo_id)
    .execute(&pool)
    .await
    .unwrap();

    let req = Request::builder()
        .method("POST")
        .uri(format!(
            "/api/v1/repositories/{github_repo_id}/api-tokens/{token_id}/revoke"
        ))
        .header("Cookie", session_cookie(session_id))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ─── Test: create with malformed JSON returns validation_failed + request_id ─

#[tokio::test]
async fn test_create_api_token_malformed_json() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_test_app(pool.clone());

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    create_test_repository(&pool, github_repo_id).await;

    // Send invalid JSON body
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/repositories/{github_repo_id}/api-tokens"))
        .header("Content-Type", "application/json")
        .header("Cookie", session_cookie(session_id))
        .body(Body::from(b"{ not valid json".to_vec()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    // Verify error structure
    assert_eq!(json["error"]["code"], "validation_failed");
    // request_id must be non-empty (proves handler-level conversion is working)
    let request_id = json["error"]["request_id"].as_str().unwrap();
    assert!(!request_id.is_empty(), "request_id must not be empty for malformed JSON");
}
