use axum::extract::{Path, State};
use axum::{Extension, Json};
use chrono::Utc;
use sqlx::PgPool;

use boardflow_domain::models::artifact::{Artifact, ArtifactStatus, ArtifactType};
use boardflow_domain::public_ids::{ArtifactId, BoardRunId};

use crate::error::{AppError, RequestId};
use crate::extractors::AuthenticatedSession;
use crate::github_access::DynGithubAccessChecker;
use crate::{ArtifactBaseUrl, ArtifactSecret};

use super::dto::{
    ViewerAvailabilityStatus, ViewerDownload, ViewerMap, ViewerSource, ViewerSourceKind,
    ViewerSourcesResponse, ViewerStatus,
};
use crate::services::authz::ensure_board_run_access;

fn find_artifact(artifacts: &[Artifact], artifact_type: ArtifactType) -> Option<&Artifact> {
    artifacts
        .iter()
        .find(|artifact| artifact.r#type == artifact_type)
}

fn find_artifacts(artifacts: &[Artifact], artifact_type: ArtifactType) -> Vec<&Artifact> {
    artifacts
        .iter()
        .filter(|artifact| artifact.r#type == artifact_type)
        .collect()
}

fn single_viewer_status(artifact: Option<&Artifact>) -> ViewerAvailabilityStatus {
    match artifact {
        Some(a) if a.status == ArtifactStatus::Available => ViewerAvailabilityStatus::Available,
        Some(a) if a.status == ArtifactStatus::Failed => ViewerAvailabilityStatus::Failed,
        Some(a) if a.status == ArtifactStatus::Skipped => ViewerAvailabilityStatus::Skipped,
        _ => ViewerAvailabilityStatus::Missing,
    }
}

fn viewer_status(
    available_count: usize,
    required_count: usize,
    artifacts: &[Option<&Artifact>],
) -> ViewerAvailabilityStatus {
    if available_count == required_count {
        ViewerAvailabilityStatus::Available
    } else if available_count > 0 {
        ViewerAvailabilityStatus::Partial
    } else {
        // Check if all are skipped
        let all_skipped = artifacts
            .iter()
            .all(|a| a.is_some_and(|art| art.status == ArtifactStatus::Skipped));
        if all_skipped && artifacts.iter().any(|a| a.is_some()) {
            return ViewerAvailabilityStatus::Skipped;
        }
        // Check if any are failed
        let has_failed = artifacts
            .iter()
            .any(|a| a.is_some_and(|art| art.status == ArtifactStatus::Failed));
        if has_failed {
            ViewerAvailabilityStatus::Failed
        } else {
            ViewerAvailabilityStatus::Missing
        }
    }
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
    let id = board_run_id
        .parse::<BoardRunId>()
        .map(BoardRunId::into_uuid)
        .map_err(|_| AppError::validation_failed("invalid board_run_id format", &request_id))?;

    // Check repository access via board_run → board_project → repository
    ensure_board_run_access(
        &pool,
        &access_checker,
        &session.user.github_access_token,
        id,
        &request_id,
    )
    .await?;

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

    let user_id = session.user.id;
    let secret = &artifact_secret.0;
    let proxy_url = |a: &Artifact| -> String {
        let token = crate::artifact_token::generate_artifact_token(a.id, user_id, secret);
        format!(
            "{}/proxy/artifacts/{}?token={}",
            artifact_base_url.0,
            ArtifactId::from(a.id),
            token
        )
    };
    let proxy_url_with_filename = |a: &Artifact| -> String {
        let token = crate::artifact_token::generate_artifact_token(a.id, user_id, secret);
        match a.filename.as_deref() {
            Some(filename) => format!(
                "{}/proxy/artifacts/{}/{}?token={}",
                artifact_base_url.0,
                ArtifactId::from(a.id),
                urlencoding::encode(filename),
                token
            ),
            None => proxy_url(a),
        }
    };

    // KiCanvas viewer: needs kicad_pro, kicad_sch, kicad_pcb
    let kicanvas = {
        let pro = find_artifacts(&artifacts, ArtifactType::KicadPro);
        let sch = find_artifacts(&artifacts, ArtifactType::KicadSch);
        let pcb = find_artifacts(&artifacts, ArtifactType::KicadPcb);
        let groups = [&pro, &sch, &pcb];
        let available_count = groups
            .iter()
            .filter(|group| {
                group
                    .iter()
                    .any(|artifact| artifact.status == ArtifactStatus::Available)
            })
            .count();
        let all_skipped = groups.iter().all(|group| {
            !group.is_empty()
                && group
                    .iter()
                    .all(|artifact| artifact.status == ArtifactStatus::Skipped)
        });
        let has_failed = groups.iter().any(|group| {
            group
                .iter()
                .any(|artifact| artifact.status == ArtifactStatus::Failed)
        });
        let status = if available_count == 3 {
            ViewerAvailabilityStatus::Available
        } else if available_count > 0 {
            ViewerAvailabilityStatus::Partial
        } else if all_skipped {
            ViewerAvailabilityStatus::Skipped
        } else if has_failed {
            ViewerAvailabilityStatus::Failed
        } else {
            ViewerAvailabilityStatus::Missing
        };

        let sources: Vec<_> = artifacts
            .iter()
            .filter_map(|artifact| {
                let kind = match artifact.r#type {
                    ArtifactType::KicadPro => ViewerSourceKind::Project,
                    ArtifactType::KicadSch => ViewerSourceKind::Schematic,
                    ArtifactType::KicadPcb => ViewerSourceKind::Board,
                    _ => return None,
                };

                if artifact.status != ArtifactStatus::Available {
                    return None;
                }

                Some(ViewerSource {
                    artifact_id: Some(ArtifactId::from(artifact.id)),
                    artifact_type: None,
                    kind: Some(kind),
                    name: artifact.filename.clone(),
                    source_path: artifact.source_path.clone(),
                    url: Some(proxy_url_with_filename(artifact)),
                })
            })
            .collect();

        ViewerStatus {
            status,
            sources: (!sources.is_empty()).then_some(sources),
            primary: None,
            iframe_url: None,
            downloads: None,
        }
    };

    // Schematic viewer: needs schematic_pdf
    let schematic = {
        let pdf = find_artifact(&artifacts, ArtifactType::SchematicPdf);
        let status = single_viewer_status(pdf);
        let primary = pdf
            .filter(|a| a.status == ArtifactStatus::Available)
            .map(|a| ViewerSource {
                artifact_id: Some(ArtifactId::from(a.id)),
                artifact_type: Some(ArtifactType::SchematicPdf),
                kind: None,
                name: None,
                source_path: None,
                url: Some(proxy_url(a)),
            });
        ViewerStatus {
            status,
            sources: None,
            primary,
            iframe_url: None,
            downloads: None,
        }
    };

    // PCB Preview: needs pcb_top_svg, pcb_bottom_svg
    let pcb_preview = {
        let top = find_artifact(&artifacts, ArtifactType::PcbTopSvg);
        let bottom = find_artifact(&artifacts, ArtifactType::PcbBottomSvg);
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
                    artifact_id: Some(ArtifactId::from(a.id)),
                    artifact_type: Some(ArtifactType::PcbTopSvg),
                    kind: None,
                    name: None,
                    source_path: None,
                    url: Some(proxy_url(a)),
                });
            }
            if let Some(a) = bottom.filter(|a| a.status == ArtifactStatus::Available) {
                srcs.push(ViewerSource {
                    artifact_id: Some(ArtifactId::from(a.id)),
                    artifact_type: Some(ArtifactType::PcbBottomSvg),
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
        let html = find_artifact(&artifacts, ArtifactType::Ibom);
        let status = single_viewer_status(html);
        let iframe_url = html
            .filter(|a| a.status == ArtifactStatus::Available)
            .map(proxy_url);
        ViewerStatus {
            status,
            sources: None,
            primary: None,
            iframe_url,
            downloads: None,
        }
    };

    // BOM viewer: needs bom_csv
    let bom = {
        let csv = find_artifact(&artifacts, ArtifactType::BomCsv);
        let status = single_viewer_status(csv);
        let downloads = csv
            .filter(|a| a.status == ArtifactStatus::Available)
            .map(|a| {
                vec![ViewerDownload {
                    artifact_id: Some(ArtifactId::from(a.id)),
                    artifact_type: ArtifactType::BomCsv,
                    status: ArtifactStatus::Available,
                    url: Some(proxy_url(a)),
                    status_reason: None,
                }]
            });
        ViewerStatus {
            status,
            sources: None,
            primary: None,
            iframe_url: None,
            downloads,
        }
    };

    // Fabrication viewer: needs gerber_zip, drill_zip
    let fabrication = {
        let gerber = find_artifact(&artifacts, ArtifactType::GerberZip);
        let drill = find_artifact(&artifacts, ArtifactType::DrillZip);
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
                    artifact_id: Some(ArtifactId::from(a.id)),
                    artifact_type: ArtifactType::GerberZip,
                    status: ArtifactStatus::Available,
                    url: Some(proxy_url(a)),
                    status_reason: None,
                });
            }
            Some(a) => {
                downloads.push(ViewerDownload {
                    artifact_id: None,
                    artifact_type: ArtifactType::GerberZip,
                    status: a.status,
                    url: None,
                    status_reason: a.status_reason.clone(),
                });
            }
            None => {
                downloads.push(ViewerDownload {
                    artifact_id: None,
                    artifact_type: ArtifactType::GerberZip,
                    status: ArtifactStatus::Missing,
                    url: None,
                    status_reason: None,
                });
            }
        }
        match drill {
            Some(a) if a.status == ArtifactStatus::Available => {
                downloads.push(ViewerDownload {
                    artifact_id: Some(ArtifactId::from(a.id)),
                    artifact_type: ArtifactType::DrillZip,
                    status: ArtifactStatus::Available,
                    url: Some(proxy_url(a)),
                    status_reason: None,
                });
            }
            Some(a) => {
                downloads.push(ViewerDownload {
                    artifact_id: None,
                    artifact_type: ArtifactType::DrillZip,
                    status: a.status,
                    url: None,
                    status_reason: a.status_reason.clone(),
                });
            }
            None => {
                downloads.push(ViewerDownload {
                    artifact_id: None,
                    artifact_type: ArtifactType::DrillZip,
                    status: ArtifactStatus::Missing,
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
        board_run_id: BoardRunId::from(id),
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
