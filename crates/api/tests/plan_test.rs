use axum::body::Body;
use axum::http::{Request, StatusCode};
use boardflow_api::create_app;
use http_body_util::BodyExt;
use serial_test::serial;
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

async fn create_test_repository(
    pool: &PgPool,
    github_repository_id: i64,
    installation_id: i64,
) -> Uuid {
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

/// Create a pre-existing BoardProject with the given latest_tree_hash.
/// Uses a past created_at/updated_at to ensure is_new detection (created_at != updated_at) works.
async fn create_existing_board_project(
    pool: &PgPool,
    repository_id: Uuid,
    project_path: &str,
    latest_tree_hash: Option<&str>,
) -> Uuid {
    let id = Uuid::now_v7();
    let actual_id: Uuid = sqlx::query_scalar(
        "INSERT INTO board_projects (id, repository_id, project_path, project_dir, display_name, \
         issue_sync_status, recreate_issue_on_update, latest_tree_hash, \
         created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, 'pending', true, $6, \
         NOW() - INTERVAL '1 hour', NOW()) \
         ON CONFLICT (repository_id, project_path) DO UPDATE SET \
         latest_tree_hash = EXCLUDED.latest_tree_hash, \
         updated_at = NOW() \
         RETURNING id",
    )
    .bind(id)
    .bind(repository_id)
    .bind(project_path)
    .bind("hardware")
    .bind("LightStick")
    .bind(latest_tree_hash)
    .fetch_one(pool)
    .await
    .unwrap();
    actual_id
}

fn plan_request_body(github_repository_id: &str) -> serde_json::Value {
    serde_json::json!({
        "repository": {
            "github_repository_id": github_repository_id,
            "owner": "test-owner",
            "name": "test-repo"
        },
        "git": {
            "ref": "refs/heads/main",
            "branch": "main",
            "commit_sha": "abc123def456",
            "event_name": "push"
        },
        "action": {
            "workflow": "boardflow.yml",
            "run_id": "12345",
            "run_attempt": "1"
        },
        "mode": "auto",
        "projects": [{
            "project_path": "hardware/LightStick.kicad_pro",
            "config_path": "hardware/boardflow.yml",
            "project_dir": "hardware",
            "tree_hash": "deadbeef1234567890",
            "files": [{
                "path": "hardware/LightStick.kicad_pcb",
                "sha256": "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
            }]
        }]
    })
}

/// 正常系: 新規プロジェクト → build / new_project
#[tokio::test]
#[serial]
async fn plan_new_project_returns_build_new_project() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id: i64 = rand_i64();
    let installation_id: i64 = 1001;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    let app = create_app(pool, None);
    let body = plan_request_body(&github_repo_id.to_string());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/plan")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        json["repository"]["github_repository_id"],
        github_repo_id.to_string()
    );
    assert_eq!(json["repository"]["owner"], "test-owner");
    assert_eq!(json["repository"]["name"], "test-repo");
    assert_eq!(json["projects"][0]["decision"], "build");
    assert_eq!(json["projects"][0]["reason"], "new_project");
    assert!(
        json["projects"][0]["board_project_id"]
            .as_str()
            .unwrap()
            .starts_with("bp_")
    );
}

/// 正常系: mode=all → build / manual_dispatch
#[tokio::test]
#[serial]
async fn plan_mode_all_returns_build_manual_dispatch() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id: i64 = rand_i64();
    let installation_id: i64 = 1002;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    let app = create_app(pool, None);
    let mut body = plan_request_body(&github_repo_id.to_string());
    body["mode"] = serde_json::json!("all");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/plan")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["projects"][0]["decision"], "build");
    assert_eq!(json["projects"][0]["reason"], "manual_dispatch");
}

/// 異常系: 認証なし → 401
#[tokio::test]
#[serial]
async fn plan_without_auth_returns_401() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = create_app(pool, None);
    let body = plan_request_body("12345");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/plan")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// 異常系: 認可失敗(別repository) → 403
#[tokio::test]
#[serial]
async fn plan_wrong_repository_returns_403() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id: i64 = rand_i64();
    let installation_id: i64 = 1003;

    // Create a repo with a DIFFERENT github_repository_id and a token for it
    let different_github_repo_id = github_repo_id + 9999;
    let different_repo_id =
        create_test_repository(&pool, different_github_repo_id, installation_id).await;
    let token = create_test_token(&pool, different_repo_id, installation_id).await;

    // The request targets github_repo_id, but the token belongs to different_github_repo_id → 403
    let app = create_app(pool, None);
    let body = plan_request_body(&github_repo_id.to_string());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/plan")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"]["code"], "forbidden");
}

/// 異常系: 不正な github_repository_id → 400
#[tokio::test]
#[serial]
async fn plan_invalid_github_repository_id_returns_400() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id: i64 = rand_i64();
    let installation_id: i64 = 1004;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    let app = create_app(pool, None);
    let body = plan_request_body("not_a_number");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/plan")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"]["code"], "validation_failed");
}

/// 異常系: JSONパースエラー → 400 with ErrorResponse body
#[tokio::test]
#[serial]
async fn plan_invalid_json_returns_400() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id: i64 = rand_i64();
    let installation_id: i64 = 1005;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    let app = create_app(pool, None);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/plan")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::from(b"not valid json".to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["error"]["code"], "validation_failed");
    assert!(json["error"]["request_id"].as_str().is_some());
    assert!(!json["error"]["request_id"].as_str().unwrap().is_empty());
}

/// 異常系: 重複project_path → decision: error
#[tokio::test]
#[serial]
async fn plan_duplicate_project_path_returns_error() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id: i64 = rand_i64();
    let installation_id: i64 = 1006;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "repository": {
            "github_repository_id": github_repo_id.to_string(),
            "owner": "test-owner",
            "name": "test-repo"
        },
        "git": {
            "ref": "refs/heads/main",
            "branch": "main",
            "commit_sha": "abc123def456",
            "event_name": "push"
        },
        "action": {
            "workflow": "boardflow.yml",
            "run_id": "12345",
            "run_attempt": "1"
        },
        "mode": "auto",
        "projects": [
            {
                "project_path": "hardware/LightStick.kicad_pro",
                "config_path": "hardware/boardflow.yml",
                "project_dir": "hardware",
                "tree_hash": "deadbeef1234567890",
                "files": []
            },
            {
                "project_path": "hardware/LightStick.kicad_pro",
                "config_path": "hardware/boardflow.yml",
                "project_dir": "hardware",
                "tree_hash": "different_hash",
                "files": []
            }
        ]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/plan")
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

    assert_eq!(json["projects"][0]["decision"], "error");
    assert_eq!(json["projects"][0]["reason"], "duplicate_project_path");
    assert_eq!(json["projects"][1]["decision"], "error");
    assert_eq!(json["projects"][1]["reason"], "duplicate_project_path");
}

/// 異常系: 空のproject_path → decision: error / invalid_project_path
#[tokio::test]
#[serial]
async fn plan_empty_project_path_returns_error() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id: i64 = rand_i64();
    let installation_id: i64 = 1007;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "repository": {
            "github_repository_id": github_repo_id.to_string(),
            "owner": "test-owner",
            "name": "test-repo"
        },
        "git": {
            "ref": "refs/heads/main",
            "branch": "main",
            "commit_sha": "abc123def456",
            "event_name": "push"
        },
        "action": {
            "workflow": "boardflow.yml",
            "run_id": "12345",
            "run_attempt": "1"
        },
        "mode": "auto",
        "projects": [{
            "project_path": "",
            "config_path": "boardflow.yml",
            "project_dir": ".",
            "tree_hash": "deadbeef1234567890",
            "files": []
        }]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/plan")
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

    assert_eq!(json["projects"][0]["decision"], "error");
    assert_eq!(json["projects"][0]["reason"], "invalid_project_path");
}

/// 異常系: project_pathが.kicad_proで終わらない → decision: error / invalid_project_path
#[tokio::test]
#[serial]
async fn plan_invalid_project_path_extension_returns_error() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id: i64 = rand_i64();
    let installation_id: i64 = 1013;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "repository": {
            "github_repository_id": github_repo_id.to_string(),
            "owner": "test-owner",
            "name": "test-repo"
        },
        "git": {
            "ref": "refs/heads/main",
            "branch": "main",
            "commit_sha": "abc123def456",
            "event_name": "push"
        },
        "action": {
            "workflow": "boardflow.yml",
            "run_id": "12345",
            "run_attempt": "1"
        },
        "mode": "auto",
        "projects": [{
            "project_path": "hardware/board.txt",
            "config_path": "hardware/boardflow.yml",
            "project_dir": "hardware",
            "tree_hash": "deadbeef1234567890",
            "files": []
        }]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/plan")
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

    assert_eq!(json["projects"][0]["decision"], "error");
    assert_eq!(json["projects"][0]["reason"], "invalid_project_path");
}

/// 異常系: project_pathにパストラバーサル → decision: error / invalid_project_path
#[tokio::test]
#[serial]
async fn plan_path_traversal_project_path_returns_error() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id: i64 = rand_i64();
    let installation_id: i64 = 1014;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "repository": {
            "github_repository_id": github_repo_id.to_string(),
            "owner": "test-owner",
            "name": "test-repo"
        },
        "git": {
            "ref": "refs/heads/main",
            "branch": "main",
            "commit_sha": "abc123def456",
            "event_name": "push"
        },
        "action": {
            "workflow": "boardflow.yml",
            "run_id": "12345",
            "run_attempt": "1"
        },
        "mode": "auto",
        "projects": [{
            "project_path": "../../../etc/passwd.kicad_pro",
            "config_path": "hardware/boardflow.yml",
            "project_dir": "hardware",
            "tree_hash": "deadbeef1234567890",
            "files": []
        }]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/plan")
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

    assert_eq!(json["projects"][0]["decision"], "error");
    assert_eq!(json["projects"][0]["reason"], "invalid_project_path");
}

/// 異常系: config_pathにパストラバーサル → decision: error / invalid_config_path
#[tokio::test]
#[serial]
async fn plan_path_traversal_config_path_returns_error() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id: i64 = rand_i64();
    let installation_id: i64 = 1015;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "repository": {
            "github_repository_id": github_repo_id.to_string(),
            "owner": "test-owner",
            "name": "test-repo"
        },
        "git": {
            "ref": "refs/heads/main",
            "branch": "main",
            "commit_sha": "abc123def456",
            "event_name": "push"
        },
        "action": {
            "workflow": "boardflow.yml",
            "run_id": "12345",
            "run_attempt": "1"
        },
        "mode": "auto",
        "projects": [{
            "project_path": "hardware/LightStick.kicad_pro",
            "config_path": "../../etc/config.yml",
            "project_dir": "hardware",
            "tree_hash": "deadbeef1234567890",
            "files": []
        }]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/plan")
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

    assert_eq!(json["projects"][0]["decision"], "error");
    assert_eq!(json["projects"][0]["reason"], "invalid_config_path");
}

/// 異常系: 空のtree_hash → decision: error / invalid_tree_hash
#[tokio::test]
#[serial]
async fn plan_empty_tree_hash_returns_error() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id: i64 = rand_i64();
    let installation_id: i64 = 1008;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "repository": {
            "github_repository_id": github_repo_id.to_string(),
            "owner": "test-owner",
            "name": "test-repo"
        },
        "git": {
            "ref": "refs/heads/main",
            "branch": "main",
            "commit_sha": "abc123def456",
            "event_name": "push"
        },
        "action": {
            "workflow": "boardflow.yml",
            "run_id": "12345",
            "run_attempt": "1"
        },
        "mode": "auto",
        "projects": [{
            "project_path": "hardware/LightStick.kicad_pro",
            "config_path": "hardware/boardflow.yml",
            "project_dir": "hardware",
            "tree_hash": "",
            "files": []
        }]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/plan")
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

    assert_eq!(json["projects"][0]["decision"], "error");
    assert_eq!(json["projects"][0]["reason"], "invalid_tree_hash");
}

/// 異常系: 空のconfig_path → decision: error / invalid_config_path
#[tokio::test]
#[serial]
async fn plan_empty_config_path_returns_error() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id: i64 = rand_i64();
    let installation_id: i64 = 1009;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    let app = create_app(pool, None);
    let body = serde_json::json!({
        "repository": {
            "github_repository_id": github_repo_id.to_string(),
            "owner": "test-owner",
            "name": "test-repo"
        },
        "git": {
            "ref": "refs/heads/main",
            "branch": "main",
            "commit_sha": "abc123def456",
            "event_name": "push"
        },
        "action": {
            "workflow": "boardflow.yml",
            "run_id": "12345",
            "run_attempt": "1"
        },
        "mode": "auto",
        "projects": [{
            "project_path": "hardware/LightStick.kicad_pro",
            "config_path": "",
            "project_dir": "hardware",
            "tree_hash": "deadbeef1234567890",
            "files": []
        }]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/plan")
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

    assert_eq!(json["projects"][0]["decision"], "error");
    assert_eq!(json["projects"][0]["reason"], "invalid_config_path");
}

/// 正常系: 既存プロジェクトでtree_hash変更 → build / hash_changed
#[tokio::test]
#[serial]
async fn plan_existing_project_hash_changed() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id: i64 = rand_i64();
    let installation_id: i64 = 1010;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    let project_path = "hardware/LightStick.kicad_pro";
    create_existing_board_project(&pool, repo_id, project_path, Some("old_hash_value")).await;

    let app = create_app(pool, None);
    let mut body = plan_request_body(&github_repo_id.to_string());
    body["projects"][0]["tree_hash"] = serde_json::json!("new_different_hash");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/plan")
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

    assert_eq!(json["projects"][0]["decision"], "build");
    assert_eq!(json["projects"][0]["reason"], "hash_changed");
}

/// 正常系: 既存プロジェクトでtree_hash一致 → skip / unchanged
#[tokio::test]
#[serial]
async fn plan_existing_project_unchanged() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id: i64 = rand_i64();
    let installation_id: i64 = 1011;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    let project_path = "hardware/LightStick.kicad_pro";
    let tree_hash = "deadbeef1234567890";
    create_existing_board_project(&pool, repo_id, project_path, Some(tree_hash)).await;

    let app = create_app(pool, None);
    let body = plan_request_body(&github_repo_id.to_string());
    // body's tree_hash is "deadbeef1234567890" by default, matching the stored value

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/plan")
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

    assert_eq!(json["projects"][0]["decision"], "skip");
    assert_eq!(json["projects"][0]["reason"], "unchanged");
}

/// 正常系: 既存プロジェクトでlatest_tree_hash=NULL → build / no_previous_snapshot
#[tokio::test]
#[serial]
async fn plan_existing_project_no_previous_snapshot() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id: i64 = rand_i64();
    let installation_id: i64 = 1012;
    let repo_id = create_test_repository(&pool, github_repo_id, installation_id).await;
    let token = create_test_token(&pool, repo_id, installation_id).await;

    let project_path = "hardware/LightStick.kicad_pro";
    create_existing_board_project(&pool, repo_id, project_path, None).await;

    let app = create_app(pool, None);
    let body = plan_request_body(&github_repo_id.to_string());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/runs/plan")
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

    assert_eq!(json["projects"][0]["decision"], "build");
    assert_eq!(json["projects"][0]["reason"], "no_previous_snapshot");
}

fn rand_i64() -> i64 {
    // Use UUID timestamp bits for a unique i64 to avoid test collisions
    let uuid = Uuid::now_v7();
    let bytes = uuid.as_bytes();
    i64::from_be_bytes(bytes[0..8].try_into().unwrap()).abs()
}
