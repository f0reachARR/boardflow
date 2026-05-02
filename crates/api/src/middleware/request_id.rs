use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

use crate::error::RequestId;

pub async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    let id = format!("req_{}", uuid::Uuid::now_v7());
    let request_id = RequestId(id.clone());
    request.extensions_mut().insert(request_id);

    let mut response = next.run(request).await;
    response.headers_mut().insert(
        "x-request-id",
        id.parse().expect("request id is valid header value"),
    );
    response
}
