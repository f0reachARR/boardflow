use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use boardflow_domain::models::artifact::ArtifactStatus;
use boardflow_domain::models::run_check::{FindingSeverity, SubjectKind};

use crate::error::{AppError, RequestId};
use crate::extractors::AuthenticatedSession;
use crate::github_access::{AccessError, AccessResult, DynGithubAccessChecker};
use crate::{ArtifactBaseUrl, ArtifactSecret};

// ─── ID prefix helpers ───────────────────────────────────────────────────────

fn format_board_project_id(id: Uuid) -> String {
    format!("bp_{id}")
}

fn format_board_run_id(id: Uuid) -> String {
    format!("br_{id}")
}

fn format_artifact_id(id: Uuid) -> String {
    format!("art_{id}")
}

fn parse_board_project_id(s: &str) -> Option<Uuid> {
    s.strip_prefix("bp_").and_then(|v| Uuid::parse_str(v).ok())
}

fn parse_board_run_id(s: &str) -> Option<Uuid> {
    s.strip_prefix("br_").and_then(|v| Uuid::parse_str(v).ok())
}

// ─── Cursor encoding/decoding ────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct CursorPayload {
    ts: String,
    id: String,
}

fn encode_cursor(ts: &DateTime<Utc>, id: &Uuid) -> String {
    let payload = CursorPayload {
        ts: ts.to_rfc3339(),
        id: id.to_string(),
    };
    let json = serde_json::to_string(&payload).unwrap();
    URL_SAFE_NO_PAD.encode(json.as_bytes())
}

fn decode_cursor(cursor: &str) -> Option<(DateTime<Utc>, Uuid)> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let payload: CursorPayload = serde_json::from_slice(&bytes).ok()?;
    let ts = DateTime::parse_from_rfc3339(&payload.ts).ok()?.to_utc();
    let id = Uuid::parse_str(&payload.id).ok()?;
    Some((ts, id))
}

// Repository cursor uses github_repository_id as tie-breaker
#[derive(Debug, Serialize, Deserialize)]
struct RepositoryCursorPayload {
    ts: String,
    gid: String,
}

fn encode_repository_cursor(ts: &DateTime<Utc>, github_repository_id: i64) -> String {
    let payload = RepositoryCursorPayload {
        ts: ts.to_rfc3339(),
        gid: github_repository_id.to_string(),
    };
    let json = serde_json::to_string(&payload).unwrap();
    URL_SAFE_NO_PAD.encode(json.as_bytes())
}

fn decode_repository_cursor(cursor: &str) -> Option<(DateTime<Utc>, i64)> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let payload: RepositoryCursorPayload = serde_json::from_slice(&bytes).ok()?;
    let ts = DateTime::parse_from_rfc3339(&payload.ts).ok()?.to_utc();
    let gid: i64 = payload.gid.parse().ok()?;
    Some((ts, gid))
}

// ─── Query parameters ────────────────────────────────────────────────────────

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

fn access_error_to_app_error(err: &AccessError, request_id: &str) -> AppError {
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

#[derive(Debug, Deserialize, IntoParams)]
pub struct PaginationParams {
    #[param(default = 50, minimum = 1, maximum = 100)]
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

impl PaginationParams {
    fn effective_limit(&self) -> i64 {
        self.limit.unwrap_or(50).clamp(1, 100)
    }

    fn decoded_cursor(&self, request_id: &str) -> Result<Option<(DateTime<Utc>, Uuid)>, AppError> {
        match &self.cursor {
            None => Ok(None),
            Some(c) => decode_cursor(c)
                .map(Some)
                .ok_or_else(|| AppError::validation_failed("invalid cursor", request_id)),
        }
    }

    fn decoded_repository_cursor(
        &self,
        request_id: &str,
    ) -> Result<Option<(DateTime<Utc>, i64)>, AppError> {
        match &self.cursor {
            None => Ok(None),
            Some(c) => decode_repository_cursor(c)
                .map(Some)
                .ok_or_else(|| AppError::validation_failed("invalid cursor", request_id)),
        }
    }
}

// ─── Response types ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, ToSchema)]
pub struct PaginatedResponse<T: Serialize> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

// Repository responses
#[derive(Debug, Serialize, ToSchema)]
pub struct RepositoryListItem {
    pub github_repository_id: String,
    pub owner: String,
    pub name: String,
    pub installation_id: String,
    pub board_project_count: i64,
    pub latest_run_status: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RepositoryDetailResponse {
    pub github_repository_id: String,
    pub owner: String,
    pub name: String,
    pub installation_id: String,
    pub html_url: String,
    pub board_project_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

// BoardProject responses
#[derive(Debug, Serialize, ToSchema)]
pub struct BoardProjectListItem {
    pub board_project_id: String,
    pub project_path: String,
    pub project_dir: String,
    pub display_name: String,
    pub state: String,
    pub latest_completed_run_id: Option<String>,
    pub latest_tree_hash: Option<String>,
    pub issue_url: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BoardProjectDetailResponse {
    pub board_project_id: String,
    pub repository: RepositoryRef,
    pub project_path: String,
    pub project_dir: String,
    pub display_name: String,
    pub state: String,
    pub latest_completed_run_id: Option<String>,
    pub latest_tree_hash: Option<String>,
    pub issue_number: Option<i32>,
    pub issue_url: Option<String>,
    pub recreate_issue_on_update: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RepositoryRef {
    pub github_repository_id: String,
    pub owner: String,
    pub name: String,
}

// BoardRun responses
#[derive(Debug, Serialize, ToSchema)]
pub struct BoardRunListItem {
    pub board_run_id: String,
    pub status: String,
    pub commit_sha: String,
    pub branch: String,
    #[serde(rename = "ref")]
    pub ref_: String,
    pub github_run_id: String,
    pub github_run_attempt: String,
    pub tree_hash: Option<String>,
    pub erc_status: Option<String>,
    pub erc_errors: i32,
    pub erc_warnings: i32,
    pub drc_status: Option<String>,
    pub drc_errors: i32,
    pub drc_warnings: i32,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BoardRunDetailResponse {
    pub board_run_id: String,
    pub board_project_id: String,
    pub status: String,
    pub commit_sha: String,
    pub branch: String,
    #[serde(rename = "ref")]
    pub ref_: String,
    pub github_run_id: String,
    pub github_run_attempt: String,
    pub tree_hash: Option<String>,
    pub checks: Vec<CheckInfo>,
    pub artifact_summary: ArtifactSummary,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CheckInfo {
    pub kind: String,
    pub status: String,
    pub error_count: i32,
    pub warning_count: i32,
    pub notice_count: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ArtifactSummary {
    pub available: i64,
    pub missing: i64,
    pub failed: i64,
    pub skipped: i64,
}

// Artifact responses
#[derive(Debug, Serialize, ToSchema)]
pub struct ArtifactListItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    pub r#type: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logical_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

// Viewer Sources responses
#[derive(Debug, Serialize, ToSchema)]
pub struct ViewerSourcesResponse {
    pub board_run_id: String,
    pub expires_at: String,
    pub viewers: ViewerMap,
}

// Diff responses
#[derive(Debug, Serialize, ToSchema)]
pub struct BoardRunDiffResponse {
    pub board_run_id: String,
    pub base_board_run_id: Option<String>,
    pub status: String,
    pub summary: Option<serde_json::Value>,
    pub metadata: Option<DiffMetadataResponse>,
    pub error_message: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DiffMetadataResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_hashes: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bom_summary: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checks_summary: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts_summary: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previews: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ViewerMap {
    pub kicanvas: ViewerStatus,
    pub schematic: ViewerStatus,
    pub pcb_preview: ViewerStatus,
    pub ibom: ViewerStatus,
    pub bom: ViewerStatus,
    pub fabrication: ViewerStatus,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ViewerStatus {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<ViewerSource>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<ViewerSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iframe_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloads: Option<Vec<ViewerDownload>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ViewerSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ViewerDownload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    pub artifact_type: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
}

// ─── Helper: board project state derivation ──────────────────────────────────

fn derive_board_project_state(
    latest_completed_run_id: Option<Uuid>,
    latest_run_status: Option<&str>,
) -> &'static str {
    match latest_completed_run_id {
        Some(_) => "completed",
        None => match latest_run_status {
            Some("failed") => "failed",
            Some("timed_out") => "timed_out",
            Some("created") | Some("uploading") | Some("importing") => "processing",
            _ => "detected",
        },
    }
}

// ─── GET /api/v1/repositories ────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/repositories",
    params(PaginationParams),
    responses(
        (status = 200, description = "Repository list", body = PaginatedResponse<RepositoryListItem>),
        (status = 400, description = "Validation error", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
    )
)]
pub async fn list_repositories(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<RepositoryListItem>>, AppError> {
    let limit = params.effective_limit();
    let cursor = params.decoded_repository_cursor(&request_id)?;

    // Pre-filter: get accessible repo ids from GitHub API
    let token = &session.user.github_access_token;
    let accessible_ids = access_checker
        .list_accessible_repo_ids(token)
        .await
        .map_err(|e| access_error_to_app_error(&e, &request_id))?;

    let rows = boardflow_db::queries::repository::list_with_stats(
        &pool,
        limit + 1,
        cursor,
        accessible_ids.as_deref(),
    )
    .await
    .map_err(|e| {
        tracing::error!("list_repositories failed: {e}");
        AppError::internal_error("database error", &request_id)
    })?;

    let has_more = rows.len() as i64 > limit;
    let items: Vec<_> = rows
        .iter()
        .take(limit as usize)
        .map(|r| RepositoryListItem {
            github_repository_id: r.github_repository_id.to_string(),
            owner: r.owner.clone(),
            name: r.name.clone(),
            installation_id: r.installation_id.to_string(),
            board_project_count: r.board_project_count,
            latest_run_status: r.latest_run_status.clone(),
            updated_at: r.updated_at.to_rfc3339(),
        })
        .collect();

    let next_cursor = if has_more {
        items.last().map(|_| {
            let last = &rows[limit as usize - 1];
            encode_repository_cursor(&last.updated_at, last.github_repository_id)
        })
    } else {
        None
    };

    Ok(Json(PaginatedResponse {
        items,
        next_cursor,
        has_more,
    }))
}

// ─── GET /api/v1/repositories/{github_repository_id} ─────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/repositories/{github_repository_id}",
    params(("github_repository_id" = i64, Path, description = "GitHub repository ID")),
    responses(
        (status = 200, description = "Repository detail", body = RepositoryDetailResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    )
)]
pub async fn get_repository(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Path(github_repository_id): Path<i64>,
) -> Result<Json<RepositoryDetailResponse>, AppError> {
    let repo = boardflow_db::queries::repository::find_by_github_id(&pool, github_repository_id)
        .await
        .map_err(|e| {
            tracing::error!("get_repository failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("repository not found", &request_id))?;

    let result = access_checker
        .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
        .await;
    if let Some(err) = access_result_to_error(&result, "repository not found", &request_id) {
        return Err(err);
    }

    let board_project_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM board_projects WHERE repository_id = $1")
            .bind(repo.id)
            .fetch_one(&pool)
            .await
            .map_err(|e| {
                tracing::error!("count board_projects failed: {e}");
                AppError::internal_error("database error", &request_id)
            })?;

    let html_url = format!("https://github.com/{}/{}", repo.owner, repo.name);

    Ok(Json(RepositoryDetailResponse {
        github_repository_id: repo.github_repository_id.to_string(),
        owner: repo.owner,
        name: repo.name,
        installation_id: repo.installation_id.to_string(),
        html_url,
        board_project_count,
        created_at: repo.created_at.to_rfc3339(),
        updated_at: repo.updated_at.to_rfc3339(),
    }))
}

// ─── GET /api/v1/repositories/{github_repository_id}/board-projects ──────────

#[utoipa::path(
    get,
    path = "/api/v1/repositories/{github_repository_id}/board-projects",
    params(
        ("github_repository_id" = i64, Path, description = "GitHub repository ID"),
        PaginationParams
    ),
    responses(
        (status = 200, description = "BoardProject list", body = PaginatedResponse<BoardProjectListItem>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    )
)]
pub async fn list_board_projects(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Path(github_repository_id): Path<i64>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<BoardProjectListItem>>, AppError> {
    let repo = boardflow_db::queries::repository::find_by_github_id(&pool, github_repository_id)
        .await
        .map_err(|e| {
            tracing::error!("list_board_projects repo lookup failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("repository not found", &request_id))?;

    let result = access_checker
        .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
        .await;
    if let Some(err) = access_result_to_error(&result, "repository not found", &request_id) {
        return Err(err);
    }

    let limit = params.effective_limit();
    let cursor = params.decoded_cursor(&request_id)?;

    let rows = boardflow_db::queries::board_project::list_by_repository_id_with_status(
        &pool,
        repo.id,
        limit + 1,
        cursor,
    )
    .await
    .map_err(|e| {
        tracing::error!("list_board_projects failed: {e}");
        AppError::internal_error("database error", &request_id)
    })?;

    let has_more = rows.len() as i64 > limit;
    let items: Vec<_> = rows
        .iter()
        .take(limit as usize)
        .map(|bp| {
            let state = derive_board_project_state(
                bp.latest_completed_run_id,
                bp.latest_run_status.as_deref(),
            );
            BoardProjectListItem {
                board_project_id: format_board_project_id(bp.id),
                project_path: bp.project_path.clone(),
                project_dir: bp.project_dir.clone(),
                display_name: bp.display_name.clone(),
                state: state.to_string(),
                latest_completed_run_id: bp.latest_completed_run_id.map(format_board_run_id),
                latest_tree_hash: bp.latest_tree_hash.clone(),
                issue_url: bp.issue_url.clone(),
                updated_at: bp.updated_at.to_rfc3339(),
            }
        })
        .collect();

    let next_cursor = if has_more {
        items.last().map(|_| {
            let last = &rows[limit as usize - 1];
            encode_cursor(&last.updated_at, &last.id)
        })
    } else {
        None
    };

    Ok(Json(PaginatedResponse {
        items,
        next_cursor,
        has_more,
    }))
}

// ─── GET /api/v1/board-projects/{board_project_id} ───────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/board-projects/{board_project_id}",
    params(("board_project_id" = String, Path, description = "BoardProject ID (bp_ prefix)")),
    responses(
        (status = 200, description = "BoardProject detail", body = BoardProjectDetailResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    )
)]
pub async fn get_board_project(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Path(board_project_id): Path<String>,
) -> Result<Json<BoardProjectDetailResponse>, AppError> {
    let id = parse_board_project_id(&board_project_id).ok_or_else(|| {
        AppError::validation_failed("invalid board_project_id format", &request_id)
    })?;

    let row = boardflow_db::queries::board_project::find_by_id_with_repository(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("get_board_project failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("board project not found", &request_id))?;

    if let Some(err) = access_result_to_error(
        &access_checker
            .check_access(
                &session.user.github_access_token,
                &row.repo_owner,
                &row.repo_name,
            )
            .await,
        "board project not found",
        &request_id,
    ) {
        return Err(err);
    }

    // Get latest run status for state derivation
    let latest_run_status = boardflow_db::queries::board_project::get_latest_run_status(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("get_board_project latest status failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?;

    let state =
        derive_board_project_state(row.latest_completed_run_id, latest_run_status.as_deref());

    Ok(Json(BoardProjectDetailResponse {
        board_project_id: format_board_project_id(row.id),
        repository: RepositoryRef {
            github_repository_id: row.github_repository_id.to_string(),
            owner: row.repo_owner,
            name: row.repo_name,
        },
        project_path: row.project_path,
        project_dir: row.project_dir,
        display_name: row.display_name,
        state: state.to_string(),
        latest_completed_run_id: row.latest_completed_run_id.map(format_board_run_id),
        latest_tree_hash: row.latest_tree_hash,
        issue_number: row.issue_number,
        issue_url: row.issue_url,
        recreate_issue_on_update: row.recreate_issue_on_update,
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
    }))
}

// ─── GET /api/v1/board-projects/{board_project_id}/board-runs ────────────────

#[utoipa::path(
    get,
    path = "/api/v1/board-projects/{board_project_id}/board-runs",
    params(
        ("board_project_id" = String, Path, description = "BoardProject ID (bp_ prefix)"),
        PaginationParams
    ),
    responses(
        (status = 200, description = "BoardRun list", body = PaginatedResponse<BoardRunListItem>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    )
)]
pub async fn list_board_runs(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Path(board_project_id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<BoardRunListItem>>, AppError> {
    let bp_id = parse_board_project_id(&board_project_id).ok_or_else(|| {
        AppError::validation_failed("invalid board_project_id format", &request_id)
    })?;

    // Verify board_project exists and check repository access
    let repo =
        boardflow_db::queries::board_project::find_repository_by_board_project_id(&pool, bp_id)
            .await
            .map_err(|e| {
                tracing::error!("list_board_runs repo lookup failed: {e}");
                AppError::internal_error("database error", &request_id)
            })?
            .ok_or_else(|| AppError::not_found("board project not found", &request_id))?;

    let result = access_checker
        .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
        .await;
    if let Some(err) = access_result_to_error(&result, "board project not found", &request_id) {
        return Err(err);
    }

    let limit = params.effective_limit();
    let cursor = params.decoded_cursor(&request_id)?;

    let rows =
        boardflow_db::queries::board_run::list_by_board_project(&pool, bp_id, limit + 1, cursor)
            .await
            .map_err(|e| {
                tracing::error!("list_board_runs failed: {e}");
                AppError::internal_error("database error", &request_id)
            })?;

    let has_more = rows.len() as i64 > limit;
    let items: Vec<_> = rows
        .iter()
        .take(limit as usize)
        .map(|r| BoardRunListItem {
            board_run_id: format_board_run_id(r.id),
            status: format!("{:?}", r.status).to_lowercase(),
            commit_sha: r.commit_sha.clone(),
            branch: r.branch.clone(),
            ref_: r.r#ref.clone(),
            github_run_id: r.github_run_id.to_string(),
            github_run_attempt: r.github_run_attempt.to_string(),
            tree_hash: r.tree_hash.clone(),
            erc_status: r.erc_status.map(|s| format!("{:?}", s).to_lowercase()),
            erc_errors: r.erc_errors,
            erc_warnings: r.erc_warnings,
            drc_status: r.drc_status.map(|s| format!("{:?}", s).to_lowercase()),
            drc_errors: r.drc_errors,
            drc_warnings: r.drc_warnings,
            created_at: r.created_at.to_rfc3339(),
            completed_at: r.completed_at.map(|t| t.to_rfc3339()),
        })
        .collect();

    let next_cursor = if has_more {
        items.last().map(|_| {
            let last = &rows[limit as usize - 1];
            encode_cursor(&last.created_at, &last.id)
        })
    } else {
        None
    };

    Ok(Json(PaginatedResponse {
        items,
        next_cursor,
        has_more,
    }))
}

// ─── GET /api/v1/board-runs/{board_run_id} ───────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/board-runs/{board_run_id}",
    params(("board_run_id" = String, Path, description = "BoardRun ID (br_ prefix)")),
    responses(
        (status = 200, description = "BoardRun detail", body = BoardRunDetailResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    )
)]
pub async fn get_board_run(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Path(board_run_id): Path<String>,
) -> Result<Json<BoardRunDetailResponse>, AppError> {
    let id = parse_board_run_id(&board_run_id)
        .ok_or_else(|| AppError::validation_failed("invalid board_run_id format", &request_id))?;

    // Check repository access via board_run → board_project → repository
    let repo = boardflow_db::queries::board_run::find_repository_by_board_run_id(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("get_board_run repo lookup failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("board run not found", &request_id))?;

    let result = access_checker
        .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
        .await;
    if let Some(err) = access_result_to_error(&result, "board run not found", &request_id) {
        return Err(err);
    }

    let run = boardflow_db::queries::board_run::find_by_id(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("get_board_run failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("board run not found", &request_id))?;

    let checks = boardflow_db::queries::run_check::list_by_board_run(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("get_board_run checks failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?;

    let artifacts = boardflow_db::queries::artifact::list_by_board_run(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("get_board_run artifacts failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?;

    let check_infos: Vec<CheckInfo> = checks
        .iter()
        .map(|c| CheckInfo {
            kind: format!("{:?}", c.check_kind).to_lowercase(),
            status: format!("{:?}", c.status).to_lowercase(),
            error_count: c.error_count,
            warning_count: c.warning_count,
            notice_count: c.notice_count,
        })
        .collect();

    let artifact_summary = ArtifactSummary {
        available: artifacts
            .iter()
            .filter(|a| a.status == ArtifactStatus::Available)
            .count() as i64,
        missing: artifacts
            .iter()
            .filter(|a| a.status == ArtifactStatus::Missing)
            .count() as i64,
        failed: artifacts
            .iter()
            .filter(|a| a.status == ArtifactStatus::Failed)
            .count() as i64,
        skipped: artifacts
            .iter()
            .filter(|a| a.status == ArtifactStatus::Skipped)
            .count() as i64,
    };

    Ok(Json(BoardRunDetailResponse {
        board_run_id: format_board_run_id(run.id),
        board_project_id: format_board_project_id(run.board_project_id),
        status: format!("{:?}", run.status).to_lowercase(),
        commit_sha: run.commit_sha,
        branch: run.branch,
        ref_: run.r#ref,
        github_run_id: run.github_run_id.to_string(),
        github_run_attempt: run.github_run_attempt.to_string(),
        tree_hash: run.tree_hash,
        checks: check_infos,
        artifact_summary,
        created_at: run.created_at.to_rfc3339(),
        completed_at: run.completed_at.map(|t| t.to_rfc3339()),
    }))
}

// ─── GET /api/v1/board-runs/{board_run_id}/artifacts ─────────────────────────

#[derive(Debug, Serialize, ToSchema)]
pub struct ArtifactListResponse {
    pub items: Vec<ArtifactListItem>,
}

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
    let id = parse_board_run_id(&board_run_id)
        .ok_or_else(|| AppError::validation_failed("invalid board_run_id format", &request_id))?;

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
                    Some(format_artifact_id(a.id))
                } else {
                    None
                },
                r#type: a.r#type.clone(),
                status: format!("{:?}", a.status).to_lowercase(),
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

// ─── GET /api/v1/board-runs/{board_run_id}/viewer-sources ────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/board-runs/{board_run_id}/viewer-sources",
    params(("board_run_id" = String, Path, description = "BoardRun ID (br_ prefix)")),
    responses(
        (status = 200, description = "Viewer sources", body = ViewerSourcesResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    )
)]
pub async fn get_viewer_sources(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    Extension(artifact_secret): Extension<ArtifactSecret>,
    Extension(artifact_base_url): Extension<ArtifactBaseUrl>,
    State(pool): State<PgPool>,
    Path(board_run_id): Path<String>,
) -> Result<Json<ViewerSourcesResponse>, AppError> {
    let id = parse_board_run_id(&board_run_id)
        .ok_or_else(|| AppError::validation_failed("invalid board_run_id format", &request_id))?;

    // Check repository access via board_run → board_project → repository
    let repo = boardflow_db::queries::board_run::find_repository_by_board_run_id(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("get_viewer_sources repo lookup failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("board run not found", &request_id))?;

    let result = access_checker
        .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
        .await;
    if let Some(err) = access_result_to_error(&result, "board run not found", &request_id) {
        return Err(err);
    }

    // Verify board_run exists
    boardflow_db::queries::board_run::find_by_id(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("get_viewer_sources run lookup failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("board run not found", &request_id))?;

    let artifacts = boardflow_db::queries::artifact::list_by_board_run(&pool, id)
        .await
        .map_err(|e| {
            tracing::error!("get_viewer_sources artifacts failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?;

    let expires_at = Utc::now() + chrono::Duration::hours(1);

    // Helper: find artifact by type
    let find_artifact =
        |artifact_type: &str| -> Option<&boardflow_domain::models::artifact::Artifact> {
            artifacts.iter().find(|a| a.r#type == artifact_type)
        };

    let user_id = session.user.id;
    let secret = &artifact_secret.0;
    let proxy_url = |a: &boardflow_domain::models::artifact::Artifact| -> String {
        let token = crate::artifact_token::generate_artifact_token(a.id, user_id, secret);
        format!(
            "{}/proxy/artifacts/{}?token={}",
            artifact_base_url.0,
            format_artifact_id(a.id),
            token
        )
    };

    // KiCanvas viewer: needs kicad_pro, kicad_sch, kicad_pcb
    let kicanvas = {
        let pro = find_artifact("kicad_pro");
        let sch = find_artifact("kicad_sch");
        let pcb = find_artifact("kicad_pcb");
        let all = [pro, sch, pcb];
        let available_count = all
            .iter()
            .filter(|a| a.is_some_and(|art| art.status == ArtifactStatus::Available))
            .count();

        let status = viewer_status(available_count, 3, &all);

        let sources = if available_count > 0 {
            let mut srcs = Vec::new();
            if let Some(a) = pro.filter(|a| a.status == ArtifactStatus::Available) {
                srcs.push(ViewerSource {
                    artifact_id: Some(format_artifact_id(a.id)),
                    artifact_type: None,
                    kind: Some("project".to_string()),
                    name: a.filename.clone(),
                    source_path: a.source_path.clone(),
                    url: Some(proxy_url(a)),
                });
            }
            if let Some(a) = sch.filter(|a| a.status == ArtifactStatus::Available) {
                srcs.push(ViewerSource {
                    artifact_id: Some(format_artifact_id(a.id)),
                    artifact_type: None,
                    kind: Some("schematic".to_string()),
                    name: a.filename.clone(),
                    source_path: a.source_path.clone(),
                    url: Some(proxy_url(a)),
                });
            }
            if let Some(a) = pcb.filter(|a| a.status == ArtifactStatus::Available) {
                srcs.push(ViewerSource {
                    artifact_id: Some(format_artifact_id(a.id)),
                    artifact_type: None,
                    kind: Some("board".to_string()),
                    name: a.filename.clone(),
                    source_path: a.source_path.clone(),
                    url: Some(proxy_url(a)),
                });
            }
            Some(srcs)
        } else {
            None
        };

        ViewerStatus {
            status,
            sources,
            primary: None,
            iframe_url: None,
            downloads: None,
        }
    };

    // Schematic viewer: needs schematic_pdf
    let schematic = {
        let pdf = find_artifact("schematic_pdf");
        let status = match pdf {
            Some(a) if a.status == ArtifactStatus::Available => "available",
            Some(a) if a.status == ArtifactStatus::Failed => "failed",
            _ => "missing",
        };
        let primary = pdf
            .filter(|a| a.status == ArtifactStatus::Available)
            .map(|a| ViewerSource {
                artifact_id: Some(format_artifact_id(a.id)),
                artifact_type: Some("schematic_pdf".to_string()),
                kind: None,
                name: None,
                source_path: None,
                url: Some(proxy_url(a)),
            });
        ViewerStatus {
            status: status.to_string(),
            sources: None,
            primary,
            iframe_url: None,
            downloads: None,
        }
    };

    // PCB Preview: needs pcb_top_svg, pcb_bottom_svg
    let pcb_preview = {
        let top = find_artifact("pcb_top_svg");
        let bottom = find_artifact("pcb_bottom_svg");
        let all = [top, bottom];
        let available_count = all
            .iter()
            .filter(|a| a.is_some_and(|art| art.status == ArtifactStatus::Available))
            .count();
        let status = viewer_status(available_count, 2, &all);

        let sources = if available_count > 0 {
            let mut srcs = Vec::new();
            if let Some(a) = top.filter(|a| a.status == ArtifactStatus::Available) {
                srcs.push(ViewerSource {
                    artifact_id: Some(format_artifact_id(a.id)),
                    artifact_type: Some("pcb_top_svg".to_string()),
                    kind: None,
                    name: None,
                    source_path: None,
                    url: Some(proxy_url(a)),
                });
            }
            if let Some(a) = bottom.filter(|a| a.status == ArtifactStatus::Available) {
                srcs.push(ViewerSource {
                    artifact_id: Some(format_artifact_id(a.id)),
                    artifact_type: Some("pcb_bottom_svg".to_string()),
                    kind: None,
                    name: None,
                    source_path: None,
                    url: Some(proxy_url(a)),
                });
            }
            Some(srcs)
        } else {
            None
        };

        ViewerStatus {
            status,
            sources,
            primary: None,
            iframe_url: None,
            downloads: None,
        }
    };

    // iBOM viewer: needs ibom
    let ibom = {
        let html = find_artifact("ibom");
        let status = match html {
            Some(a) if a.status == ArtifactStatus::Available => "available",
            Some(a) if a.status == ArtifactStatus::Failed => "failed",
            _ => "missing",
        };
        let iframe_url = html
            .filter(|a| a.status == ArtifactStatus::Available)
            .map(proxy_url);
        ViewerStatus {
            status: status.to_string(),
            sources: None,
            primary: None,
            iframe_url,
            downloads: None,
        }
    };

    // BOM viewer: needs bom_csv
    let bom = {
        let csv = find_artifact("bom_csv");
        let status = match csv {
            Some(a) if a.status == ArtifactStatus::Available => "available",
            Some(a) if a.status == ArtifactStatus::Failed => "failed",
            _ => "missing",
        };
        let downloads = csv
            .filter(|a| a.status == ArtifactStatus::Available)
            .map(|a| {
                vec![ViewerDownload {
                    artifact_id: Some(format_artifact_id(a.id)),
                    artifact_type: "bom_csv".to_string(),
                    status: "available".to_string(),
                    url: Some(proxy_url(a)),
                    status_reason: None,
                }]
            });
        ViewerStatus {
            status: status.to_string(),
            sources: None,
            primary: None,
            iframe_url: None,
            downloads,
        }
    };

    // Fabrication viewer: needs gerber_zip, drill_zip
    let fabrication = {
        let gerber = find_artifact("gerber_zip");
        let drill = find_artifact("drill_zip");
        let all = [gerber, drill];
        let available_count = all
            .iter()
            .filter(|a| a.is_some_and(|art| art.status == ArtifactStatus::Available))
            .count();
        let status = viewer_status(available_count, 2, &all);

        let mut downloads = Vec::new();
        match gerber {
            Some(a) if a.status == ArtifactStatus::Available => {
                downloads.push(ViewerDownload {
                    artifact_id: Some(format_artifact_id(a.id)),
                    artifact_type: "gerber_zip".to_string(),
                    status: "available".to_string(),
                    url: Some(proxy_url(a)),
                    status_reason: None,
                });
            }
            Some(a) => {
                downloads.push(ViewerDownload {
                    artifact_id: None,
                    artifact_type: "gerber_zip".to_string(),
                    status: format!("{:?}", a.status).to_lowercase(),
                    url: None,
                    status_reason: a.status_reason.clone(),
                });
            }
            None => {
                downloads.push(ViewerDownload {
                    artifact_id: None,
                    artifact_type: "gerber_zip".to_string(),
                    status: "missing".to_string(),
                    url: None,
                    status_reason: None,
                });
            }
        }
        match drill {
            Some(a) if a.status == ArtifactStatus::Available => {
                downloads.push(ViewerDownload {
                    artifact_id: Some(format_artifact_id(a.id)),
                    artifact_type: "drill_zip".to_string(),
                    status: "available".to_string(),
                    url: Some(proxy_url(a)),
                    status_reason: None,
                });
            }
            Some(a) => {
                downloads.push(ViewerDownload {
                    artifact_id: None,
                    artifact_type: "drill_zip".to_string(),
                    status: format!("{:?}", a.status).to_lowercase(),
                    url: None,
                    status_reason: a.status_reason.clone(),
                });
            }
            None => {
                downloads.push(ViewerDownload {
                    artifact_id: None,
                    artifact_type: "drill_zip".to_string(),
                    status: "missing".to_string(),
                    url: None,
                    status_reason: None,
                });
            }
        }

        ViewerStatus {
            status,
            sources: None,
            primary: None,
            iframe_url: None,
            downloads: Some(downloads),
        }
    };

    Ok(Json(ViewerSourcesResponse {
        board_run_id: format_board_run_id(id),
        expires_at: expires_at.to_rfc3339(),
        viewers: ViewerMap {
            kicanvas,
            schematic,
            pcb_preview,
            ibom,
            bom,
            fabrication,
        },
    }))
}

// ─── Viewer status helper ────────────────────────────────────────────────────

fn viewer_status(
    available_count: usize,
    required_count: usize,
    artifacts: &[Option<&boardflow_domain::models::artifact::Artifact>],
) -> String {
    if available_count == required_count {
        "available".to_string()
    } else if available_count > 0 {
        "partial".to_string()
    } else {
        // Check if all are skipped
        let all_skipped = artifacts
            .iter()
            .all(|a| a.is_some_and(|art| art.status == ArtifactStatus::Skipped));
        if all_skipped && artifacts.iter().any(|a| a.is_some()) {
            return "skipped".to_string();
        }
        // Check if any are failed
        let has_failed = artifacts
            .iter()
            .any(|a| a.is_some_and(|art| art.status == ArtifactStatus::Failed));
        if has_failed {
            "failed".to_string()
        } else {
            "missing".to_string()
        }
    }
}

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
    let id = parse_board_run_id(&board_run_id)
        .ok_or_else(|| AppError::validation_failed("invalid board_run_id format", &request_id))?;

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

    let status_str = match diff.status {
        boardflow_domain::models::snapshot::BoardRunDiffStatus::Ready => "ready",
        boardflow_domain::models::snapshot::BoardRunDiffStatus::NoBaseline => "no_baseline",
        boardflow_domain::models::snapshot::BoardRunDiffStatus::Unavailable => "unavailable",
        boardflow_domain::models::snapshot::BoardRunDiffStatus::Failed => "failed",
    };

    Ok(Json(BoardRunDiffResponse {
        board_run_id: format_board_run_id(id),
        base_board_run_id: diff.base_board_run_id.map(format_board_run_id),
        status: status_str.to_string(),
        summary: diff.summary_json,
        metadata: metadata_response,
        error_message: diff.error_message,
        created_at: diff.created_at.to_rfc3339(),
    }))
}

// ─── Findings cursor encoding/decoding ───────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct FindingsCursorPayload {
    si: i32,
    id: String,
}

fn encode_findings_cursor(sort_index: i32, id: &Uuid) -> String {
    let payload = FindingsCursorPayload {
        si: sort_index,
        id: id.to_string(),
    };
    let json = serde_json::to_string(&payload).unwrap();
    URL_SAFE_NO_PAD.encode(json.as_bytes())
}

fn decode_findings_cursor(cursor: &str) -> Option<(i32, Uuid)> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let payload: FindingsCursorPayload = serde_json::from_slice(&bytes).ok()?;
    let id = Uuid::parse_str(&payload.id).ok()?;
    Some((payload.si, id))
}

// ─── Findings query parameters ───────────────────────────────────────────────

#[derive(Debug, Deserialize, IntoParams)]
pub struct FindingsQueryParams {
    #[param(default = 50, minimum = 1, maximum = 100)]
    pub limit: Option<i64>,
    pub cursor: Option<String>,
    pub severity: Option<String>,
}

// ─── Findings response types ─────────────────────────────────────────────────

#[derive(Debug, Serialize, ToSchema)]
pub struct FindingListItem {
    pub id: String,
    pub severity: String,
    pub rule_code: Option<String>,
    pub title: Option<String>,
    pub message: Option<String>,
    pub subject_kind: Option<String>,
    pub subject_ref: Option<String>,
    pub sheet_path: Option<String>,
    pub pcb_layer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos_mm: Option<CoordinateMmResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CoordinateMmResponse {
    pub x: f64,
    pub y: f64,
}

// ─── GET /api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings ──────

#[utoipa::path(
    get,
    path = "/api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings",
    params(
        ("board_run_id" = String, Path, description = "BoardRun ID (br_ prefix)"),
        ("check_kind" = String, Path, description = "Check kind: erc or drc"),
        FindingsQueryParams,
    ),
    responses(
        (status = 200, description = "Findings list", body = PaginatedResponse<FindingListItem>),
        (status = 400, description = "Validation error", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorResponse),
    )
)]
pub async fn list_findings(
    session: AuthenticatedSession,
    Extension(RequestId(request_id)): Extension<RequestId>,
    Extension(access_checker): Extension<DynGithubAccessChecker>,
    State(pool): State<PgPool>,
    Path((board_run_id, check_kind)): Path<(String, String)>,
    Query(params): Query<FindingsQueryParams>,
) -> Result<Json<PaginatedResponse<FindingListItem>>, AppError> {
    // 1. Parse board_run_id
    let br_id = parse_board_run_id(&board_run_id)
        .ok_or_else(|| AppError::validation_failed("invalid board_run_id format", &request_id))?;

    // 2. Validate check_kind
    if check_kind != "erc" && check_kind != "drc" {
        return Err(AppError::validation_failed(
            "check_kind must be 'erc' or 'drc'",
            &request_id,
        ));
    }

    // 3. Validate cursor (must reject invalid cursor before any early-return path)
    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let cursor = match &params.cursor {
        None => None,
        Some(c) => Some(
            decode_findings_cursor(c)
                .ok_or_else(|| AppError::validation_failed("invalid cursor", &request_id))?,
        ),
    };

    // 4. Validate severity if provided
    if let Some(ref sev) = params.severity
        && sev != "error"
        && sev != "warning"
        && sev != "notice"
    {
        return Err(AppError::validation_failed(
            "severity must be 'error', 'warning', or 'notice'",
            &request_id,
        ));
    }

    // 5. Check repository access (same pattern as get_board_run)
    let repo = boardflow_db::queries::board_run::find_repository_by_board_run_id(&pool, br_id)
        .await
        .map_err(|e| {
            tracing::error!("list_findings repo lookup failed: {e}");
            AppError::internal_error("database error", &request_id)
        })?
        .ok_or_else(|| AppError::not_found("board run not found", &request_id))?;

    let result = access_checker
        .check_access(&session.user.github_access_token, &repo.owner, &repo.name)
        .await;
    if let Some(err) = access_result_to_error(&result, "board run not found", &request_id) {
        return Err(err);
    }

    // 6. Find run_check by board_run_id + check_kind
    let run_check =
        boardflow_db::queries::run_check::find_by_board_run_and_kind(&pool, br_id, &check_kind)
            .await
            .map_err(|e| {
                tracing::error!("list_findings run_check lookup failed: {e}");
                AppError::internal_error("database error", &request_id)
            })?;

    // If run_check not found, return empty list (not 404)
    let run_check = match run_check {
        Some(rc) => rc,
        None => {
            return Ok(Json(PaginatedResponse {
                items: vec![],
                next_cursor: None,
                has_more: false,
            }));
        }
    };

    // 7. Query findings with pagination + severity filter
    let rows = boardflow_db::queries::run_check_finding::list_by_run_check_id(
        &pool,
        run_check.id,
        limit + 1,
        cursor,
        params.severity.as_deref(),
    )
    .await
    .map_err(|e| {
        tracing::error!("list_findings query failed: {e}");
        AppError::internal_error("database error", &request_id)
    })?;

    // 8. Build response with cursor
    let has_more = rows.len() as i64 > limit;
    let items: Vec<_> = rows
        .iter()
        .take(limit as usize)
        .map(|f| {
            let pos_mm = match (f.x_um, f.y_um) {
                (Some(x), Some(y)) => Some(CoordinateMmResponse {
                    x: x as f64 / 1000.0,
                    y: y as f64 / 1000.0,
                }),
                _ => None,
            };

            let severity_str = match f.severity {
                FindingSeverity::Error => "error",
                FindingSeverity::Warning => "warning",
                FindingSeverity::Notice => "notice",
            };

            let subject_kind_str = f.subject_kind.map(|sk| match sk {
                SubjectKind::Schematic => "schematic",
                SubjectKind::Pcb => "pcb",
                SubjectKind::Net => "net",
                SubjectKind::Footprint => "footprint",
                SubjectKind::Symbol => "symbol",
            });

            FindingListItem {
                id: f.id.to_string(),
                severity: severity_str.to_string(),
                rule_code: f.rule_code.clone(),
                title: f.title.clone(),
                message: f.message.clone(),
                subject_kind: subject_kind_str.map(|s| s.to_string()),
                subject_ref: f.subject_ref.clone(),
                sheet_path: f.sheet_path.clone(),
                pcb_layer: f.pcb_layer.clone(),
                pos_mm,
            }
        })
        .collect();

    let next_cursor = if has_more {
        let last = &rows[limit as usize - 1];
        Some(encode_findings_cursor(last.sort_index, &last.id))
    } else {
        None
    };

    Ok(Json(PaginatedResponse {
        items,
        next_cursor,
        has_more,
    }))
}
