use boardflow_domain::public_ids::BoardRunId;
use std::time::Duration;
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[path = "../src/api.rs"]
mod api;
#[path = "../src/error.rs"]
mod error;

use api::ApiClient;
use boardflow_api_types::board_run::CreateBoardRunStatus;

fn sample_board_project_id() -> &'static str {
    "bp_123e4567-e89b-12d3-a456-426614174000"
}

fn sample_board_run_id() -> BoardRunId {
    BoardRunId::from(Uuid::parse_str("123e4567-e89b-12d3-a456-426614174111").unwrap())
}

#[tokio::test]
async fn test_plan_api_success() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/runs/plan"))
        .and(header("Authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "projects": [
                {"project_path": "board/board.kicad_pro", "decision": "build", "board_project_id": "bp_123e4567-e89b-12d3-a456-426614174000", "reason": "new_project"},
                {"project_path": "other/other.kicad_pro", "decision": "skip", "board_project_id": "", "reason": "unchanged"}
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
    assert!(matches!(
        decisions[0].decision,
        boardflow_api_types::plan::PlanDecision::Build
    ));
    assert_eq!(
        decisions[0].board_project_id.map(|id| id.to_string()),
        Some(sample_board_project_id().to_string())
    );
    assert!(matches!(
        decisions[1].decision,
        boardflow_api_types::plan::PlanDecision::Skip
    ));
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
            "projects": [{"project_path": "x.kicad_pro", "decision": "build", "board_project_id": "", "reason": "new_project"}]
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
            "board_run_id": "br_123e4567-e89b-12d3-a456-426614174111",
            "status": "created",
            "artifact_bundle": {
                "upload_mode": "staging_s3",
                "upload_url": "https://s3.example.com/upload",
                "object_key": "bundles/run-123.zip",
                "method": "PUT",
                "expires_at": "2026-01-01T00:00:00Z"
            }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri(), "tok");
    let payload = serde_json::json!({"board_project_id": sample_board_project_id()});
    let resp = client.create_board_run(&payload).await.unwrap();
    assert_eq!(resp.board_run_id, sample_board_run_id());
    assert_eq!(resp.status, CreateBoardRunStatus::Created);
    let bundle = resp.artifact_bundle.unwrap();
    assert_eq!(bundle.upload_url, "https://s3.example.com/upload");
    assert_eq!(bundle.object_key, "bundles/run-123.zip");
}

#[tokio::test]
async fn test_import_api() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(
            "/api/v1/board-runs/br_123e4567-e89b-12d3-a456-426614174111/artifact-bundles/import",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri(), "tok");
    let payload = serde_json::json!({"staging_object_key": "key", "bundle_sha256": "sha256:abc"});
    client
        .import(sample_board_run_id(), &payload)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_fail_api() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(
            "/api/v1/board-runs/br_123e4567-e89b-12d3-a456-426614174111/fail",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri(), "tok");
    client
        .fail(sample_board_run_id(), "error msg", "details here")
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
            serde_json::json!({"projects": [{"project_path": "x", "decision": "skip", "board_project_id": "", "reason": "unchanged"}]}),
        ))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri(), "tok");
    let payload = serde_json::json!({});
    let decisions = client.plan(&payload).await.unwrap();
    assert_eq!(decisions.len(), 1);
}

#[tokio::test]
async fn test_create_board_run_idempotent_no_bundle() {
    let mock_server = MockServer::start().await;

    // Simulate an already-completed board run: artifact_bundle is null
    Mock::given(method("POST"))
        .and(path("/api/v1/board-runs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "board_run_id": "br_123e4567-e89b-12d3-a456-426614174112",
            "status": "completed",
            "artifact_bundle": null
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(&mock_server.uri(), "tok");
    let payload = serde_json::json!({"board_project_id": sample_board_project_id()});
    let resp = client.create_board_run(&payload).await.unwrap();
    assert_eq!(
        resp.board_run_id,
        BoardRunId::from(Uuid::parse_str("123e4567-e89b-12d3-a456-426614174112").unwrap())
    );
    assert_eq!(resp.status, CreateBoardRunStatus::Completed);
    assert!(resp.artifact_bundle.is_none());
}
