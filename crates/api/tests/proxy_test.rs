use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use boardflow_api::artifact_token::generate_artifact_token;
use boardflow_api::create_app_with_config;
use boardflow_api::github_access::{AllowAllGithubAccessChecker, DynGithubAccessChecker};
use hmac::{Hmac, Mac};
use http_body_util::BodyExt;
use sha2::Sha256;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

const TEST_SECRET: &[u8] = b"test-secret-for-proxy-tests";

/// Create a token that is already expired (expires 1 hour in the past)
fn create_expired_token(artifact_id: Uuid, user_id: Uuid, secret: &[u8]) -> String {
    let expires = chrono::Utc::now().timestamp() - 3600; // 1 hour in the past
    let payload = format!("{artifact_id}:{user_id}:{expires}");
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(payload.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    let token_raw = format!("{payload}:{sig}");
    URL_SAFE_NO_PAD.encode(token_raw.as_bytes())
}

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
        Some("https://app.boardflow.example.com".to_string()),
        None,
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
                .uri(format!("/proxy/artifacts/art_{artifact_id}"))
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
                .uri(format!("/proxy/artifacts/art_{artifact_id}?token=invalid-garbage"))
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
                .uri(format!("/proxy/artifacts/art_{artifact_id}?token={token}"))
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
                .uri(format!("/proxy/artifacts/art_{artifact_id}?token={token}"))
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
                .uri(format!("/proxy/artifacts/art_{artifact_id}?token={token}"))
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
                .uri(format!("/proxy/artifacts/art_{artifact_id}?token={token}"))
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
                .uri(format!("/proxy/artifacts/art_{artifact_id}?token={token}"))
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

// ─── Test: invalid UUID in path → 400 ───────────────────────────────────────

#[tokio::test]
async fn test_proxy_invalid_uuid_path_returns_400() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_proxy_test_app(pool.clone());

    // Generate token for a valid artifact_id but request with invalid path (no art_ prefix)
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

    // Invalid format (missing art_ prefix or invalid UUID) → 400 validation error
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "validation_failed");
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
                .uri(format!("/proxy/artifacts/art_{artifact_id}?token="))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ─── Test: raw UUID without art_ prefix → 400 ───────────────────────────────

#[tokio::test]
async fn test_proxy_raw_uuid_without_prefix_returns_400() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_proxy_test_app(pool.clone());

    let artifact_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let token = generate_artifact_token(artifact_id, user_id, TEST_SECRET);

    // Use raw UUID without art_ prefix — should fail validation
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/proxy/artifacts/{artifact_id}?token={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "validation_failed");
}

// ─── Test: expired token → 401 ──────────────────────────────────────────────

#[tokio::test]
async fn test_proxy_expired_token_returns_401() {
    let Some(pool) = setup_pool().await else { return };
    let app = create_proxy_test_app(pool.clone());

    let artifact_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();

    // Manually create an expired token (expires in the past)
    let expired_token = create_expired_token(artifact_id, user_id, TEST_SECRET);

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/proxy/artifacts/art_{artifact_id}?token={expired_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "unauthorized");
    assert!(json["error"]["message"].as_str().unwrap().contains("expired"));
}

// ─── Test: viewer-sources URL format end-to-end ─────────────────────────────

#[tokio::test]
async fn test_proxy_viewer_sources_url_format() {
    // Verify that the URL format generated by viewer-sources (art_{uuid}) works
    let Some(pool) = setup_pool().await else { return };
    let app = create_proxy_test_app(pool.clone());

    let user_id = create_test_user(&pool).await;
    let repo_id = create_test_repository(&pool).await;
    let project_id = create_test_board_project(&pool, repo_id).await;
    let run_id = create_test_board_run(&pool, project_id).await;
    let artifact_id = create_test_artifact(&pool, run_id, "available", "schematic_pdf").await;

    // Generate token exactly as viewer-sources would
    let token = generate_artifact_token(artifact_id, user_id, TEST_SECRET);
    // Format URL exactly as viewer-sources does: /proxy/artifacts/art_{uuid}?token=...
    let url = format!("/proxy/artifacts/art_{artifact_id}?token={token}");

    let response = app
        .oneshot(
            Request::builder()
                .uri(&url)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should reach S3 layer (returns 500 because no S3 client in test)
    // but importantly does NOT return 400 or 401 — the URL format is correct
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // "storage not configured" means we got past auth and DB lookup successfully
    assert_eq!(json["error"]["message"], "storage not configured");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Header generation helper unit tests (no S3/DB dependency)
// ═══════════════════════════════════════════════════════════════════════════════

use boardflow_api::routes::proxy::build_response_headers;

// ─── Test: ibom_html CSP includes sandbox allow-scripts ──────────────────────

#[test]
fn test_headers_ibom_html_has_sandbox_csp() {
    let headers = build_response_headers(
        "text/html",
        "ibom_html",
        "https://app.boardflow.example.com",
        Some(4096),
        Some("ibom.html"),
    );

    let csp = headers.get("Content-Security-Policy").unwrap().to_str().unwrap();
    assert!(csp.starts_with("sandbox allow-scripts;"), "CSP should start with sandbox directive, got: {csp}");
    assert!(csp.contains("script-src 'unsafe-inline'"));
    assert!(csp.contains("style-src 'unsafe-inline'"));
    assert!(csp.contains("img-src data:"));
    assert!(csp.contains("frame-ancestors https://app.boardflow.example.com"));
    assert!(csp.contains("default-src 'none'"));
}

// ─── Test: ibom_html does NOT get X-Frame-Options ────────────────────────────

#[test]
fn test_headers_ibom_html_no_x_frame_options() {
    let headers = build_response_headers(
        "text/html",
        "ibom_html",
        "https://app.boardflow.example.com",
        None,
        None,
    );

    assert!(headers.get("X-Frame-Options").is_none(), "iframe artifacts should not have X-Frame-Options");
}

// ─── Test: non-iframe artifact gets X-Frame-Options: DENY ────────────────────

#[test]
fn test_headers_non_iframe_has_x_frame_options_deny() {
    let headers = build_response_headers(
        "application/pdf",
        "schematic_pdf",
        "https://app.boardflow.example.com",
        Some(2048),
        Some("schematic.pdf"),
    );

    let xfo = headers.get("X-Frame-Options").unwrap().to_str().unwrap();
    assert_eq!(xfo, "DENY");
}

// ─── Test: non-iframe CSP has frame-ancestors 'none' ─────────────────────────

#[test]
fn test_headers_non_iframe_csp_no_sandbox() {
    let headers = build_response_headers(
        "image/svg+xml",
        "schematic_svg",
        "https://app.boardflow.example.com",
        None,
        None,
    );

    let csp = headers.get("Content-Security-Policy").unwrap().to_str().unwrap();
    assert_eq!(csp, "default-src 'none'; frame-ancestors 'none'");
    assert!(!csp.contains("sandbox"));
}

// ─── Test: common security headers present ───────────────────────────────────

#[test]
fn test_headers_common_security_headers() {
    let headers = build_response_headers(
        "application/pdf",
        "schematic_pdf",
        "https://app.boardflow.example.com",
        None,
        None,
    );

    assert_eq!(headers.get("X-Content-Type-Options").unwrap(), "nosniff");
    assert_eq!(headers.get("Referrer-Policy").unwrap(), "no-referrer");
    assert_eq!(headers.get("Access-Control-Allow-Methods").unwrap(), "GET");
    assert_eq!(headers.get("Vary").unwrap(), "Origin");
    assert_eq!(
        headers.get("Access-Control-Allow-Origin").unwrap(),
        "https://app.boardflow.example.com"
    );
}

// ─── Test: Content-Length set when size_bytes provided ────────────────────────

#[test]
fn test_headers_content_length_set() {
    let headers = build_response_headers(
        "application/pdf",
        "schematic_pdf",
        "https://app.boardflow.example.com",
        Some(12345),
        None,
    );

    assert_eq!(headers.get("Content-Length").unwrap(), "12345");
}

// ─── Test: Content-Length absent when size_bytes is None ──────────────────────

#[test]
fn test_headers_content_length_absent_when_none() {
    let headers = build_response_headers(
        "application/pdf",
        "schematic_pdf",
        "https://app.boardflow.example.com",
        None,
        None,
    );

    assert!(headers.get("Content-Length").is_none());
}

// ─── Test: Content-Disposition inline for viewable types ─────────────────────

#[test]
fn test_headers_content_disposition_inline() {
    for artifact_type in &["ibom_html", "schematic_svg", "pcb_svg", "schematic_pdf", "pcb_pdf"] {
        let headers = build_response_headers(
            "application/pdf",
            artifact_type,
            "https://app.boardflow.example.com",
            None,
            Some("test.file"),
        );

        let disp = headers.get("Content-Disposition").unwrap().to_str().unwrap();
        assert!(disp.starts_with("inline;"), "Expected inline for {artifact_type}, got: {disp}");
        assert!(disp.contains("test.file"));
    }
}

// ─── Test: Content-Disposition attachment for other types ─────────────────────

#[test]
fn test_headers_content_disposition_attachment() {
    let headers = build_response_headers(
        "application/zip",
        "gerber_zip",
        "https://app.boardflow.example.com",
        None,
        Some("gerbers.zip"),
    );

    let disp = headers.get("Content-Disposition").unwrap().to_str().unwrap();
    assert!(disp.starts_with("attachment;"), "Expected attachment for gerber_zip, got: {disp}");
    assert!(disp.contains("gerbers.zip"));
}

// ─── Test: Content-Disposition absent when filename is None ──────────────────

#[test]
fn test_headers_content_disposition_absent_when_no_filename() {
    let headers = build_response_headers(
        "application/pdf",
        "schematic_pdf",
        "https://app.boardflow.example.com",
        None,
        None,
    );

    assert!(headers.get("Content-Disposition").is_none());
}

// ─── Test: Content-Type passed through correctly ─────────────────────────────

#[test]
fn test_headers_content_type_passthrough() {
    let headers = build_response_headers(
        "image/svg+xml",
        "schematic_svg",
        "https://app.boardflow.example.com",
        None,
        None,
    );

    assert_eq!(headers.get("Content-Type").unwrap(), "image/svg+xml");
}

// TODO: S3 正常系テスト（ストリーミングレスポンス全体の検証）は
// docker-compose 統合テスト（MinIO）で実施予定。
// ヘッダ生成ロジックは上記ユニットテストでカバー済み。
