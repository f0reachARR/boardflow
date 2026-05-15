use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};

use boardflow_domain::models::run_check::{CheckKind, FindingSeverity, SubjectKind};
use boardflow_domain::public_ids::BoardRunId;

use crate::error::{AppError, RequestId};
use crate::extractors::AuthenticatedSession;
use crate::github_access::DynGithubAccessChecker;
use crate::pagination::{PaginatedResponse, decode_findings_cursor, encode_findings_cursor};

use crate::github_access::access_result_to_error;

fn check_kind_str(kind: CheckKind) -> &'static str {
    match kind {
        CheckKind::Erc => "erc",
        CheckKind::Drc => "drc",
    }
}

fn parse_check_kind(value: &str) -> Option<CheckKind> {
    match value {
        "erc" => Some(CheckKind::Erc),
        "drc" => Some(CheckKind::Drc),
        _ => None,
    }
}

fn finding_severity_str(severity: FindingSeverity) -> &'static str {
    match severity {
        FindingSeverity::Error => "error",
        FindingSeverity::Warning => "warning",
        FindingSeverity::Notice => "notice",
    }
}

fn parse_finding_severity(value: &str) -> Option<FindingSeverity> {
    match value {
        "error" => Some(FindingSeverity::Error),
        "warning" => Some(FindingSeverity::Warning),
        "notice" => Some(FindingSeverity::Notice),
        _ => None,
    }
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
    pub severity: FindingSeverity,
    pub rule_code: Option<String>,
    pub title: Option<String>,
    pub message: Option<String>,
    pub subject_kind: Option<SubjectKind>,
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
    let br_id = board_run_id
        .parse::<BoardRunId>()
        .map(BoardRunId::into_uuid)
        .map_err(|_| AppError::validation_failed("invalid board_run_id format", &request_id))?;

    // 2. Validate check_kind
    let check_kind = if let Some(check_kind) = parse_check_kind(&check_kind) {
        check_kind
    } else {
        return Err(AppError::validation_failed(
            "check_kind must be 'erc' or 'drc'",
            &request_id,
        ));
    };

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
    let severity = match params.severity.as_deref() {
        Some(sev) => Some(parse_finding_severity(sev).ok_or_else(|| {
            AppError::validation_failed(
                "severity must be 'error', 'warning', or 'notice'",
                &request_id,
            )
        })?),
        None => None,
    };

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
    let run_check = boardflow_db::queries::run_check::find_by_board_run_and_kind(
        &pool,
        br_id,
        check_kind_str(check_kind),
    )
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
        severity.map(finding_severity_str),
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

            FindingListItem {
                id: f.id.to_string(),
                severity: f.severity,
                rule_code: f.rule_code.clone(),
                title: f.title.clone(),
                message: f.message.clone(),
                subject_kind: f.subject_kind,
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
