use axum::body::Body;
use axum::http::{Request, StatusCode};
use hmac::{Hmac, Mac};
use serial_test::serial;
use sha2::Sha256;
use sqlx::PgPool;
use tower::ServiceExt;

type HmacSha256 = Hmac<Sha256>;

fn compute_signature(secret: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    let result = mac.finalize().into_bytes();
    format!("sha256={}", hex::encode(result))
}

const WEBHOOK_SECRET: &str = "test-webhook-secret";

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
    boardflow_api::create_app_with_config(
        pool,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(WEBHOOK_SECRET.to_string()),
        None,
    )
}

fn rand_i64() -> i64 {
    use rand::Rng;
    rand::thread_rng().gen_range(1..i64::MAX)
}

// --- Test: ping event returns 200 ---

#[tokio::test]
#[serial]
async fn test_webhook_ping() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let app = create_test_app(pool);
    let body = br#"{"zen":"Keep it logically awesome."}"#;
    let signature = compute_signature(WEBHOOK_SECRET, body);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/github/webhook")
                .header("Content-Type", "application/json")
                .header("X-GitHub-Event", "ping")
                .header("X-Hub-Signature-256", &signature)
                .body(Body::from(body.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

// --- Test: invalid signature returns 401 ---

#[tokio::test]
#[serial]
async fn test_webhook_invalid_signature() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let app = create_test_app(pool);
    let body = br#"{"zen":"test"}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/github/webhook")
                .header("Content-Type", "application/json")
                .header("X-GitHub-Event", "ping")
                .header(
                    "X-Hub-Signature-256",
                    "sha256=0000000000000000000000000000000000000000000000000000000000000000",
                )
                .body(Body::from(body.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// --- Test: missing signature returns 401 ---

#[tokio::test]
#[serial]
async fn test_webhook_missing_signature() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let app = create_test_app(pool);
    let body = br#"{"zen":"test"}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/github/webhook")
                .header("Content-Type", "application/json")
                .header("X-GitHub-Event", "ping")
                .body(Body::from(body.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// --- Test: installation created upserts repositories ---

#[tokio::test]
#[serial]
async fn test_webhook_installation_created() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let installation_id = rand_i64();
    let repo_id_1 = rand_i64();
    let repo_id_2 = rand_i64();

    let body = serde_json::json!({
        "action": "created",
        "installation": { "id": installation_id },
        "repositories": [
            { "id": repo_id_1, "name": "repo-alpha", "full_name": "test-org/repo-alpha" },
            { "id": repo_id_2, "name": "repo-beta", "full_name": "test-org/repo-beta" }
        ]
    });
    let body_bytes = serde_json::to_vec(&body).unwrap();
    let signature = compute_signature(WEBHOOK_SECRET, &body_bytes);

    let app = create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/github/webhook")
                .header("Content-Type", "application/json")
                .header("X-GitHub-Event", "installation")
                .header("X-Hub-Signature-256", &signature)
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify repos were created
    let r1 = boardflow_db::queries::repository::find_by_github_id(&pool, repo_id_1)
        .await
        .unwrap();
    assert!(r1.is_some(), "repo-alpha should exist");
    let r1 = r1.unwrap();
    assert_eq!(r1.owner, "test-org");
    assert_eq!(r1.name, "repo-alpha");
    assert_eq!(r1.installation_id, installation_id);

    let r2 = boardflow_db::queries::repository::find_by_github_id(&pool, repo_id_2)
        .await
        .unwrap();
    assert!(r2.is_some(), "repo-beta should exist");
    let r2 = r2.unwrap();
    assert_eq!(r2.owner, "test-org");
    assert_eq!(r2.name, "repo-beta");
    assert_eq!(r2.installation_id, installation_id);
}

// --- Test: installation deleted clears installation_id ---

#[tokio::test]
#[serial]
async fn test_webhook_installation_deleted() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let installation_id = rand_i64();
    let repo_id = rand_i64();

    // Pre-insert a repo with this installation_id
    boardflow_db::queries::repository::upsert(
        &pool,
        repo_id,
        "del-org",
        "del-repo",
        installation_id,
    )
    .await
    .unwrap();

    let body = serde_json::json!({
        "action": "deleted",
        "installation": { "id": installation_id }
    });
    let body_bytes = serde_json::to_vec(&body).unwrap();
    let signature = compute_signature(WEBHOOK_SECRET, &body_bytes);

    let app = create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/github/webhook")
                .header("Content-Type", "application/json")
                .header("X-GitHub-Event", "installation")
                .header("X-Hub-Signature-256", &signature)
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify installation_id was cleared to 0
    let repo = boardflow_db::queries::repository::find_by_github_id(&pool, repo_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        repo.installation_id, 0,
        "installation_id should be cleared to 0"
    );
}

// --- Test: installation_repositories added upserts repos ---

#[tokio::test]
#[serial]
async fn test_webhook_repos_added() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let installation_id = rand_i64();
    let repo_id = rand_i64();

    let body = serde_json::json!({
        "action": "added",
        "installation": { "id": installation_id },
        "repositories_added": [
            { "id": repo_id, "name": "new-repo", "full_name": "add-org/new-repo" }
        ],
        "repositories_removed": []
    });
    let body_bytes = serde_json::to_vec(&body).unwrap();
    let signature = compute_signature(WEBHOOK_SECRET, &body_bytes);

    let app = create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/github/webhook")
                .header("Content-Type", "application/json")
                .header("X-GitHub-Event", "installation_repositories")
                .header("X-Hub-Signature-256", &signature)
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let repo = boardflow_db::queries::repository::find_by_github_id(&pool, repo_id)
        .await
        .unwrap();
    assert!(repo.is_some(), "new-repo should exist");
    let repo = repo.unwrap();
    assert_eq!(repo.owner, "add-org");
    assert_eq!(repo.name, "new-repo");
    assert_eq!(repo.installation_id, installation_id);
}

// --- Test: installation_repositories removed clears installation_id ---

#[tokio::test]
#[serial]
async fn test_webhook_repos_removed() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let installation_id = rand_i64();
    let repo_id = rand_i64();

    // Pre-insert the repo
    boardflow_db::queries::repository::upsert(&pool, repo_id, "rm-org", "rm-repo", installation_id)
        .await
        .unwrap();

    let body = serde_json::json!({
        "action": "removed",
        "installation": { "id": installation_id },
        "repositories_added": [],
        "repositories_removed": [
            { "id": repo_id, "name": "rm-repo", "full_name": "rm-org/rm-repo" }
        ]
    });
    let body_bytes = serde_json::to_vec(&body).unwrap();
    let signature = compute_signature(WEBHOOK_SECRET, &body_bytes);

    let app = create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/github/webhook")
                .header("Content-Type", "application/json")
                .header("X-GitHub-Event", "installation_repositories")
                .header("X-Hub-Signature-256", &signature)
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let repo = boardflow_db::queries::repository::find_by_github_id(&pool, repo_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        repo.installation_id, 0,
        "installation_id should be cleared to 0"
    );
}

// --- Test: webhook secret not configured returns 500 ---

#[tokio::test]
#[serial]
async fn test_webhook_no_secret_configured() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    // webhook_secret を None で作成
    let app = boardflow_api::create_app_with_config(
        pool, None, None, None, None, None, None, None, None, None, None,
    );
    let body = br#"{"zen":"test"}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/github/webhook")
                .header("Content-Type", "application/json")
                .header("X-GitHub-Event", "ping")
                .header("X-Hub-Signature-256", "sha256=dummy")
                .body(Body::from(body.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// --- Test: removed event from different installation does not clear ---

#[tokio::test]
#[serial]
async fn test_webhook_repos_removed_different_installation() {
    let Some(pool) = setup_pool().await else {
        return;
    };

    let current_installation_id = rand_i64();
    let different_installation_id = rand_i64();
    let repo_id = rand_i64();

    // repo を current_installation_id で登録
    boardflow_db::queries::repository::upsert(
        &pool,
        repo_id,
        "kept-org",
        "kept-repo",
        current_installation_id,
    )
    .await
    .unwrap();

    // different_installation_id からの removed event
    let body = serde_json::json!({
        "action": "removed",
        "installation": { "id": different_installation_id },
        "repositories_added": [],
        "repositories_removed": [
            { "id": repo_id, "name": "kept-repo", "full_name": "kept-org/kept-repo" }
        ]
    });
    let body_bytes = serde_json::to_vec(&body).unwrap();
    let signature = compute_signature(WEBHOOK_SECRET, &body_bytes);

    let app = create_test_app(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/github/webhook")
                .header("Content-Type", "application/json")
                .header("X-GitHub-Event", "installation_repositories")
                .header("X-Hub-Signature-256", &signature)
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // installation_id は変更されていないことを確認
    let repo = boardflow_db::queries::repository::find_by_github_id(&pool, repo_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        repo.installation_id, current_installation_id,
        "installation_id should NOT be cleared by different installation's removal"
    );
}

// --- Test: unknown event returns 200 ---

#[tokio::test]
#[serial]
async fn test_webhook_unknown_event() {
    let Some(pool) = setup_pool().await else {
        return;
    };
    let app = create_test_app(pool);
    let body = br#"{"action":"something"}"#;
    let signature = compute_signature(WEBHOOK_SECRET, body);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/github/webhook")
                .header("Content-Type", "application/json")
                .header("X-GitHub-Event", "push")
                .header("X-Hub-Signature-256", &signature)
                .body(Body::from(body.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
