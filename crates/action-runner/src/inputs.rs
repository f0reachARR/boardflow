use std::env;
use std::path::PathBuf;

use crate::error::{ActionError, Result};

#[derive(Debug, Clone)]
pub struct ActionInputs {
    pub token: String,
    pub mode: String,
    pub exclude_paths: String,
    pub api_url: String,
    pub fail_on_drc: bool,
    pub fail_on_erc: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct GitHubContext {
    pub event_name: String,
    pub repository: String,
    pub owner: String,
    pub repo_name: String,
    pub sha: String,
    pub git_ref: String,
    pub ref_name: String,
    pub run_id: String,
    pub run_attempt: String,
    pub output_path: PathBuf,
    pub summary_path: PathBuf,
    pub workspace: PathBuf,
}

pub fn parse_inputs() -> Result<ActionInputs> {
    let token = env::var("INPUT_TOKEN")
        .or_else(|_| env::var("INPUT_token"))
        .map_err(|_| ActionError::Input("Input 'token' is required".to_string()))?;

    if token.is_empty() {
        return Err(ActionError::Input("Input 'token' is required".to_string()));
    }

    let mode = env::var("INPUT_MODE")
        .or_else(|_| env::var("INPUT_mode"))
        .unwrap_or_else(|_| "auto".to_string());

    let exclude_paths = env::var("INPUT_EXCLUDE_PATHS")
        .or_else(|_| env::var("INPUT_EXCLUDE-PATHS"))
        .or_else(|_| env::var("INPUT_exclude_paths"))
        .or_else(|_| env::var("INPUT_exclude-paths"))
        .unwrap_or_default();

    let api_url = env::var("INPUT_API_URL")
        .or_else(|_| env::var("INPUT_API-URL"))
        .or_else(|_| env::var("INPUT_api_url"))
        .or_else(|_| env::var("INPUT_api-url"))
        .unwrap_or_else(|_| "https://api.boardflow.example.com".to_string());

    let fail_on_drc = env::var("INPUT_FAIL_ON_DRC")
        .or_else(|_| env::var("INPUT_FAIL-ON-DRC"))
        .or_else(|_| env::var("INPUT_fail_on_drc"))
        .or_else(|_| env::var("INPUT_fail-on-drc"))
        .unwrap_or_else(|_| "false".to_string())
        .eq_ignore_ascii_case("true");

    let fail_on_erc = env::var("INPUT_FAIL_ON_ERC")
        .or_else(|_| env::var("INPUT_FAIL-ON-ERC"))
        .or_else(|_| env::var("INPUT_fail_on_erc"))
        .or_else(|_| env::var("INPUT_fail-on-erc"))
        .unwrap_or_else(|_| "false".to_string())
        .eq_ignore_ascii_case("true");

    Ok(ActionInputs {
        token,
        mode,
        exclude_paths,
        api_url,
        fail_on_drc,
        fail_on_erc,
    })
}

pub fn parse_github_context() -> Result<GitHubContext> {
    let event_name = env::var("GITHUB_EVENT_NAME").unwrap_or_default();
    let repository = env::var("GITHUB_REPOSITORY").unwrap_or_default();
    let sha = env::var("GITHUB_SHA").unwrap_or_default();
    let git_ref = env::var("GITHUB_REF").unwrap_or_default();
    let ref_name = env::var("GITHUB_REF_NAME").unwrap_or_default();
    let run_id = env::var("GITHUB_RUN_ID").unwrap_or_default();
    let run_attempt = env::var("GITHUB_RUN_ATTEMPT").unwrap_or_else(|_| "1".to_string());
    let output_path = PathBuf::from(env::var("GITHUB_OUTPUT").unwrap_or_default());
    let summary_path = PathBuf::from(env::var("GITHUB_STEP_SUMMARY").unwrap_or_default());
    let workspace =
        PathBuf::from(env::var("GITHUB_WORKSPACE").unwrap_or_else(|_| "/github/workspace".into()));

    let (owner, repo_name) = if let Some((o, r)) = repository.split_once('/') {
        (o.to_string(), r.to_string())
    } else {
        (repository.clone(), String::new())
    };

    Ok(GitHubContext {
        event_name,
        repository,
        owner,
        repo_name,
        sha,
        git_ref,
        ref_name,
        run_id,
        run_attempt,
        output_path,
        summary_path,
        workspace,
    })
}
