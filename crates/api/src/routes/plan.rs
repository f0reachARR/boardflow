use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::{Extension, Json};
use sqlx::PgPool;

use boardflow_api_types::plan::*;

use crate::error::{AppError, RequestId};
use crate::extractors::AuthenticatedToken;

#[utoipa::path(
    post,
    path = "/api/v1/runs/plan",
    request_body = PlanRequest,
    responses(
        (status = 200, description = "Plan decisions", body = PlanResponse),
        (status = 400, description = "Validation error", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    ),
    security(("bearer_auth" = []))
)]
pub async fn plan_run(
    auth: AuthenticatedToken,
    Extension(request_id): Extension<RequestId>,
    State(pool): State<PgPool>,
    payload: Result<Json<PlanRequest>, JsonRejection>,
) -> Result<Json<PlanResponse>, AppError> {
    let rid = &request_id.0;
    let Json(req) = payload.map_err(|e| AppError::validation_failed(e.body_text(), rid))?;

    let response = crate::services::plan::execute_plan_run(
        &pool,
        auth.0.repository_id,
        auth.0.installation_id,
        req,
        rid,
    )
    .await?;

    Ok(Json(response))
}
