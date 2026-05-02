use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::get;
use http_body_util::BodyExt;
use tower::ServiceExt;

use boardflow_api::error::{AppError, ErrorCode, ErrorResponse, RequestId};
use boardflow_api::middleware::request_id::request_id_middleware;

#[tokio::test]
async fn request_id_header_is_present() {
    let app = Router::new()
        .route("/ping", get(|| async { "pong" }))
        .layer(middleware::from_fn(request_id_middleware));

    let response = app
        .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("x-request-id"));
    let request_id = response
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(!request_id.is_empty());
}

#[tokio::test]
async fn request_id_is_uuid_v7_format() {
    let app = Router::new()
        .route("/ping", get(|| async { "pong" }))
        .layer(middleware::from_fn(request_id_middleware));

    let response = app
        .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let request_id = response
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap();

    // Format: req_ + UUIDv7 (8-4-4-4-12 hex chars)
    assert!(request_id.starts_with("req_"));
    let uuid_part = &request_id[4..];
    assert_eq!(uuid_part.len(), 36);
    assert!(uuid::Uuid::parse_str(uuid_part).is_ok());
}

#[test]
fn error_code_status_mapping() {
    assert_eq!(
        ErrorCode::Unauthorized.status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(ErrorCode::Forbidden.status_code(), StatusCode::FORBIDDEN);
    assert_eq!(
        ErrorCode::ValidationFailed.status_code(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(ErrorCode::NotFound.status_code(), StatusCode::NOT_FOUND);
    assert_eq!(ErrorCode::Conflict.status_code(), StatusCode::CONFLICT);
    assert_eq!(ErrorCode::Gone.status_code(), StatusCode::GONE);
    assert_eq!(
        ErrorCode::RateLimited.status_code(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        ErrorCode::InternalError.status_code(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn error_code_as_str() {
    assert_eq!(ErrorCode::Unauthorized.as_str(), "unauthorized");
    assert_eq!(ErrorCode::Forbidden.as_str(), "forbidden");
    assert_eq!(ErrorCode::ValidationFailed.as_str(), "validation_failed");
    assert_eq!(ErrorCode::NotFound.as_str(), "not_found");
    assert_eq!(ErrorCode::Conflict.as_str(), "conflict");
    assert_eq!(ErrorCode::Gone.as_str(), "gone");
    assert_eq!(ErrorCode::RateLimited.as_str(), "rate_limited");
    assert_eq!(ErrorCode::InternalError.as_str(), "internal_error");
}

#[tokio::test]
async fn app_error_into_response_format() {
    let error = AppError::new(ErrorCode::NotFound, "resource not found", "req-123");
    let response = error.into_response();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let error_response: ErrorResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(error_response.error.code, "not_found");
    assert_eq!(error_response.error.message, "resource not found");
    assert_eq!(error_response.error.request_id, "req-123");
    assert!(error_response.error.details.is_none());
}

#[tokio::test]
async fn app_error_unauthorized_response() {
    let error = AppError::unauthorized("invalid token", "req-456");
    let response = error.into_response();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let error_response: ErrorResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(error_response.error.code, "unauthorized");
    assert_eq!(error_response.error.message, "invalid token");
    assert_eq!(error_response.error.request_id, "req-456");
}

#[tokio::test]
async fn app_error_internal_error_response() {
    let error = AppError::internal_error("something went wrong", "req-789");
    let response = error.into_response();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let error_response: ErrorResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(error_response.error.code, "internal_error");
    assert_eq!(error_response.error.message, "something went wrong");
}

#[test]
fn request_id_clone() {
    let id = RequestId("test-123".to_string());
    let cloned = id.clone();
    assert_eq!(id.0, cloned.0);
}
