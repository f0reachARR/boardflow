mod artifact_pipeline;
mod manifest_builder;
mod plan;
mod project_discovery;
mod submission;

use boardflow_api_types::plan::PlanDecision;
use boardflow_kicad::cli::KicadCli;
use boardflow_kicad::hash;
use tracing::{error, info};

use crate::api::ApiClient;
use crate::bundle;
use crate::error::ActionError;
use crate::inputs::{self, ActionInputs, GitHubContext};
use crate::summary::{self, ProjectResult};

use project_discovery::ValidProject;

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

    // 3-4. Discover and validate projects
    let (valid_projects, detection_errors) =
        project_discovery::discover_and_validate(&gh.workspace, &action_inputs);

    if valid_projects.is_empty() {
        summary::error("No valid projects found");
        return 1;
    }

    // 5. Compute hashes and build plan payload
    let plan_payload = plan::build_plan_request(&valid_projects, &action_inputs, &gh);

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
        let decision = decisions.iter().find(|d| d.project_path == vp.rel_pro_path);

        let is_build = decision
            .map(|d| matches!(d.decision, PlanDecision::Build))
            .unwrap_or(false);

        if !is_build {
            results.push(ProjectResult {
                path: vp.rel_pro_path.clone(),
                status: "skipped".to_string(),
                error: None,
            });
            continue;
        }

        let board_project_id = decision.and_then(|d| d.board_project_id);
        let Some(board_project_id) = board_project_id else {
            error!(
                "Build decision for {} is missing board_project_id",
                vp.rel_pro_path
            );
            results.push(ProjectResult {
                path: vp.rel_pro_path.clone(),
                status: "error".to_string(),
                error: Some("plan response missing board_project_id for build".to_string()),
            });
            exit_code = 1;
            continue;
        };

        match process_project(&kicad, &api, vp, &gh, &action_inputs, board_project_id).await {
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
    let results_json = serde_json::to_string(
        &results
            .iter()
            .map(|r| serde_json::json!({ "path": r.path, "status": r.status, "error": r.error }))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_default();
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
    board_project_id: boardflow_domain::public_ids::BoardProjectId,
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
        .map_err(ActionError::Io)?;
    let output_path = output_dir.path();

    // Create board run
    let create_resp = submission::create_board_run(api, vp, gh, board_project_id).await?;
    let board_run_id = create_resp.board_run_id;

    // If artifact_bundle is None, the run already exists in a terminal or importing state.
    // Skip processing — no upload or build needed.
    let artifact_bundle = match &create_resp.artifact_bundle {
        Some(bundle) => bundle,
        None => {
            info!(
                "Board run {} already in status '{:?}', skipping build",
                board_run_id, create_resp.status
            );
            return Ok(false);
        }
    };
    let upload_url = &artifact_bundle.upload_url;
    let staging_object_key = &artifact_bundle.object_key;

    // Run artifact pipeline (ERC, DRC, PDFs, SVGs, Gerber, Drill, BOM, Position, 3D, iBOM, source files)
    let (artifacts, checks_failed) =
        artifact_pipeline::run_artifact_pipeline(kicad, vp, inputs, output_path).await?;

    // Build diff metadata + manifest
    let tree_hash = hash::compute_tree_hash(&vp.project_dir, &vp.excludes)
        .map_err(|e| ActionError::Bundle(format!("tree hash: {e}")))?;
    let manifest_path = manifest_builder::build_diff_and_manifest(
        vp,
        output_path,
        &tree_hash,
        &gh.sha,
        &artifacts,
    )?;

    // Build staging directory
    let staging_dir = bundle::build_staging_dir(
        output_path,
        &vp.project_dir,
        &vp.rel_dir,
        &vp.excludes,
        &manifest_path,
    )?;

    // Submit bundle (zip, upload, import)
    submission::submit_bundle(
        api,
        board_run_id,
        &staging_dir,
        output_path,
        upload_url,
        staging_object_key,
    )
    .await?;

    info!("Successfully processed project: {}", vp.rel_pro_path);
    Ok(checks_failed)
}
