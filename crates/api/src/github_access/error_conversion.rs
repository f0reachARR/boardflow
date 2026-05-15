use crate::error::AppError;

use super::types::{AccessError, AccessResult};

// Helper: convert AccessResult::Denied/Error to AppError
pub fn access_result_to_error(
    result: &AccessResult,
    not_found_msg: &str,
    request_id: &str,
) -> Option<AppError> {
    match result {
        AccessResult::Allowed => None,
        AccessResult::Denied => Some(AppError::not_found(not_found_msg, request_id)),
        AccessResult::Error(AccessError::TokenExpired) => Some(AppError::unauthorized(
            "github session expired, please re-login",
            request_id,
        )),
        AccessResult::Error(AccessError::RateLimited) => Some(AppError::new(
            crate::error::ErrorCode::RateLimited,
            "rate limited",
            request_id,
        )),
        AccessResult::Error(AccessError::Upstream(detail)) => {
            tracing::error!("GitHub API error: {detail}");
            Some(AppError::internal_error("upstream error", request_id))
        }
    }
}

pub(crate) fn access_error_to_app_error(err: &AccessError, request_id: &str) -> AppError {
    match err {
        AccessError::TokenExpired => {
            AppError::unauthorized("github session expired, please re-login", request_id)
        }
        AccessError::RateLimited => AppError::new(
            crate::error::ErrorCode::RateLimited,
            "rate limited",
            request_id,
        ),
        AccessError::Upstream(detail) => {
            tracing::error!("GitHub API error: {detail}");
            AppError::internal_error("upstream error", request_id)
        }
    }
}
