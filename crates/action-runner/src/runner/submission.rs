use std::fs;
use std::path::Path;

use boardflow_api_types::board_run::{
    CreateBoardRunRequest, CreateBoardRunResponse, ImportArtifactBundleRequest,
};
use boardflow_domain::public_ids::{BoardProjectId, BoardRunId};
use boardflow_kicad::hash;
use tracing::info;

use crate::api::ApiClient;
use crate::bundle;
use crate::error::ActionError;
use crate::inputs::GitHubContext;

use super::project_discovery::ValidProject;

pub(super) async fn create_board_run(
    api: &ApiClient,
    vp: &ValidProject,
    gh: &GitHubContext,
    board_project_id: BoardProjectId,
) -> std::result::Result<CreateBoardRunResponse, ActionError> {
    let tree_hash = hash::compute_tree_hash(&vp.project_dir, &vp.excludes)
        .map_err(|e| ActionError::Bundle(format!("tree hash: {e}")))?;

    let create_payload = serde_json::to_value(&CreateBoardRunRequest {
        board_project_id,
        project_path: vp.rel_pro_path.clone(),
        tree_hash: format!("sha256:{tree_hash}"),
        commit_sha: gh.sha.clone(),
        branch: gh.ref_name.clone(),
        ref_: gh.git_ref.clone(),
        github_run_id: gh.run_id.clone(),
        github_run_attempt: gh.run_attempt.clone(),
    })
    .expect("failed to serialize create_board_run request");

    api.create_board_run(&create_payload).await
}

pub(super) async fn submit_bundle(
    api: &ApiClient,
    board_run_id: BoardRunId,
    staging_dir: &Path,
    output_path: &Path,
    upload_url: &str,
    staging_object_key: &str,
) -> std::result::Result<(), ActionError> {
    // Create bundle zip
    let bundle_path = output_path.join("bundle.zip");
    if let Err(e) = bundle::create_bundle_zip(staging_dir, &bundle_path) {
        let _ = api
            .fail(board_run_id, "Bundle creation failed", &e.to_string())
            .await;
        return Err(ActionError::Bundle(format!("Bundle creation failed: {e}")));
    }

    let bundle_sha256 = bundle::compute_bundle_sha256(&bundle_path)?;

    // Upload bundle
    if let Err(e) = api.upload_bundle(upload_url, &bundle_path).await {
        let _ = api
            .fail(board_run_id, "Upload failed", &e.to_string())
            .await;
        return Err(e);
    }

    // Import
    let bundle_size = fs::metadata(&bundle_path)?.len();
    let import_payload = serde_json::to_value(&ImportArtifactBundleRequest {
        staging_object_key: staging_object_key.to_string(),
        bundle_sha256: format!("sha256:{bundle_sha256}"),
        bundle_size_bytes: bundle_size as i64,
    })
    .expect("failed to serialize import request");

    if let Err(e) = api.import(board_run_id, &import_payload).await {
        let _ = api
            .fail(board_run_id, "Import failed", &e.to_string())
            .await;
        return Err(e);
    }

    info!("Successfully uploaded and imported bundle for board run {board_run_id}");
    Ok(())
}
