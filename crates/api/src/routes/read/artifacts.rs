use axum::extract::{Path, State};
use axum::{Extension, Json};
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;

use boardflow_domain::models::artifact::ArtifactStatus;
use boardflow_domain::public_ids::{ArtifactId, BoardRunId};

use crate::error::{AppError, RequestId};
use crate::extractors::AuthenticatedSession;
use crate::github_access::DynGithubAccessChecker;

use super::dto::ArtifactListItem;
use crate::github_access::access_result_to_error;

#[derive(Debug, Serialize, ToSchema)]
pub struct ArtifactListResponse {
    pub items: Vec<ArtifactListItem>,
}

// ─── GET /api/v1/board-runs/{board_run_id}/artifacts ─────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/board-runs/{board_run_id}/artifacts",
    params(("board_run_id" = String, Path, description = "BoardRun ID (br_ prefix)")),
    responses(
        (status = 200, description = "Artifact list", body = ArtifactListResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    )
)]
pub async fn list_artifacts(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Path(board_run_id): Path<String>,
) -> Result<Json<ArtifactListResponse>, AppError> {
    let id = board_run_id
        .parse::<BoardRunId>()
        .map(BoardRunId::into_uuid)
        .map_err(|_| AppError::validation_failed("invalid board_run_id format", &request_id))?;

    // Check repository access via board_run → board_project → repository
    let repo = boardflow_db::queries::board_run::find_repository_by_board_run_id(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("list_artifacts repo lookup failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("board run not found", &request_id))?;

    let result = access_checker
        .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
        .await;
    if let Some(err) = access_result_to_error(&result, "board run not found", &request_id) {
        return Err(err);
    }

    let artifacts = boardflow_db::queries::artifact::list_by_board_run(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("list_artifacts failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?;

    let items: Vec<_> = artifacts
        .iter()
        .map(|a| {
            let is_available = a.status == ArtifactStatus::Available;
            ArtifactListItem {
                artifact_id: if is_available {
                    Some(ArtifactId::from(a.id))
                } else {
                    None
                },
                r#type: a.r#type,
                status: a.status,
                filename: if is_available {
                    a.filename.clone()
                } else {
                    None
                },
                content_type: if is_available {
                    a.content_type.clone()
                } else {
                    None
                },
                sha256: if is_available { a.sha256.clone() } else { None },
                size_bytes: if is_available { a.size_bytes } else { None },
                source_path: a.source_path.clone(),
                logical_name: a.logical_name.clone(),
                status_reason: a.status_reason.clone(),
                created_at: if is_available {
                    Some(a.created_at.to_rfc3339())
                } else {
                    None
                },
            }
        })
        .collect();

    Ok(Json(ArtifactListResponse { items }))
}
