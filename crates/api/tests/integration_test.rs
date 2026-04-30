use axum::body::Body;
use axum::http::{Request, StatusCode};
use boardflow_api::create_app;
use http_body_util::BodyExt;
use sqlx::PgPool;
use tower::ServiceExt;

/// OpenAPI JSONエンドポイントが200を返し、有効なJSONを含むことを確認
#[tokio::test]
async fn test_openapi_endpoint_returns_json() {
    // テスト用に接続不要のダミープールは作れないので、
    // OpenAPIエンドポイントはDB不要のため正常動作する想定
    // ただし PgPool は接続が必要なので、このテストはDB接続がある環境でのみ実行
    // ここでは cargo test 時にスキップするようにする
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("Skipping test: DATABASE_URL not set");
            return;
        }
    };

    let pool = PgPool::connect(&database_url).await.unwrap();
    let app = create_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["info"]["title"], "BoardFlow API");
}

/// healthzエンドポイントがDB接続成功時に200を返すことを確認
#[tokio::test]
async fn test_healthz_returns_ok_with_db() {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("Skipping test: DATABASE_URL not set");
            return;
        }
    };

    let pool = PgPool::connect(&database_url).await.unwrap();
    let app = create_app(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}
