use axum::extract::{Path, State};
use axum::{Extension, Json};
use sqlx::PgPool;

use boardflow_domain::public_ids::BoardRunId;

use crate::error::{AppError, RequestId};
use crate::extractors::AuthenticatedSession;
use crate::github_access::DynGithubAccessChecker;

use super::dto::{BoardRunDiffResponse, DiffMetadataResponse};
use crate::github_access::access_result_to_error;

// ─── GET /api/v1/board-runs/{board_run_id}/diff ──────────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/board-runs/{board_run_id}/diff",
    params(
        ("board_run_id" = String, Path, description = "Board run ID (br_<uuid>)")
    ),
    responses(
        (status = 200, description = "Diff details", body = BoardRunDiffResponse),
        (status = 400, description = "Validation error", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    )
)]
pub async fn get_board_run_diff(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Path(board_run_id): Path<String>,
) -> Result<Json<BoardRunDiffResponse>, AppError> {
    let id = board_run_id
        .parse::<BoardRunId>()
        .map(BoardRunId::into_uuid)
        .map_err(|_| AppError::validation_failed("invalid board_run_id format", &request_id))?;

    // Check repository access via board_run → board_project → repository
    let repo = boardflow_db::queries::board_run::find_repository_by_board_run_id(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("get_board_run_diff repo lookup failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("board run not found", &request_id))?;

    let result = access_checker
        .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
        .await;
    if let Some(err) = access_result_to_error(&result, "board run not found", &request_id) {
        return Err(err);
    }

    // Get diff
    let diff = boardflow_db::queries::diff::find_diff_by_board_run_id(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("find_diff_by_board_run_id failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("diff not found for this board run", &request_id))?;

    // Get metadata (optional)
    let metadata = boardflow_db::queries::diff::find_diff_metadata_by_board_run_id(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("find_diff_metadata_by_board_run_id failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?;

    let metadata_response = metadata.map(|m| DiffMetadataResponse {
        file_hashes: m.file_hashes_json,
        bom_summary: m.bom_summary_json,
        checks_summary: m.checks_summary_json,
        artifacts_summary: m.artifacts_summary_json,
        previews: m.previews_json,
    });

    Ok(Json(BoardRunDiffResponse {
        board_run_id: BoardRunId::from(id),
        base_board_run_id: diff.base_board_run_id.map(BoardRunId::from),
        status: diff.status,
        summary: diff.summary_json,
        metadata: metadata_response,
        error_message: diff.error_message,
        created_at: diff.created_at.to_rfc3339(),
    }))
}
