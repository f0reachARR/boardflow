use axum::body::Body;
use axum::http::{Request, StatusCode};
use boardflow_api::create_app;
use http_body_util::BodyExt;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn setup_pool() -> Option<PgPool> {
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

fn rand_i64() -> i64 {
    let uuid = Uuid::now_v7();
    let bytes = uuid.as_bytes();
    i64::from_be_bytes(bytes[0..8].try_into().unwrap()).abs()
}

async fn create_test_repository(pool: &PgPool, github_repository_id: i64) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
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
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn create_test_board_project(pool: &PgPool, repository_id: Uuid) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
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
    .execute(pool)
    .await
    .unwrap();
    id
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

/// 正常系: リポジトリ一覧を取得できる
#[tokio::test]
async fn test_list_repositories_success() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    create_test_repository(&pool, github_repo_id).await;

    let app = create_app(pool, None);
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

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert!(json["items"].is_array());
    assert!(json.get("has_more").is_some());
}

/// 正常系: limit パラメータが機能する
#[tokio::test]
async fn test_list_repositories_with_limit() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    // 3件作成
    for _ in 0..3 {
        create_test_repository(&pool, rand_i64()).await;
    }

    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories?limit=2")
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
async fn test_list_repositories_limit_zero_clamped() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    create_test_repository(&pool, rand_i64()).await;

    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories?limit=0")
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
async fn test_list_repositories_invalid_cursor() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories?cursor=invalid_base64_cursor")
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
async fn test_get_repository_success() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    create_test_repository(&pool, github_repo_id).await;

    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/repositories/{github_repo_id}"))
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
async fn test_get_repository_not_found() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories/999999999")
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
async fn test_list_board_projects_success() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    create_test_board_project(&pool, repo_id).await;

    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/repositories/{github_repo_id}/board-projects"
                ))
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
    assert!(items[0]["board_project_id"].as_str().unwrap().starts_with("bp_"));
    assert!(items[0]["state"].is_string());
}

/// エラー系: 存在しないリポジトリのBoardProject一覧
#[tokio::test]
async fn test_list_board_projects_repo_not_found() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories/999999999/board-projects")
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
async fn test_get_board_project_success() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;

    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-projects/bp_{bp_id}"))
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
    assert_eq!(json["repository"]["github_repository_id"], github_repo_id.to_string());
    assert_eq!(json["display_name"], "TestProject");
    assert!(json["state"].is_string());
}

/// エラー系: 不正なBoardProject IDフォーマット
#[tokio::test]
async fn test_get_board_project_invalid_id() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/board-projects/invalid_id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// エラー系: 存在しないBoardProject
#[tokio::test]
async fn test_get_board_project_not_found() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let fake_id = Uuid::now_v7();
    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-projects/bp_{fake_id}"))
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
async fn test_list_board_runs_success() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    create_test_board_run(&pool, bp_id, "completed").await;

    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-projects/bp_{bp_id}/board-runs"))
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
    assert!(items[0]["board_run_id"].as_str().unwrap().starts_with("br_"));
    assert_eq!(items[0]["status"], "completed");
}

/// エラー系: 存在しないBoardProjectのBoardRun一覧
#[tokio::test]
async fn test_list_board_runs_bp_not_found() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let fake_id = Uuid::now_v7();
    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-projects/bp_{fake_id}/board-runs"))
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
async fn test_get_board_run_success() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

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

    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}"))
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
async fn test_get_board_run_invalid_id() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/board-runs/invalid_id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// エラー系: 存在しないBoardRun
#[tokio::test]
async fn test_get_board_run_not_found() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let fake_id = Uuid::now_v7();
    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{fake_id}"))
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
async fn test_list_artifacts_success() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    create_test_artifact(&pool, br_id, "schematic_pdf", "available").await;
    create_test_artifact(&pool, br_id, "drill_zip", "missing").await;

    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}/artifacts"))
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
    assert!(available["artifact_id"].as_str().unwrap().starts_with("art_"));
    assert!(available["filename"].is_string());
    assert!(available["size_bytes"].is_number());

    // Missing artifact should not have artifact_id
    let missing = items.iter().find(|i| i["status"] == "missing").unwrap();
    assert!(missing.get("artifact_id").is_none() || missing["artifact_id"].is_null());
}

/// エラー系: 存在しないBoardRunのArtifact一覧
#[tokio::test]
async fn test_list_artifacts_run_not_found() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let fake_id = Uuid::now_v7();
    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{fake_id}/artifacts"))
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
async fn test_get_viewer_sources_all_available() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

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

    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}/viewer-sources"))
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

    // Each source should have a URL with proxy path
    for src in kicanvas_sources {
        assert!(src["url"].as_str().unwrap().contains("/proxy/artifacts/"));
    }
}

/// 正常系: 一部のartifactが missing の場合は partial
#[tokio::test]
async fn test_get_viewer_sources_partial() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;

    // Only gerber available, drill missing
    create_test_artifact(&pool, br_id, "gerber_zip", "available").await;
    create_test_artifact(&pool, br_id, "drill_zip", "missing").await;

    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}/viewer-sources"))
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
async fn test_get_viewer_sources_missing() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let github_repo_id = rand_i64();
    let repo_id = create_test_repository(&pool, github_repo_id).await;
    let bp_id = create_test_board_project(&pool, repo_id).await;
    let br_id = create_test_board_run(&pool, bp_id, "completed").await;
    // No artifacts at all

    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{br_id}/viewer-sources"))
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

/// エラー系: 存在しないBoardRunのViewer Sources
#[tokio::test]
async fn test_get_viewer_sources_run_not_found() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

    let fake_id = Uuid::now_v7();
    let app = create_app(pool, None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/board-runs/br_{fake_id}/viewer-sources"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── Pagination Integration Tests ────────────────────────────────────────────

/// 統合: cursor pagination でページ遷移ができる
#[tokio::test]
async fn test_pagination_cursor_traversal() {
    let pool = match setup_pool().await {
        Some(p) => p,
        None => return,
    };

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
    let app = create_app(pool.clone(), None);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-projects/bp_{bp_id}/board-runs?limit=2"
                ))
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
    let app2 = create_app(pool, None);
    let response2 = app2
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/board-projects/bp_{bp_id}/board-runs?limit=2&cursor={next_cursor}"
                ))
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
