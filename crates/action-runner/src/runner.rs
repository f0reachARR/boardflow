use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tracing::{error, info, warn};

use boardflow_kicad::cli::{KicadCli, PcbSide};
use boardflow_kicad::config::{self, BoardflowConfig};
use boardflow_kicad::detect;
use boardflow_kicad::hash;

use crate::api::{ApiClient, PlanFile, PlanProject};
use crate::bundle;
use crate::error::ActionError;
use crate::inputs::{self, ActionInputs, GitHubContext};
use crate::summary::{self, ProjectResult};

struct ValidProject {
    project_dir: PathBuf,
    pro_file: PathBuf,
    pcb_file: PathBuf,
    sch_file: PathBuf,
    excludes: Vec<String>,
    #[allow(dead_code)]
    config: BoardflowConfig,
    rel_dir: String,
    rel_pro_path: String,
}

#[derive(Serialize)]
struct ArtifactEntry {
    #[serde(rename = "type")]
    artifact_type: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_message: Option<String>,
}

impl ArtifactEntry {
    fn available(
        artifact_type: &str,
        path: &str,
        content_type: &str,
        file_path: &Path,
    ) -> Self {
        let sha256 = hash::compute_file_sha256(file_path).ok();
        let size_bytes = fs::metadata(file_path).ok().map(|m| m.len());
        Self {
            artifact_type: artifact_type.to_string(),
            status: "available".to_string(),
            path: Some(path.to_string()),
            source_path: None,
            content_type: Some(content_type.to_string()),
            sha256: sha256.map(|h| format!("sha256:{h}")),
            size_bytes,
            error_message: None,
        }
    }

    fn failed(artifact_type: &str, message: &str) -> Self {
        Self {
            artifact_type: artifact_type.to_string(),
            status: "failed".to_string(),
            path: None,
            source_path: None,
            content_type: None,
            sha256: None,
            size_bytes: None,
            error_message: Some(message.to_string()),
        }
    }

    fn source(artifact_type: &str, staging_path: &str, source_path: &str, file_path: &Path) -> Self {
        let sha256 = hash::compute_file_sha256(file_path).ok();
        let size_bytes = fs::metadata(file_path).ok().map(|m| m.len());
        Self {
            artifact_type: artifact_type.to_string(),
            status: "available".to_string(),
            path: Some(staging_path.to_string()),
            source_path: Some(source_path.to_string()),
            content_type: None,
            sha256: sha256.map(|h| format!("sha256:{h}")),
            size_bytes,
            error_message: None,
        }
    }
}

pub async fn run() -> i32 {
    // 1. Parse inputs
    let action_inputs = match inputs::parse_inputs() {
        Ok(i) => i,
        Err(e) => {
            summary::error(&e.to_string());
            return 1;
        }
    };

    let gh = match inputs::parse_github_context() {
        Ok(g) => g,
        Err(e) => {
            summary::error(&format!("Failed to parse GitHub context: {e}"));
            return 1;
        }
    };

    info!(
        "BoardFlow Action: repo={}/{} sha={} event={}",
        gh.owner, gh.repo_name, gh.sha, gh.event_name
    );

    // 2. Check unsupported events
    if gh.event_name == "pull_request" {
        let _ = summary::write_unsupported_event_summary(&gh.event_name, &gh.summary_path);
        let _ = summary::set_output(
            "result",
            r#"{"status":"skipped","reason":"unsupported event: pull_request"}"#,
            &gh.output_path,
        );
        return 0;
    }

    // 3. Detect projects
    let yml_paths = match detect::find_boardflow_ymls(&gh.workspace) {
        Ok(p) => p,
        Err(_) => {
            summary::error("No .boardflow.yml found in workspace");
            return 1;
        }
    };

    // 4. Validate projects
    let mut valid_projects: Vec<ValidProject> = Vec::new();
    let mut detection_errors = 0u32;

    for yml_path in &yml_paths {
        let project_dir = match yml_path.parent() {
            Some(d) => d.to_path_buf(),
            None => {
                detection_errors += 1;
                continue;
            }
        };

        let cfg = match config::parse_boardflow_yml(yml_path) {
            Ok(c) => c,
            Err(e) => {
                summary::warning(&format!("Failed to parse {}: {e}", yml_path.display()));
                detection_errors += 1;
                continue;
            }
        };

        if let Err(e) = config::validate_schema_v1(&cfg) {
            summary::warning(&format!("Invalid schema in {}: {e}", yml_path.display()));
            detection_errors += 1;
            continue;
        }

        // Merge excludes (action.yml specifies newline-separated patterns)
        let input_excludes: Vec<String> = action_inputs
            .exclude_paths
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let excludes = config::merge_excludes(
            hash::BUILTIN_EXCLUDES,
            &input_excludes,
            &cfg.exclude_paths,
        );

        let pro_file = match detect::resolve_kicad_pro(&project_dir) {
            Ok(p) => p,
            Err(e) => {
                summary::warning(&format!(
                    "No unique .kicad_pro in {}: {e}",
                    project_dir.display()
                ));
                detection_errors += 1;
                continue;
            }
        };

        let pro_stem = pro_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();

        let pcb_file = match detect::resolve_pcb_file(&project_dir, &pro_stem) {
            Ok(p) => p,
            Err(e) => {
                summary::warning(&format!(
                    "No .kicad_pcb for {pro_stem} in {}: {e}",
                    project_dir.display()
                ));
                detection_errors += 1;
                continue;
            }
        };

        let sch_file = match detect::resolve_root_schematic(&project_dir, &pro_stem) {
            Ok(p) => p,
            Err(e) => {
                summary::warning(&format!(
                    "No .kicad_sch for {pro_stem} in {}: {e}",
                    project_dir.display()
                ));
                detection_errors += 1;
                continue;
            }
        };

        // Validate required files are not excluded
        let pro_rel = pro_file.file_name().unwrap_or_default().to_str().unwrap_or_default();
        let pcb_rel = pcb_file.file_name().unwrap_or_default().to_str().unwrap_or_default();
        let sch_rel = sch_file.file_name().unwrap_or_default().to_str().unwrap_or_default();
        if hash::is_excluded(pro_rel, &excludes)
            || hash::is_excluded(pcb_rel, &excludes)
            || hash::is_excluded(sch_rel, &excludes)
        {
            summary::warning(&format!(
                "Required files excluded in {}",
                project_dir.display()
            ));
            detection_errors += 1;
            continue;
        }

        // Compute relative paths
        let rel_dir = project_dir
            .strip_prefix(&gh.workspace)
            .map(|p| p.to_str().unwrap_or("."))
            .unwrap_or(".")
            .to_string();
        let rel_dir = if rel_dir.is_empty() { ".".to_string() } else { rel_dir };

        let rel_pro_path = pro_file
            .strip_prefix(&gh.workspace)
            .map(|p| p.to_str().unwrap_or_default().to_string())
            .unwrap_or_default();

        valid_projects.push(ValidProject {
            project_dir,
            pro_file,
            pcb_file,
            sch_file,
            excludes,
            config: cfg,
            rel_dir,
            rel_pro_path,
        });
    }

    if valid_projects.is_empty() {
        summary::error("No valid projects found");
        return 1;
    }

    // 5. Compute hashes and build plan payload
    let mut plan_projects = Vec::new();
    for vp in &valid_projects {
        let tree_hash = match hash::compute_tree_hash(&vp.project_dir, &vp.excludes) {
            Ok(h) => h,
            Err(e) => {
                summary::warning(&format!("Failed to compute tree hash for {}: {e}", vp.rel_dir));
                continue;
            }
        };

        let files = build_plan_files(&vp.project_dir, &vp.excludes);
        let yml_rel = if vp.rel_dir == "." {
            ".boardflow.yml".to_string()
        } else {
            format!("{}/.boardflow.yml", vp.rel_dir)
        };

        plan_projects.push(PlanProject {
            project_path: vp.rel_pro_path.clone(),
            config_path: yml_rel,
            project_dir: vp.rel_dir.clone(),
            tree_hash: format!("sha256:{tree_hash}"),
            files,
        });
    }

    let plan_payload = serde_json::json!({
        "repository": { "owner": gh.owner, "name": gh.repo_name },
        "git": {
            "ref": gh.git_ref,
            "branch": gh.ref_name,
            "commit_sha": gh.sha,
            "event_name": gh.event_name,
        },
        "action": {
            "workflow": "BoardFlow",
            "run_id": gh.run_id,
            "run_attempt": gh.run_attempt,
        },
        "mode": action_inputs.mode,
        "projects": plan_projects,
    });

    // 6. Call plan API
    let api = ApiClient::new(&action_inputs.api_url, &action_inputs.token);
    let decisions = match api.plan(&plan_payload).await {
        Ok(d) => d,
        Err(e) => {
            summary::error(&format!("Plan API call failed: {e}"));
            return 1;
        }
    };

    // 7. Process each project with decision "build"
    let kicad = KicadCli::with_bin_path("/usr/bin/kicad-cli");
    let mut exit_code = 0i32;
    let mut results: Vec<ProjectResult> = Vec::new();

    for vp in &valid_projects {
        let decision = decisions
            .iter()
            .find(|d| d.project_path == vp.rel_pro_path)
            .map(|d| d.decision.as_str())
            .unwrap_or("skip");

        if decision != "build" {
            results.push(ProjectResult {
                path: vp.rel_pro_path.clone(),
                status: "skipped".to_string(),
                error: None,
            });
            continue;
        }

        let board_project_id = decisions
            .iter()
            .find(|d| d.project_path == vp.rel_pro_path)
            .and_then(|d| d.board_project_id.clone())
            .unwrap_or_default();

        match process_project(&kicad, &api, vp, &gh, &action_inputs, &board_project_id).await {
            Ok(checks_failed) => {
                results.push(ProjectResult {
                    path: vp.rel_pro_path.clone(),
                    status: "success".to_string(),
                    error: None,
                });
                // fail-on-erc/fail-on-drc: after successful upload/import, fail the job
                if checks_failed {
                    exit_code = 1;
                }
            }
            Err(e) => {
                error!("Project {} failed: {e}", vp.rel_pro_path);
                results.push(ProjectResult {
                    path: vp.rel_pro_path.clone(),
                    status: "error".to_string(),
                    error: Some(e.to_string()),
                });
                exit_code = 1;
            }
        }
    }

    // 8. Write summary
    let _ = summary::write_job_summary(&results, &gh.summary_path);
    let results_json = serde_json::to_string(&results.iter().map(|r| {
        serde_json::json!({ "path": r.path, "status": r.status, "error": r.error })
    }).collect::<Vec<_>>()).unwrap_or_default();
    let _ = summary::set_output("result", &results_json, &gh.output_path);

    // 9. Return exit code
    if detection_errors > 0 {
        exit_code = 1;
    }
    exit_code
}

async fn process_project(
    kicad: &KicadCli,
    api: &ApiClient,
    vp: &ValidProject,
    gh: &GitHubContext,
    inputs: &ActionInputs,
    board_project_id: &str,
) -> std::result::Result<bool, ActionError> {
    let pro_stem = vp
        .pro_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();

    // Create temp output directory
    let output_dir = tempfile::Builder::new()
        .prefix(&format!("boardflow-{pro_stem}-"))
        .tempdir()
        .map_err(|e| ActionError::Io(e))?;
    let output_path = output_dir.path();

    // Create board run
    let tree_hash = hash::compute_tree_hash(&vp.project_dir, &vp.excludes)
        .map_err(|e| ActionError::Bundle(format!("tree hash: {e}")))?;

    let create_payload = serde_json::json!({
        "board_project_id": board_project_id,
        "project_path": vp.rel_pro_path,
        "tree_hash": format!("sha256:{tree_hash}"),
        "commit_sha": gh.sha,
        "branch": gh.ref_name,
        "ref": gh.git_ref,
        "github_run_id": gh.run_id,
        "github_run_attempt": gh.run_attempt,
    });

    let create_resp = api.create_board_run(&create_payload).await?;
    let board_run_id = &create_resp.board_run_id;
    let upload_url = &create_resp.artifact_bundle.upload_url;
    let staging_object_key = &create_resp.artifact_bundle.object_key;

    let mut artifacts: Vec<serde_json::Value> = Vec::new();
    let mut checks_failed = false;

    // Run ERC
    let erc_json = output_path.join("erc.json");
    match kicad.run_erc(&vp.sch_file, &erc_json).await {
        Ok(cmd_out) => {
            if cmd_out.exit_code == 0 || cmd_out.exit_code == 5 {
                artifacts.push(serde_json::to_value(ArtifactEntry::available(
                    "erc_report", "checks/erc.json", "application/json", &erc_json,
                )).unwrap());
                if cmd_out.exit_code == 5 && inputs.fail_on_erc {
                    checks_failed = true;
                }
            } else {
                artifacts.push(serde_json::to_value(ArtifactEntry::failed(
                    "erc_report", "ERC execution failed",
                )).unwrap());
            }
        }
        Err(e) => {
            warn!("ERC failed: {e}");
            artifacts.push(serde_json::to_value(ArtifactEntry::failed(
                "erc_report", &format!("ERC execution failed: {e}"),
            )).unwrap());
        }
    }

    // Run DRC
    let drc_json = output_path.join("drc.json");
    match kicad.run_drc(&vp.pcb_file, &drc_json).await {
        Ok(cmd_out) => {
            if cmd_out.exit_code == 0 || cmd_out.exit_code == 5 {
                artifacts.push(serde_json::to_value(ArtifactEntry::available(
                    "drc_report", "checks/drc.json", "application/json", &drc_json,
                )).unwrap());
                if cmd_out.exit_code == 5 && inputs.fail_on_drc {
                    checks_failed = true;
                }
            } else {
                artifacts.push(serde_json::to_value(ArtifactEntry::failed(
                    "drc_report", "DRC execution failed",
                )).unwrap());
            }
        }
        Err(e) => {
            warn!("DRC failed: {e}");
            artifacts.push(serde_json::to_value(ArtifactEntry::failed(
                "drc_report", &format!("DRC execution failed: {e}"),
            )).unwrap());
        }
    }

    // Export PCB PDF
    let pdf_dir = output_path.join("pdf");
    fs::create_dir_all(&pdf_dir)?;
    match kicad.export_pcb_pdf(&vp.pcb_file, &pdf_dir.join("pcb.pdf")).await {
        Ok(_) => {
            artifacts.push(serde_json::to_value(ArtifactEntry::available(
                "pcb_pdf", "review/pcb.pdf", "application/pdf", &pdf_dir.join("pcb.pdf"),
            )).unwrap());
        }
        Err(e) => {
            warn!("PCB PDF failed: {e}");
            artifacts.push(serde_json::to_value(ArtifactEntry::failed(
                "pcb_pdf", "PCB PDF export failed",
            )).unwrap());
        }
    }

    // Export Schematic PDF
    match kicad.export_sch_pdf(&vp.sch_file, &pdf_dir.join("schematic.pdf")).await {
        Ok(_) => {
            artifacts.push(serde_json::to_value(ArtifactEntry::available(
                "schematic_pdf", "review/schematic.pdf", "application/pdf", &pdf_dir.join("schematic.pdf"),
            )).unwrap());
        }
        Err(e) => {
            warn!("Schematic PDF failed: {e}");
            artifacts.push(serde_json::to_value(ArtifactEntry::failed(
                "schematic_pdf", "Schematic PDF export failed",
            )).unwrap());
        }
    }

    // Export SVG
    let svg_dir = output_path.join("svg");
    fs::create_dir_all(&svg_dir)?;
    match kicad.export_pcb_svg(&vp.pcb_file, &svg_dir.join("pcb_top.svg"), PcbSide::Top).await {
        Ok(_) => {
            artifacts.push(serde_json::to_value(ArtifactEntry::available(
                "pcb_top_svg", "review/pcb_top.svg", "image/svg+xml", &svg_dir.join("pcb_top.svg"),
            )).unwrap());
        }
        Err(e) => {
            warn!("PCB top SVG failed: {e}");
            artifacts.push(serde_json::to_value(ArtifactEntry::failed(
                "pcb_top_svg", "PCB top SVG export failed",
            )).unwrap());
        }
    }

    match kicad.export_pcb_svg(&vp.pcb_file, &svg_dir.join("pcb_bottom.svg"), PcbSide::Bottom).await {
        Ok(_) => {
            artifacts.push(serde_json::to_value(ArtifactEntry::available(
                "pcb_bottom_svg", "review/pcb_bottom.svg", "image/svg+xml", &svg_dir.join("pcb_bottom.svg"),
            )).unwrap());
        }
        Err(e) => {
            warn!("PCB bottom SVG failed: {e}");
            artifacts.push(serde_json::to_value(ArtifactEntry::failed(
                "pcb_bottom_svg", "PCB bottom SVG export failed",
            )).unwrap());
        }
    }

    // Export Gerber
    let gerber_dir = output_path.join("gerber");
    fs::create_dir_all(&gerber_dir)?;
    let gerber_ok = kicad.export_gerbers(&vp.pcb_file, &gerber_dir).await.is_ok();

    // Export Drill
    let drill_dir = output_path.join("drill");
    fs::create_dir_all(&drill_dir)?;
    let drill_ok = kicad.export_drill(&vp.pcb_file, &drill_dir).await.is_ok();

    // Create zip archives
    let gerbers_zip = output_path.join("gerbers.zip");
    if gerber_ok {
        if bundle::create_bundle_zip(&gerber_dir, &gerbers_zip).is_ok() {
            artifacts.push(serde_json::to_value(ArtifactEntry::available(
                "gerber_zip", "fabrication/gerbers.zip", "application/zip", &gerbers_zip,
            )).unwrap());
        } else {
            artifacts.push(serde_json::to_value(ArtifactEntry::failed(
                "gerber_zip", "Gerber zip creation failed",
            )).unwrap());
        }
    } else {
        artifacts.push(serde_json::to_value(ArtifactEntry::failed(
            "gerber_zip", "Gerber export failed",
        )).unwrap());
    }

    let drill_zip = output_path.join("drill.zip");
    if drill_ok {
        if bundle::create_bundle_zip(&drill_dir, &drill_zip).is_ok() {
            artifacts.push(serde_json::to_value(ArtifactEntry::available(
                "drill_zip", "fabrication/drill.zip", "application/zip", &drill_zip,
            )).unwrap());
        } else {
            artifacts.push(serde_json::to_value(ArtifactEntry::failed(
                "drill_zip", "Drill zip creation failed",
            )).unwrap());
        }
    } else {
        artifacts.push(serde_json::to_value(ArtifactEntry::failed(
            "drill_zip", "Drill export failed",
        )).unwrap());
    }

    // Fabrication zip (combined)
    let fab_zip = output_path.join("fabrication.zip");
    if gerber_ok || drill_ok {
        if bundle::create_fabrication_zip(&gerber_dir, &drill_dir, &fab_zip).is_ok() {
            artifacts.push(serde_json::to_value(ArtifactEntry::available(
                "fabrication_zip", "fabrication/fabrication.zip", "application/zip", &fab_zip,
            )).unwrap());
        } else {
            artifacts.push(serde_json::to_value(ArtifactEntry::failed(
                "fabrication_zip", "Fabrication zip creation failed",
            )).unwrap());
        }
    } else {
        artifacts.push(serde_json::to_value(ArtifactEntry::failed(
            "fabrication_zip", "Fabrication zip creation failed",
        )).unwrap());
    }

    // Export BOM
    let bom_dir = output_path.join("bom");
    fs::create_dir_all(&bom_dir)?;
    match kicad.export_bom(&vp.sch_file, &bom_dir.join("bom.csv")).await {
        Ok(_) => {
            artifacts.push(serde_json::to_value(ArtifactEntry::available(
                "bom_csv", "assembly/bom.csv", "text/csv", &bom_dir.join("bom.csv"),
            )).unwrap());
        }
        Err(e) => {
            warn!("BOM export failed: {e}");
            artifacts.push(serde_json::to_value(ArtifactEntry::failed(
                "bom_csv", "BOM export failed",
            )).unwrap());
        }
    }

    // Export Position
    let pos_dir = output_path.join("position");
    fs::create_dir_all(&pos_dir)?;
    match kicad.export_position(&vp.pcb_file, &pos_dir.join("position.csv")).await {
        Ok(_) => {
            artifacts.push(serde_json::to_value(ArtifactEntry::available(
                "position_csv", "assembly/position.csv", "text/csv", &pos_dir.join("position.csv"),
            )).unwrap());
        }
        Err(e) => {
            warn!("Position export failed: {e}");
            artifacts.push(serde_json::to_value(ArtifactEntry::failed(
                "position_csv", "Position export failed",
            )).unwrap());
        }
    }

    // 3D Renders
    let render_dir = output_path.join("3d");
    fs::create_dir_all(&render_dir)?;
    match kicad.render_3d(&vp.pcb_file, &render_dir.join("top.png"), PcbSide::Top).await {
        Ok(_) => {
            artifacts.push(serde_json::to_value(ArtifactEntry::available(
                "render_top_png", "review/render_top.png", "image/png", &render_dir.join("top.png"),
            )).unwrap());
        }
        Err(e) => {
            warn!("3D top render failed: {e}");
            artifacts.push(serde_json::to_value(ArtifactEntry::failed(
                "render_top_png", "3D top render failed",
            )).unwrap());
        }
    }

    match kicad.render_3d(&vp.pcb_file, &render_dir.join("bottom.png"), PcbSide::Bottom).await {
        Ok(_) => {
            artifacts.push(serde_json::to_value(ArtifactEntry::available(
                "render_bottom_png", "review/render_bottom.png", "image/png", &render_dir.join("bottom.png"),
            )).unwrap());
        }
        Err(e) => {
            warn!("3D bottom render failed: {e}");
            artifacts.push(serde_json::to_value(ArtifactEntry::failed(
                "render_bottom_png", "3D bottom render failed",
            )).unwrap());
        }
    }

    // iBOM
    let ibom_dir = output_path.join("ibom");
    fs::create_dir_all(&ibom_dir)?;
    match boardflow_kicad::ibom::run_ibom(&vp.pcb_file, &ibom_dir).await {
        Ok(html_path) => {
            // Copy to expected staging name
            let dest = ibom_dir.join("ibom.html");
            if html_path != dest {
                let _ = fs::copy(&html_path, &dest);
            }
            artifacts.push(serde_json::to_value(ArtifactEntry::available(
                "ibom", "assembly/ibom.html", "text/html", &dest,
            )).unwrap());
        }
        Err(e) => {
            warn!("iBOM failed: {e}");
            artifacts.push(serde_json::to_value(ArtifactEntry::failed(
                "ibom", "iBOM generation failed",
            )).unwrap());
        }
    }

    // KiCad source files as artifacts
    if let Ok(entries) = fs::read_dir(&vp.project_dir) {
        let mut src_entries: Vec<_> = entries
            .flatten()
            .filter(|e| {
                e.path().is_file()
                    && matches!(
                        e.path().extension().and_then(|e| e.to_str()),
                        Some("kicad_pro" | "kicad_sch" | "kicad_pcb" | "kicad_wks")
                    )
            })
            .collect();
        src_entries.sort_by_key(|e| e.path());

        for entry in src_entries {
            let path = entry.path();
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let src_rel = file_name.to_string();

            if hash::is_excluded(&src_rel, &vp.excludes) {
                continue;
            }

            let kicad_type = match path.extension().and_then(|e| e.to_str()) {
                Some("kicad_pro") => "kicad_project",
                Some("kicad_sch") => "kicad_schematic",
                Some("kicad_pcb") => "kicad_pcb",
                Some("kicad_wks") => "kicad_worksheet",
                _ => continue,
            };

            let (staging_path, source_path) = if vp.rel_dir == "." {
                (format!("kicad/{src_rel}"), src_rel.clone())
            } else {
                (
                    format!("kicad/{}/{src_rel}", vp.rel_dir),
                    format!("{}/{src_rel}", vp.rel_dir),
                )
            };

            artifacts.push(
                serde_json::to_value(ArtifactEntry::source(
                    kicad_type, &staging_path, &source_path, &path,
                ))
                .unwrap(),
            );
        }
    }

    // Generate diff metadata
    let diff_dir = output_path.join("diff");
    fs::create_dir_all(&diff_dir)?;
    let _ = bundle::generate_file_hashes_json(&vp.project_dir, &vp.excludes, &diff_dir.join("file_hashes.json"));
    let _ = bundle::generate_bom_summary_json(&bom_dir.join("bom.csv"), &diff_dir.join("bom_summary.json"));
    let _ = bundle::generate_checks_summary_json(&erc_json, &drc_json, &diff_dir.join("checks_summary.json"));
    let _ = bundle::generate_artifacts_summary_json(&artifacts, &diff_dir.join("artifacts_summary.json"));
    let _ = bundle::generate_previews_json(output_path, &diff_dir.join("previews.json"));

    // Create manifest
    let manifest_path = output_path.join("manifest.json");
    bundle::create_manifest(
        &vp.rel_pro_path,
        &format!("sha256:{tree_hash}"),
        &gh.sha,
        &diff_dir.join("checks_summary.json"),
        &artifacts,
        &manifest_path,
    )?;

    // Build staging directory
    let staging_dir = bundle::build_staging_dir(
        output_path,
        &vp.project_dir,
        &vp.rel_dir,
        &vp.excludes,
        &manifest_path,
    )?;

    // Create bundle zip
    let bundle_path = output_path.join("bundle.zip");
    if let Err(e) = bundle::create_bundle_zip(&staging_dir, &bundle_path) {
        let _ = api.fail(board_run_id, "Bundle creation failed", &e.to_string()).await;
        return Err(ActionError::Bundle(format!("Bundle creation failed: {e}")));
    }

    let bundle_sha256 = bundle::compute_bundle_sha256(&bundle_path)?;

    // Upload bundle
    if let Err(e) = api.upload_bundle(upload_url, &bundle_path).await {
        let _ = api.fail(board_run_id, "Upload failed", &e.to_string()).await;
        return Err(e);
    }

    // Import
    let bundle_size = fs::metadata(&bundle_path)?.len();
    let import_payload = serde_json::json!({
        "staging_object_key": staging_object_key,
        "bundle_sha256": format!("sha256:{bundle_sha256}"),
        "bundle_size_bytes": bundle_size,
    });

    if let Err(e) = api.import(board_run_id, &import_payload).await {
        let _ = api.fail(board_run_id, "Import failed", &e.to_string()).await;
        return Err(e);
    }

    info!("Successfully processed project: {}", vp.rel_pro_path);
    Ok(checks_failed)
}

fn build_plan_files(project_dir: &Path, excludes: &[String]) -> Vec<PlanFile> {
    let files = match hash::list_project_files(project_dir, excludes) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let mut entries: Vec<(String, PathBuf)> = files
        .into_iter()
        .filter_map(|abs_path| {
            let rel = abs_path.strip_prefix(project_dir).ok()?;
            Some((rel.to_str()?.to_string(), abs_path))
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    entries
        .into_iter()
        .filter_map(|(rel_path, abs_path)| {
            let file_hash = hash::compute_file_sha256(&abs_path).ok()?;
            Some(PlanFile {
                path: rel_path,
                sha256: format!("sha256:{file_hash}"),
            })
        })
        .collect()
}
