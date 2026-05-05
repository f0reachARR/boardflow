use std::time::Duration;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[path = "../src/api.rs"]
mod api;
#[path = "../src/error.rs"]
mod error;

use api::ApiClient;

#[tokio::test]
async fn test_plan_api_success() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/runs/plan"))
        .and(header("Authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "projects": [
                {"project_path": "board/board.kicad_pro", "decision": "build", "board_project_id": "proj-1"},
                {"project_path": "other/other.kicad_pro", "decision": "skip"}
            ]
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri(), "test-token");
    let payload = serde_json::json!({"test": true});
    let decisions = client.plan(&payload).await.unwrap();

    assert_eq!(decisions.len(), 2);
    assert_eq!(decisions[0].project_path, "board/board.kicad_pro");
    assert_eq!(decisions[0].decision, "build");
    assert_eq!(decisions[0].board_project_id.as_deref(), Some("proj-1"));
    assert_eq!(decisions[1].decision, "skip");
}

#[tokio::test]
async fn test_api_retries_on_5xx() {
    let mock_server = MockServer::start().await;

    // First 2 calls fail with 500, 3rd succeeds
    Mock::given(method("POST"))
        .and(path("/api/v1/runs/plan"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(2)
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/v1/runs/plan"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "projects": [{"project_path": "x.kicad_pro", "decision": "build"}]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri(), "tok");
    let payload = serde_json::json!({});
    let decisions = client.plan(&payload).await.unwrap();
    assert_eq!(decisions.len(), 1);
}

#[tokio::test]
async fn test_api_no_retry_on_4xx() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/runs/plan"))
        .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri(), "tok");
    let payload = serde_json::json!({});
    let result = client.plan(&payload).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("403"), "error should contain status: {err}");
}

#[tokio::test]
async fn test_api_fails_after_3_retries() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/runs/plan"))
        .respond_with(ResponseTemplate::new(502))
        .expect(3)
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri(), "tok");
    let payload = serde_json::json!({});
    let result = client.plan(&payload).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_create_board_run() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/board-runs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "board_run_id": "run-123",
            "artifact_bundle": {
                "upload_url": "https://s3.example.com/upload",
                "object_key": "bundles/run-123.zip"
            }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri(), "tok");
    let payload = serde_json::json!({"board_project_id": "proj-1"});
    let resp = client.create_board_run(&payload).await.unwrap();
    assert_eq!(resp.board_run_id, "run-123");
    assert_eq!(
        resp.artifact_bundle.upload_url,
        "https://s3.example.com/upload"
    );
    assert_eq!(resp.artifact_bundle.object_key, "bundles/run-123.zip");
}

#[tokio::test]
async fn test_import_api() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/board-runs/run-abc/artifact-bundles/import"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri(), "tok");
    let payload = serde_json::json!({"staging_object_key": "key", "bundle_sha256": "sha256:abc"});
    client.import("run-abc", &payload).await.unwrap();
}

#[tokio::test]
async fn test_fail_api() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/board-runs/run-xyz/fail"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri(), "tok");
    client
        .fail("run-xyz", "error msg", "details here")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore] // Takes 60+ seconds due to real HTTP timeout
async fn test_retries_on_timeout() {
    let mock_server = MockServer::start().await;

    // First request times out (delay > client timeout of 60s), second succeeds
    Mock::given(method("POST"))
        .and(path("/api/v1/runs/plan"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"projects": []}))
                .set_delay(Duration::from_secs(90)),
        )
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/v1/runs/plan"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"projects": [{"project_path": "x", "decision": "skip"}]}),
        ))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri(), "tok");
    let payload = serde_json::json!({});
    let decisions = client.plan(&payload).await.unwrap();
    assert_eq!(decisions.len(), 1);
}
