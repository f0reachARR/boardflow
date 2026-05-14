use std::path::{Path, PathBuf};

use boardflow_kicad::config::{self, BoardflowConfig};
use boardflow_kicad::detect;
use boardflow_kicad::hash;

use boardflow_api_types::plan::PlanProjectFile;

use crate::inputs::ActionInputs;
use crate::summary;

pub(super) struct ValidProject {
    pub(super) project_dir: PathBuf,
    pub(super) pro_file: PathBuf,
    pub(super) pcb_file: PathBuf,
    pub(super) sch_file: PathBuf,
    pub(super) excludes: Vec<String>,
    #[allow(dead_code)]
    pub(super) config: BoardflowConfig,
    pub(super) rel_dir: String,
    pub(super) rel_pro_path: String,
}

pub(super) fn discover_and_validate(
    workspace: &Path,
    action_inputs: &ActionInputs,
) -> (Vec<ValidProject>, u32) {
    let yml_paths = match detect::find_boardflow_ymls(workspace) {
        Ok(p) => p,
        Err(_) => {
            summary::error("No .boardflow.yml found in workspace");
            return (Vec::new(), 0);
        }
    };

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
        let excludes =
            config::merge_excludes(hash::BUILTIN_EXCLUDES, &input_excludes, &cfg.exclude_paths);

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
        let pro_rel = pro_file
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default();
        let pcb_rel = pcb_file
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default();
        let sch_rel = sch_file
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default();
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
            .strip_prefix(workspace)
            .map(|p| p.to_str().unwrap_or("."))
            .unwrap_or(".")
            .to_string();
        let rel_dir = if rel_dir.is_empty() {
            ".".to_string()
        } else {
            rel_dir
        };

        let rel_pro_path = pro_file
            .strip_prefix(workspace)
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

    (valid_projects, detection_errors)
}

pub(super) fn build_plan_files(project_dir: &Path, excludes: &[String]) -> Vec<PlanProjectFile> {
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
            Some(PlanProjectFile {
                path: rel_path,
                sha256: format!("sha256:{file_hash}"),
            })
        })
        .collect()
}
