use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct RequestId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorized,
    Forbidden,
    ValidationFailed,
    NotFound,
    Conflict,
    Gone,
    RateLimited,
    InternalError,
}

impl ErrorCode {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::ValidationFailed => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict => StatusCode::CONFLICT,
            Self::Gone => StatusCode::GONE,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::ValidationFailed => "validation_failed",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::Gone => "gone",
            Self::RateLimited => "rate_limited",
            Self::InternalError => "internal_error",
        }
    }
}

#[derive(Debug, Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    pub request_id: String,
}

#[derive(Debug, Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ErrorResponse {
    pub error: ErrorBody,
}

#[derive(Debug)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub request_id: String,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
            request_id: request_id.into(),
        }
    }

    pub fn unauthorized(message: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unauthorized, message, request_id)
    }

    pub fn forbidden(message: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self::new(ErrorCode::Forbidden, message, request_id)
    }

    pub fn validation_failed(message: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self::new(ErrorCode::ValidationFailed, message, request_id)
    }

    pub fn not_found(message: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message, request_id)
    }

    pub fn conflict(message: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, message, request_id)
    }

    pub fn gone(message: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self::new(ErrorCode::Gone, message, request_id)
    }

    pub fn internal_error(message: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self::new(ErrorCode::InternalError, message, request_id)
    }
}

impl From<JsonRejection> for AppError {
    fn from(rejection: JsonRejection) -> Self {
        Self::new(ErrorCode::ValidationFailed, rejection.body_text(), "")
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.code.status_code();
        let body = ErrorResponse {
            error: ErrorBody {
                code: self.code,
                message: self.message,
                details: self.details,
                request_id: self.request_id,
            },
        };
        (status, Json(body)).into_response()
    }
}
