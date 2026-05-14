use boardflow_api_types::plan::{
    PlanActionInput, PlanGitInput, PlanMode, PlanProjectInput, PlanRepositoryInput, PlanRequest,
};
use boardflow_kicad::hash;
use tracing::warn;

use crate::inputs::{ActionInputs, GitHubContext};

use super::project_discovery::{ValidProject, build_plan_files};

pub(super) fn build_plan_request(
    valid_projects: &[ValidProject],
    inputs: &ActionInputs,
    gh: &GitHubContext,
) -> serde_json::Value {
    let mut plan_projects = Vec::new();
    for vp in valid_projects {
        let tree_hash = match hash::compute_tree_hash(&vp.project_dir, &vp.excludes) {
            Ok(h) => h,
            Err(e) => {
                warn!("Failed to compute tree hash for {}: {e}", vp.rel_dir);
                continue;
            }
        };

        let files = build_plan_files(&vp.project_dir, &vp.excludes);
        let yml_rel = if vp.rel_dir == "." {
            ".boardflow.yml".to_string()
        } else {
            format!("{}/.boardflow.yml", vp.rel_dir)
        };

        plan_projects.push(PlanProjectInput {
            project_path: vp.rel_pro_path.clone(),
            config_path: yml_rel,
            project_dir: vp.rel_dir.clone(),
            tree_hash: format!("sha256:{tree_hash}"),
            files,
        });
    }

    let mode = match inputs.mode.as_str() {
        "all" => PlanMode::All,
        _ => PlanMode::Auto,
    };

    let github_repository_id = std::env::var("GITHUB_REPOSITORY_ID").unwrap_or_default();

    let plan_request = PlanRequest {
        repository: PlanRepositoryInput {
            github_repository_id,
            owner: gh.owner.clone(),
            name: gh.repo_name.clone(),
        },
        git: PlanGitInput {
            ref_: gh.git_ref.clone(),
            branch: gh.ref_name.clone(),
            commit_sha: gh.sha.clone(),
            event_name: gh.event_name.clone(),
        },
        action: PlanActionInput {
            workflow: "BoardFlow".to_string(),
            run_id: gh.run_id.clone(),
            run_attempt: gh.run_attempt.clone(),
        },
        mode,
        projects: plan_projects,
    };

    serde_json::to_value(&plan_request).expect("failed to serialize plan request")
}
