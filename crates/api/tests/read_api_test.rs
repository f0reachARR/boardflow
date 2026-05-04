use serial_test::serial;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use boardflow_api::create_app_with_config;
use boardflow_api::github_access::{
    AllowAllGithubAccessChecker, DenyAllGithubAccessChecker, DynGithubAccessChecker,
    RateLimitedGithubAccessChecker, UpstreamErrorGithubAccessChecker,
};
use http_body_util::BodyExt;
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

fn create_test_app(pool: PgPool) -> axum::Router {
    let checker: DynGithubAccessChecker = Arc::new(AllowAllGithubAccessChecker);
    create_app_with_config(
        pool,
        None,
        None,
        None,
        Some(checker),
        None,
        None,
        None,
        None,
        None,
        None,
    )
}

fn create_deny_app(pool: PgPool) -> axum::Router {
    let checker: DynGithubAccessChecker = Arc::new(DenyAllGithubAccessChecker);
    create_app_with_config(
        pool,
        None,
        None,
        None,
        Some(checker),
        None,
        None,
        None,
        None,
        None,
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

async fn create_test_expired_session(pool: &PgPool, user_id: Uuid) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at, created_at) \
         VALUES ($1, $2, NOW() - INTERVAL '1 hour', NOW())",
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

async fn create_test_artifact(
    pool: &PgPool,
    board_run_id: Uuid,
    artifact_type: &str,
    status: &str,
) -> Uuid {
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
    .bind(if status == "available" { Some(format!("storage/{id}")) } else { None })
    .bind(if status == "available" { Some("sha256:abc123") } else { None })
    .bind(if status == "available" { Some(1024_i64) } else { None })
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn create_test_run_check(
    pool: &PgPool,
    board_run_id: Uuid,
    check_kind: &str,
    status: &str,
    errors: i32,
    warnings: i32,
) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO run_checks (id, board_run_id, check_kind, status, error_count, warning_count, notice_count, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, 0, NOW())",
    )
    .bind(id)
    .bind(board_run_id)
    .bind(check_kind)
    .bind(status)
    .bind(errors)
    .bind(warnings)
    .execute(pool)
    .await
    .unwrap();
    id
}

// ─── Repository List Tests ───────────────────────────────────────────────────

/// 認証系: セッションなしで401が返る
#[tokio::test]
#[serial]
async fn test_list_repositories_unauthenticated() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// 認証系: 期限切れセッションで401が返る
#[tokio::test]
#[serial]
async fn test_list_repositories_expired_session() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_expired_session(&pool, user_id).await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories")
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// 正常系: リポジトリ一覧を取得できる
#[tokio::test]
#[serial]
async fn test_list_repositories_success() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    create_test_repository(&pool, github_repo_id).await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories")
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert!(json["items"].is_array());
    assert!(json.get("has_more").is_some());
}

/// 正常系: limit パラメータが機能する
#[tokio::test]
#[serial]
async fn test_list_repositories_with_limit() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;

    // 3件作成
    for _ in 0..3 {
        create_test_repository(&pool, rand_i64()).await;
    }

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories?limit=2")
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert!(json["items"].as_array().unwrap().len() <= 2);
    assert_eq!(json["has_more"], true);
    assert!(json["next_cursor"].is_string());
}

/// 境界値: limit=0 は 1 に clamp される
#[tokio::test]
#[serial]
async fn test_list_repositories_limit_zero_clamped() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    create_test_repository(&pool, rand_i64()).await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories?limit=0")
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    // clamp(1, 100) means limit=0 becomes 1
    assert!(json["items"].as_array().unwrap().len() <= 1);
}

/// エラー系: 不正な cursor
#[tokio::test]
#[serial]
async fn test_list_repositories_invalid_cursor() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories?cursor=invalid_base64_cursor")
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"]["code"], "validation_failed");
}

// ─── Repository Detail Tests ─────────────────────────────────────────────────

/// 正常系: リポジトリ詳細を取得できる
#[tokio::test]
#[serial]
async fn test_get_repository_success() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    create_test_repository(&pool, github_repo_id).await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/repositories/{github_repo_id}"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["github_repository_id"], github_repo_id.to_string());
    assert_eq!(json["owner"], "test-owner");
    assert_eq!(json["name"], "test-repo");
    assert!(json["html_url"].as_str().unwrap().contains("github.com"));
    assert!(json["created_at"].is_string());
    assert!(json["updated_at"].is_string());
}

/// エラー系: 存在しないリポジトリ
#[tokio::test]
#[serial]
async fn test_get_repository_not_found() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories/999999999")
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── BoardProject List Tests ─────────────────────────────────────────────────

/// 正常系: BoardProject一覧を取得できる
#[tokio::test]
#[serial]
async fn test_list_board_projects_success() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    create_test_board_project(&pool, repo_id).await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/repositories/{github_repo_id}/board-projects"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert!(json["items"].is_array());
    let items = json["items"].as_array().unwrap();
    assert!(!items.is_empty());
    assert!(
        items[0]["board_project_id"]
            .as_str()
            .unwrap()
            .starts_with("bp_")
    );
    assert!(items[0]["state"].is_string());
}

/// エラー系: 存在しないリポジトリのBoardProject一覧
#[tokio::test]
#[serial]
async fn test_list_board_projects_repo_not_found() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories/999999999/board-projects")
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── BoardProject Detail Tests ───────────────────────────────────────────────

/// 正常系: BoardProject詳細を取得できる
#[tokio::test]
#[serial]
async fn test_get_board_project_success() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-projects/bp_{bp_id}"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["board_project_id"], format!("bp_{bp_id}"));
    assert!(json["repository"].is_object());
    assert_eq!(
        json["repository"]["github_repository_id"],
        github_repo_id.to_string()
    );
    assert_eq!(json["display_name"], "TestProject");
    assert!(json["state"].is_string());
}

/// 正常系: BoardProject state がprocessing (最新runがcreated)
#[tokio::test]
#[serial]
async fn test_get_board_project_state_processing() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    create_test_board_run(&pool, bp_id, "created").await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-projects/bp_{bp_id}"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["state"], "processing");
}

/// 正常系: BoardProject state がfailed (最新runがfailed)
#[tokio::test]
#[serial]
async fn test_get_board_project_state_failed() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    create_test_board_run(&pool, bp_id, "failed").await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-projects/bp_{bp_id}"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["state"], "failed");
}

/// 正常系: BoardProject state がtimed_out
#[tokio::test]
#[serial]
async fn test_get_board_project_state_timed_out() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    create_test_board_run(&pool, bp_id, "timed_out").await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-projects/bp_{bp_id}"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["state"], "timed_out");
}

/// エラー系: 不正なBoardProject IDフォーマット
#[tokio::test]
#[serial]
async fn test_get_board_project_invalid_id() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/board-projects/invalid_id")
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// エラー系: 存在しないBoardProject
#[tokio::test]
#[serial]
async fn test_get_board_project_not_found() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let fake_id = Uuid::now_v7();

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-projects/bp_{fake_id}"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── BoardRun List Tests ─────────────────────────────────────────────────────

/// 正常系: BoardRun一覧を取得できる
#[tokio::test]
#[serial]
async fn test_list_board_runs_success() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    create_test_board_run(&pool, bp_id, "completed").await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-projects/bp_{bp_id}/board-runs"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    let items = json["items"].as_array().unwrap();
    assert!(!items.is_empty());
    assert!(
        items[0]["board_run_id"]
            .as_str()
            .unwrap()
            .starts_with("br_")
    );
    assert_eq!(items[0]["status"], "completed");
}

/// エラー系: 存在しないBoardProjectのBoardRun一覧
#[tokio::test]
#[serial]
async fn test_list_board_runs_bp_not_found() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let fake_id = Uuid::now_v7();

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-projects/bp_{fake_id}/board-runs"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── BoardRun Detail Tests ───────────────────────────────────────────────────

/// 正常系: BoardRun詳細を取得できる（checks + artifact_summary含む）
#[tokio::test]
#[serial]
async fn test_get_board_run_success() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    // Add checks
    create_test_run_check(&pool, br_id, "erc", "passed", 0, 2).await;
    create_test_run_check(&pool, br_id, "drc", "failed", 1, 4).await;

    // Add artifacts
    create_test_artifact(&pool, br_id, "schematic_pdf", "available").await;
    create_test_artifact(&pool, br_id, "drill_zip", "missing").await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["board_run_id"], format!("br_{br_id}"));
    assert_eq!(json["status"], "completed");

    // Check checks array
    let checks = json["checks"].as_array().unwrap();
    assert_eq!(checks.len(), 2);
    assert_eq!(checks[0]["kind"], "drc");
    assert_eq!(checks[0]["error_count"], 1);
    assert_eq!(checks[1]["kind"], "erc");
    assert_eq!(checks[1]["status"], "passed");

    // Check artifact_summary
    assert_eq!(json["artifact_summary"]["available"], 1);
    assert_eq!(json["artifact_summary"]["missing"], 1);
}

/// エラー系: 不正なBoardRun IDフォーマット
#[tokio::test]
#[serial]
async fn test_get_board_run_invalid_id() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/board-runs/invalid_id")
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// エラー系: 存在しないBoardRun
#[tokio::test]
#[serial]
async fn test_get_board_run_not_found() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let fake_id = Uuid::now_v7();

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{fake_id}"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── Artifact List Tests ─────────────────────────────────────────────────────

/// 正常系: Artifact一覧を取得できる
#[tokio::test]
#[serial]
async fn test_list_artifacts_success() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    create_test_artifact(&pool, br_id, "schematic_pdf", "available").await;
    create_test_artifact(&pool, br_id, "drill_zip", "missing").await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}/artifacts"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);

    // Available artifact should have artifact_id
    let available = items.iter().find(|i| i["status"] == "available").unwrap();
    assert!(
        available["artifact_id"]
            .as_str()
            .unwrap()
            .starts_with("art_")
    );
    assert!(available["filename"].is_string());
    assert!(available["size_bytes"].is_number());

    // Missing artifact should not have artifact_id
    let missing = items.iter().find(|i| i["status"] == "missing").unwrap();
    assert!(missing.get("artifact_id").is_none() || missing["artifact_id"].is_null());
}

/// エラー系: 存在しないBoardRunのArtifact一覧
#[tokio::test]
#[serial]
async fn test_list_artifacts_run_not_found() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let fake_id = Uuid::now_v7();

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{fake_id}/artifacts"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── Viewer Sources Tests ────────────────────────────────────────────────────

/// 正常系: Viewer Sources を取得できる（全artifact available）
#[tokio::test]
#[serial]
async fn test_get_viewer_sources_all_available() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    // Create all required artifacts as available
    create_test_artifact(&pool, br_id, "kicad_pro", "available").await;
    create_test_artifact(&pool, br_id, "kicad_sch", "available").await;
    create_test_artifact(&pool, br_id, "kicad_pcb", "available").await;
    create_test_artifact(&pool, br_id, "schematic_pdf", "available").await;
    create_test_artifact(&pool, br_id, "pcb_top_svg", "available").await;
    create_test_artifact(&pool, br_id, "pcb_bottom_svg", "available").await;
    create_test_artifact(&pool, br_id, "ibom_html", "available").await;
    create_test_artifact(&pool, br_id, "bom_csv", "available").await;
    create_test_artifact(&pool, br_id, "gerber_zip", "available").await;
    create_test_artifact(&pool, br_id, "drill_zip", "available").await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}/viewer-sources"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["board_run_id"], format!("br_{br_id}"));
    assert!(json["expires_at"].is_string());

    let viewers = &json["viewers"];
    assert_eq!(viewers["kicanvas"]["status"], "available");
    assert_eq!(viewers["schematic"]["status"], "available");
    assert_eq!(viewers["pcb_preview"]["status"], "available");
    assert_eq!(viewers["ibom"]["status"], "available");
    assert_eq!(viewers["bom"]["status"], "available");
    assert_eq!(viewers["fabrication"]["status"], "available");

    // KiCanvas sources should have 3 items
    let kicanvas_sources = viewers["kicanvas"]["sources"].as_array().unwrap();
    assert_eq!(kicanvas_sources.len(), 3);

    // Each source should have a URL with proxy path and a real token
    for src in kicanvas_sources {
        let url = src["url"].as_str().unwrap();
        assert!(url.contains("/proxy/artifacts/"));
        assert!(url.contains("?token="));
        // Token should not be "placeholder"
        assert!(!url.contains("token=placeholder"));
    }
}

/// 正常系: 一部のartifactが missing の場合は partial
#[tokio::test]
#[serial]
async fn test_get_viewer_sources_partial() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    // Only gerber available, drill missing
    create_test_artifact(&pool, br_id, "gerber_zip", "available").await;
    create_test_artifact(&pool, br_id, "drill_zip", "missing").await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}/viewer-sources"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["viewers"]["fabrication"]["status"], "partial");
}

/// 正常系: artifact が全くない場合は missing
#[tokio::test]
#[serial]
async fn test_get_viewer_sources_missing() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;
    // No artifacts at all

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}/viewer-sources"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["viewers"]["kicanvas"]["status"], "missing");
    assert_eq!(json["viewers"]["schematic"]["status"], "missing");
    assert_eq!(json["viewers"]["fabrication"]["status"], "missing");
}

/// 正常系: 全artifactが skipped の場合は skipped
#[tokio::test]
#[serial]
async fn test_get_viewer_sources_skipped() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    // All fabrication artifacts are skipped
    create_test_artifact(&pool, br_id, "gerber_zip", "skipped").await;
    create_test_artifact(&pool, br_id, "drill_zip", "skipped").await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}/viewer-sources"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["viewers"]["fabrication"]["status"], "skipped");
}

/// エラー系: 存在しないBoardRunのViewer Sources
#[tokio::test]
#[serial]
async fn test_get_viewer_sources_run_not_found() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let fake_id = Uuid::now_v7();

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{fake_id}/viewer-sources"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// 正常系: viewer-sources が artifact_base_url を使った絶対URLを生成する
#[tokio::test]
#[serial]
async fn test_get_viewer_sources_returns_absolute_url_with_custom_base() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    create_test_artifact(&pool, br_id, "kicad_pro", "available").await;
    create_test_artifact(&pool, br_id, "kicad_sch", "available").await;
    create_test_artifact(&pool, br_id, "kicad_pcb", "available").await;

    let checker: DynGithubAccessChecker = Arc::new(AllowAllGithubAccessChecker);
    let app = create_app_with_config(
        pool,
        None,
        None,
        None,
        Some(checker),
        None,
        None,
        None,
        Some("https://artifacts.boardflow.example.com".to_string()),
        None,
        None,
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}/viewer-sources"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    let kicanvas_sources = json["viewers"]["kicanvas"]["sources"].as_array().unwrap();
    for src in kicanvas_sources {
        let url = src["url"].as_str().unwrap();
        assert!(
            url.starts_with("https://artifacts.boardflow.example.com/proxy/artifacts/art_"),
            "URL should be absolute with configured base, got: {url}"
        );
        assert!(url.contains("?token="));
    }
}

// ─── Pagination Integration Tests ────────────────────────────────────────────

/// 統合: cursor pagination でページ遷移ができる
#[tokio::test]
#[serial]
async fn test_pagination_cursor_traversal() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;

    // Create 3 board runs
    for _ in 0..3 {
        create_test_board_run(&pool, bp_id, "completed").await;
        // Small delay to ensure different created_at
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // First page (limit=2)
    let app = create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-projects/bp_{bp_id}/board-runs?limit=2"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["items"].as_array().unwrap().len(), 2);
    assert_eq!(json["has_more"], true);
    let next_cursor = json["next_cursor"].as_str().unwrap();

    // Second page using cursor
    let app2 = create_test_app(pool);
    let response2 = app2
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-projects/bp_{bp_id}/board-runs?limit=2&cursor={next_cursor}"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response2.status(), StatusCode::OK);
    let body_bytes2 = response2.into_body().collect().await.unwrap().to_bytes();
    let json2: serde_json::Value = serde_json::from_slice(&body_bytes2).unwrap();

    assert_eq!(json2["items"].as_array().unwrap().len(), 1);
    assert_eq!(json2["has_more"], false);
    assert!(json2["next_cursor"].is_null());
}

// ─── Authorization Deny Tests ────────────────────────────────────────────────

/// 権限チェック: リポジトリアクセス拒否時にlist_repositoriesが空を返す
#[tokio::test]
#[serial]
async fn test_list_repositories_denied_returns_empty() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    create_test_repository(&pool, rand_i64()).await;

    let app = create_deny_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories")
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["items"].as_array().unwrap().len(), 0);
    assert_eq!(json["has_more"], false);
}

/// 権限チェック: リポジトリアクセス拒否時にget_repositoryが404を返す
#[tokio::test]
#[serial]
async fn test_get_repository_denied_returns_404() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    create_test_repository(&pool, github_repo_id).await;

    let app = create_deny_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/repositories/{github_repo_id}"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// 権限チェック: リポジトリアクセス拒否時にlist_board_projectsが404を返す
#[tokio::test]
#[serial]
async fn test_list_board_projects_denied_returns_404() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    create_test_board_project(&pool, repo_id).await;

    let app = create_deny_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/repositories/{github_repo_id}/board-projects"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// 権限チェック: リポジトリアクセス拒否時にget_board_projectが404を返す
#[tokio::test]
#[serial]
async fn test_get_board_project_denied_returns_404() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;

    let app = create_deny_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-projects/bp_{bp_id}"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// 権限チェック: リポジトリアクセス拒否時にget_board_runが404を返す
#[tokio::test]
#[serial]
async fn test_get_board_run_denied_returns_404() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    let app = create_deny_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// 権限チェック: リポジトリアクセス拒否時にlist_artifactsが404を返す
#[tokio::test]
#[serial]
async fn test_list_artifacts_denied_returns_404() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    let app = create_deny_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}/artifacts"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── Pagination with Access Filter Tests ─────────────────────────────────────

/// Pagination整合性: deny-allでrepository一覧が空かつhas_more=false（pre-filterのため）
#[tokio::test]
#[serial]
async fn test_list_repositories_deny_all_pagination_integrity() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;

    // Create multiple repos that would normally cause has_more=true with limit=1
    for _ in 0..3 {
        create_test_repository(&pool, rand_i64()).await;
    }

    let app = create_deny_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories?limit=1")
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    // With pre-filter, deny-all means empty list with no false has_more
    assert_eq!(json["items"].as_array().unwrap().len(), 0);
    assert_eq!(json["has_more"], false);
    assert!(json["next_cursor"].is_null());
}

/// Pagination整合性: allow-allでrepository一覧のページ遷移が正しく動作する
#[tokio::test]
#[serial]
async fn test_list_repositories_allow_all_pagination_cursor() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;

    // Create 3 repos with small delays for distinct updated_at
    let mut repo_ids = Vec::new();
    for _ in 0..3 {
        let gid = rand_i64();
        create_test_repository(&pool, gid).await;
        repo_ids.push(gid);
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // First page (limit=2)
    let app = create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories?limit=2")
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    let page1_items = json["items"].as_array().unwrap();
    assert_eq!(page1_items.len(), 2);
    assert_eq!(json["has_more"], true);
    let next_cursor = json["next_cursor"].as_str().unwrap();

    // Second page using cursor
    let app2 = create_test_app(pool);
    let response2 = app2
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/repositories?limit=2&cursor={next_cursor}"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response2.status(), StatusCode::OK);
    let body_bytes2 = response2.into_body().collect().await.unwrap().to_bytes();
    let json2: serde_json::Value = serde_json::from_slice(&body_bytes2).unwrap();

    let page2_items = json2["items"].as_array().unwrap();
    // Should have at least 1 item (could be more from other tests but that's fine)
    assert!(!page2_items.is_empty());

    // No overlap between page 1 and page 2
    let page1_ids: Vec<&str> = page1_items
        .iter()
        .map(|i| i["github_repository_id"].as_str().unwrap())
        .collect();
    for item in page2_items {
        let id = item["github_repository_id"].as_str().unwrap();
        assert!(
            !page1_ids.contains(&id),
            "page 2 should not contain items from page 1"
        );
    }
}

// ─── Rate Limited / Upstream Error tests ─────────────────────────────────────

fn create_rate_limited_app(pool: PgPool) -> axum::Router {
    let checker: DynGithubAccessChecker = Arc::new(RateLimitedGithubAccessChecker);
    create_app_with_config(
        pool,
        None,
        None,
        None,
        Some(checker),
        None,
        None,
        None,
        None,
        None,
        None,
    )
}

fn create_upstream_error_app(pool: PgPool) -> axum::Router {
    let checker: DynGithubAccessChecker = Arc::new(UpstreamErrorGithubAccessChecker);
    create_app_with_config(
        pool,
        None,
        None,
        None,
        Some(checker),
        None,
        None,
        None,
        None,
        None,
        None,
    )
}

#[tokio::test]
#[serial]
async fn test_get_repository_rate_limited_returns_429() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let app = create_rate_limited_app(pool.clone());

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let _repo_id = create_test_repository(&pool, github_repo_id).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/repositories/{}", github_repo_id))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"]["code"], "rate_limited");
}

#[tokio::test]
#[serial]
async fn test_get_repository_upstream_error_returns_500() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let app = create_upstream_error_app(pool.clone());

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let _repo_id = create_test_repository(&pool, github_repo_id).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/repositories/{}", github_repo_id))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"]["code"], "internal_error");
}

#[tokio::test]
#[serial]
async fn test_list_repositories_rate_limited_returns_429() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let app = create_rate_limited_app(pool.clone());

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories")
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"]["code"], "rate_limited");
}

#[tokio::test]
#[serial]
async fn test_list_repositories_upstream_error_returns_500() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let app = create_upstream_error_app(pool.clone());

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories")
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"]["code"], "internal_error");
}

// ─── Diff Read API Tests ─────────────────────────────────────────────────────

async fn create_test_diff(
    pool: &PgPool,
    board_run_id: Uuid,
    base_board_run_id: Option<Uuid>,
    status: &str,
) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO board_run_diffs (id, board_run_id, base_board_run_id, status, summary_json, error_message, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, NOW())",
    )
    .bind(id)
    .bind(board_run_id)
    .bind(base_board_run_id)
    .bind(status)
    .bind(if status == "ready" {
        Some(serde_json::json!({"added_files": 2, "removed_files": 0}))
    } else {
        None
    })
    .bind(if status == "failed" {
        Some("diff computation failed")
    } else {
        None
    })
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn create_test_diff_metadata(pool: &PgPool, board_run_id: Uuid) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO board_run_diff_metadata (id, board_run_id, file_hashes_json, bom_summary_json, checks_summary_json, artifacts_summary_json, previews_json, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())",
    )
    .bind(id)
    .bind(board_run_id)
    .bind(Some(serde_json::json!({"main.kicad_sch": "changed"})))
    .bind(Some(serde_json::json!({"added": 1, "removed": 0})))
    .bind(Some(serde_json::json!({"erc_errors": 0})))
    .bind(Some(serde_json::json!({"available": 5})))
    .bind(Some(serde_json::json!({"top": "url1"})))
    .execute(pool)
    .await
    .unwrap();
    id
}

/// 正常系: diff status=ready、metadata あり
#[tokio::test]
#[serial]
async fn test_get_board_run_diff_ready() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;
    let base_br_id = create_test_board_run(&pool, bp_id, "completed").await;

    create_test_diff(&pool, br_id, Some(base_br_id), "ready").await;
    create_test_diff_metadata(&pool, br_id).await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}/diff"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["board_run_id"], format!("br_{br_id}"));
    assert_eq!(json["base_board_run_id"], format!("br_{base_br_id}"));
    assert_eq!(json["status"], "ready");
    assert!(json["summary"].is_object());
    assert!(json["metadata"].is_object());
    assert_eq!(
        json["metadata"]["file_hashes"],
        serde_json::json!({"main.kicad_sch": "changed"})
    );
    assert_eq!(
        json["metadata"]["bom_summary"],
        serde_json::json!({"added": 1, "removed": 0})
    );
    assert!(json["error_message"].is_null());
    assert!(json["created_at"].is_string());
}

/// 正常系: diff status=no_baseline、base_board_run_id=null、metadata なし
#[tokio::test]
#[serial]
async fn test_get_board_run_diff_no_baseline() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    create_test_diff(&pool, br_id, None, "no_baseline").await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}/diff"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["board_run_id"], format!("br_{br_id}"));
    assert!(json["base_board_run_id"].is_null());
    assert_eq!(json["status"], "no_baseline");
    assert!(json["summary"].is_null());
    assert!(
        json.get("metadata").is_some(),
        "metadata field must be present"
    );
    assert!(json["metadata"].is_null());
    assert!(
        json.get("error_message").is_some(),
        "error_message field must be present"
    );
    assert!(json["error_message"].is_null());
}

/// 異常系: diff 未作成 → 404
#[tokio::test]
#[serial]
async fn test_get_board_run_diff_not_found() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}/diff"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"]["code"], "not_found");
}

/// 異常系: 不正ID → 400
#[tokio::test]
#[serial]
async fn test_get_board_run_diff_invalid_id() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/board-runs/invalid_id/diff")
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"]["code"], "validation_failed");
}

/// 異常系: アクセス拒否 → 404
#[tokio::test]
#[serial]
async fn test_get_board_run_diff_denied() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    create_test_diff(&pool, br_id, None, "ready").await;

    let app = create_deny_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}/diff"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"]["code"], "not_found");
}

/// 正常系: diff status=failed、error_message あり
#[tokio::test]
#[serial]
async fn test_get_board_run_diff_failed() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;
    let base_br_id = create_test_board_run(&pool, bp_id, "completed").await;

    create_test_diff(&pool, br_id, Some(base_br_id), "failed").await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}/diff"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["board_run_id"], format!("br_{br_id}"));
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error_message"], "diff computation failed");
    assert!(json["created_at"].is_string());
}

/// 正常系: diff status=unavailable
#[tokio::test]
#[serial]
async fn test_get_board_run_diff_unavailable() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;
    let base_br_id = create_test_board_run(&pool, bp_id, "completed").await;

    create_test_diff(&pool, br_id, Some(base_br_id), "unavailable").await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}/diff"))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["board_run_id"], format!("br_{br_id}"));
    assert_eq!(json["status"], "unavailable");
    assert!(json["error_message"].is_null());
    assert!(json["created_at"].is_string());
}

// ─── Findings List Tests ─────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn create_test_run_check_finding(
    pool: &PgPool,
    run_check_id: Uuid,
    severity: &str,
    sort_index: i32,
    rule_code: Option<&str>,
    title: Option<&str>,
    x_um: Option<i32>,
    y_um: Option<i32>,
) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO run_check_findings (id, run_check_id, severity, rule_code, title, message, \
         subject_kind, subject_ref, sheet_path, pcb_layer, x_um, y_um, sort_index, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, NOW())",
    )
    .bind(id)
    .bind(run_check_id)
    .bind(severity)
    .bind(rule_code)
    .bind(title)
    .bind(title.map(|t| format!("{t} detail message")))
    .bind("schematic")
    .bind(Some("U1"))
    .bind(Some("/"))
    .bind(None::<&str>)
    .bind(x_um)
    .bind(y_um)
    .bind(sort_index)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// 正常系: findings一覧を取得できる
#[tokio::test]
#[serial]
async fn test_list_findings_success() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;
    let rc_id = create_test_run_check(&pool, br_id, "erc", "failed", 2, 1).await;

    create_test_run_check_finding(
        &pool,
        rc_id,
        "error",
        0,
        Some("ERC001"),
        Some("Pin not driven"),
        Some(5715),
        Some(2667),
    )
    .await;
    create_test_run_check_finding(
        &pool,
        rc_id,
        "error",
        1,
        Some("ERC002"),
        Some("Missing connection"),
        Some(1000),
        Some(2000),
    )
    .await;
    create_test_run_check_finding(
        &pool,
        rc_id,
        "warning",
        2,
        Some("ERC003"),
        Some("Unused pin"),
        None,
        None,
    )
    .await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-runs/br_{br_id}/checks/erc/findings"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["severity"], "error");
    assert_eq!(items[0]["rule_code"], "ERC001");
    assert_eq!(items[0]["title"], "Pin not driven");
    assert_eq!(items[0]["subject_kind"], "schematic");
    assert_eq!(items[0]["subject_ref"], "U1");
    // pos_mm conversion: 5715/1000 = 5.715, 2667/1000 = 2.667
    assert_eq!(items[0]["pos_mm"]["x"], 5.715);
    assert_eq!(items[0]["pos_mm"]["y"], 2.667);
    // Third item has no position
    assert!(items[2]["pos_mm"].is_null());
    assert_eq!(json["has_more"], false);
    assert!(json["next_cursor"].is_null());
}

/// 正常系: 空リスト (findingsなし)
#[tokio::test]
#[serial]
async fn test_list_findings_empty() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;
    // run_check exists but no findings
    create_test_run_check(&pool, br_id, "drc", "passed", 0, 0).await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-runs/br_{br_id}/checks/drc/findings"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["items"].as_array().unwrap().len(), 0);
    assert_eq!(json["has_more"], false);
}

/// 正常系: run_checkが存在しない場合も空リスト (404ではない)
#[tokio::test]
#[serial]
async fn test_list_findings_no_run_check_returns_empty() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;
    // No run_check created for this board_run

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-runs/br_{br_id}/checks/erc/findings"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["items"].as_array().unwrap().len(), 0);
    assert_eq!(json["has_more"], false);
}

/// 正常系: severity filter
#[tokio::test]
#[serial]
async fn test_list_findings_severity_filter() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;
    let rc_id = create_test_run_check(&pool, br_id, "erc", "failed", 1, 1).await;

    create_test_run_check_finding(
        &pool,
        rc_id,
        "error",
        0,
        Some("ERC001"),
        Some("Error finding"),
        None,
        None,
    )
    .await;
    create_test_run_check_finding(
        &pool,
        rc_id,
        "warning",
        1,
        Some("ERC002"),
        Some("Warning finding"),
        None,
        None,
    )
    .await;
    create_test_run_check_finding(
        &pool,
        rc_id,
        "notice",
        2,
        Some("ERC003"),
        Some("Notice finding"),
        None,
        None,
    )
    .await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-runs/br_{br_id}/checks/erc/findings?severity=error"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["severity"], "error");
    assert_eq!(items[0]["rule_code"], "ERC001");
}

/// 正常系: cursor pagination
#[tokio::test]
#[serial]
async fn test_list_findings_pagination() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;
    let rc_id = create_test_run_check(&pool, br_id, "drc", "failed", 3, 0).await;

    // Create 3 findings
    create_test_run_check_finding(
        &pool,
        rc_id,
        "error",
        0,
        Some("DRC001"),
        Some("Finding 1"),
        None,
        None,
    )
    .await;
    create_test_run_check_finding(
        &pool,
        rc_id,
        "error",
        1,
        Some("DRC002"),
        Some("Finding 2"),
        None,
        None,
    )
    .await;
    create_test_run_check_finding(
        &pool,
        rc_id,
        "error",
        2,
        Some("DRC003"),
        Some("Finding 3"),
        None,
        None,
    )
    .await;

    let app = create_test_app(pool.clone());

    // First page: limit=2
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-runs/br_{br_id}/checks/drc/findings?limit=2"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(json["has_more"], true);
    let next_cursor = json["next_cursor"].as_str().unwrap();

    // Second page using cursor
    let app2 = create_test_app(pool);
    let response2 = app2
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-runs/br_{br_id}/checks/drc/findings?limit=2&cursor={next_cursor}"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response2.status(), StatusCode::OK);
    let body_bytes2 = response2.into_body().collect().await.unwrap().to_bytes();
    let json2: serde_json::Value = serde_json::from_slice(&body_bytes2).unwrap();
    let items2 = json2["items"].as_array().unwrap();
    assert_eq!(items2.len(), 1);
    assert_eq!(json2["has_more"], false);
    assert!(json2["next_cursor"].is_null());
}

/// バリデーション: 不正なcheck_kindで400
#[tokio::test]
#[serial]
async fn test_list_findings_invalid_check_kind() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-runs/br_{br_id}/checks/invalid/findings"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"]["code"], "validation_failed");
}

/// バリデーション: 不正なboard_run_idフォーマットで400
#[tokio::test]
#[serial]
async fn test_list_findings_invalid_board_run_id() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/board-runs/invalid-id/checks/erc/findings")
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"]["code"], "validation_failed");
}

/// バリデーション: 不正なseverityフィルタで400
#[tokio::test]
#[serial]
async fn test_list_findings_invalid_severity() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-runs/br_{br_id}/checks/erc/findings?severity=critical"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"]["code"], "validation_failed");
}

/// 認証系: 未認証で401
#[tokio::test]
#[serial]
async fn test_list_findings_unauthenticated() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let _session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-runs/br_{br_id}/checks/erc/findings"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// 認可系: アクセス拒否で404
#[tokio::test]
#[serial]
async fn test_list_findings_access_denied() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    let app = create_deny_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-runs/br_{br_id}/checks/erc/findings"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// エラー系: 存在しないboard_runで404
#[tokio::test]
#[serial]
async fn test_list_findings_board_run_not_found() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let fake_id = Uuid::now_v7();

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-runs/br_{fake_id}/checks/erc/findings"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// バリデーション: 不正なcursorで400
#[tokio::test]
#[serial]
async fn test_list_findings_invalid_cursor() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    let app = create_test_app(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-runs/br_{br_id}/checks/erc/findings?cursor=not-valid-base64!!!"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"]["code"], "validation_failed");
}

/// 正常系: sort_index同値時のid tie-breakerテスト
/// 2件のfindingsが同一sort_indexを持つ場合、idでtie-breakされて正しくpaginateされること
#[tokio::test]
#[serial]
async fn test_list_findings_sort_index_tie_breaker() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let user_id = create_test_user(&pool).await;
    let session_id = create_test_session(&pool, user_id).await;
    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;
    let rc_id = create_test_run_check(&pool, br_id, "erc", "failed", 2, 0).await;

    // Create 2 findings with the SAME sort_index (0) — id will be the tie-breaker
    let id_a = create_test_run_check_finding(
        &pool,
        rc_id,
        "error",
        0,
        Some("ERC001"),
        Some("First"),
        None,
        None,
    )
    .await;
    let id_b = create_test_run_check_finding(
        &pool,
        rc_id,
        "error",
        0,
        Some("ERC002"),
        Some("Second"),
        None,
        None,
    )
    .await;

    let app = create_test_app(pool.clone());

    // Page 1: limit=1
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-runs/br_{br_id}/checks/erc/findings?limit=1"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(json["has_more"], true);
    let next_cursor = json["next_cursor"].as_str().unwrap();
    let first_id = items[0]["id"].as_str().unwrap().to_string();

    // Page 2: using cursor from page 1
    let app2 = create_test_app(pool);
    let response2 = app2
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-runs/br_{br_id}/checks/erc/findings?limit=1&cursor={next_cursor}"
                ))
                .header("cookie", session_cookie(session_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response2.status(), StatusCode::OK);
    let body_bytes2 = response2.into_body().collect().await.unwrap().to_bytes();
    let json2: serde_json::Value = serde_json::from_slice(&body_bytes2).unwrap();
    let items2 = json2["items"].as_array().unwrap();
    assert_eq!(items2.len(), 1);
    assert_eq!(json2["has_more"], false);
    let second_id = items2[0]["id"].as_str().unwrap().to_string();

    // The two pages must return different findings (both items returned, no duplicates)
    assert_ne!(first_id, second_id);

    // Both ids must be from our created findings
    let expected_ids: std::collections::HashSet<String> =
        [id_a.to_string(), id_b.to_string()].into_iter().collect();
    let actual_ids: std::collections::HashSet<String> = [first_id, second_id].into_iter().collect();
    assert_eq!(expected_ids, actual_ids);
}
