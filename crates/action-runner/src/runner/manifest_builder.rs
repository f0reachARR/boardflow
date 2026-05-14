use std::fs;
use std::path::Path;

use crate::bundle;
use crate::error::ActionError;

use super::project_discovery::{ValidProject, build_plan_files};

pub(super) fn build_diff_and_manifest(
    vp: &ValidProject,
    output_path: &Path,
    tree_hash: &str,
    commit_sha: &str,
    artifacts: &[serde_json::Value],
) -> std::result::Result<std::path::PathBuf, ActionError> {
    let bom_dir = output_path.join("bom");
    let erc_json = output_path.join("erc.json");
    let drc_json = output_path.join("drc.json");

    // Generate diff metadata
    let diff_dir = output_path.join("diff");
    fs::create_dir_all(&diff_dir)?;
    let _ = bundle::generate_file_hashes_json(
        &vp.project_dir,
        &vp.excludes,
        &diff_dir.join("file_hashes.json"),
    );
    let _ = bundle::generate_bom_summary_json(
        &bom_dir.join("bom.csv"),
        &diff_dir.join("bom_summary.json"),
    );
    let _ = bundle::generate_checks_summary_json(
        &erc_json,
        &drc_json,
        &diff_dir.join("checks_summary.json"),
    );
    let _ = bundle::generate_artifacts_summary_json(
        artifacts,
        &diff_dir.join("artifacts_summary.json"),
    );
    let _ = bundle::generate_previews_json(output_path, &diff_dir.join("previews.json"));

    // Build manifest checks from ERC/DRC reports
    let manifest_checks = build_manifest_checks(&erc_json, &drc_json);

    // Build manifest files from project directory
    let manifest_files: Vec<serde_json::Value> = build_plan_files(&vp.project_dir, &vp.excludes)
        .into_iter()
        .map(|f| serde_json::json!({"path": f.path, "sha256": f.sha256}))
        .collect();

    // Create manifest
    let manifest_path = output_path.join("manifest.json");
    bundle::create_manifest(
        &vp.rel_pro_path,
        &format!("sha256:{tree_hash}"),
        commit_sha,
        &diff_dir.join("checks_summary.json"),
        artifacts,
        &manifest_checks,
        &manifest_files,
        &manifest_path,
    )?;

    Ok(manifest_path)
}

/// Build ManifestCheck entries from ERC/DRC JSON report files.
/// Always emits both erc and drc entries (skipped if report is missing/unparseable).
fn build_manifest_checks(erc_path: &Path, drc_path: &Path) -> Vec<serde_json::Value> {
    let mut checks = Vec::new();

    // ERC check
    let erc_check = if erc_path.exists() {
        if let Ok(content) = fs::read_to_string(erc_path) {
            match boardflow_kicad::report::ErcReport::parse(&content) {
                Ok(report) => {
                    let violations = report.actionable_violations();
                    let error_count =
                        violations.iter().filter(|v| v.severity == "error").count() as i32;
                    let warning_count = violations
                        .iter()
                        .filter(|v| v.severity == "warning")
                        .count() as i32;
                    let status = if error_count > 0 { "failed" } else { "passed" };

                    let findings: Vec<serde_json::Value> =
                        report
                            .sheets
                            .iter()
                            .flat_map(|sheet| {
                                sheet.violations.iter().filter(|v| v.is_actionable()).map(
                                    move |v| {
                                        let pos_mm = v.items.first().and_then(|item| {
                                            item.pos
                                                .as_ref()
                                                .map(|p| serde_json::json!({"x": p.x, "y": p.y}))
                                        });
                                        let mut finding = serde_json::json!({
                                            "severity": v.severity,
                                            "rule_code": v.violation_type,
                                            "title": v.description,
                                            "subject_kind": "schematic",
                                            "sheet_path": sheet.path,
                                        });
                                        if let Some(pos) = pos_mm {
                                            finding
                                                .as_object_mut()
                                                .unwrap()
                                                .insert("pos_mm".to_string(), pos);
                                        }
                                        finding
                                    },
                                )
                            })
                            .collect();

                    serde_json::json!({
                        "kind": "erc",
                        "status": status,
                        "error_count": error_count,
                        "warning_count": warning_count,
                        "notice_count": 0,
                        "tool_name": "kicad-cli",
                        "findings": findings,
                    })
                }
                Err(_) => serde_json::json!({
                    "kind": "erc",
                    "status": "skipped",
                    "error_count": 0,
                    "warning_count": 0,
                    "notice_count": 0,
                    "findings": [],
                }),
            }
        } else {
            serde_json::json!({
                "kind": "erc",
                "status": "skipped",
                "error_count": 0,
                "warning_count": 0,
                "notice_count": 0,
                "findings": [],
            })
        }
    } else {
        serde_json::json!({
            "kind": "erc",
            "status": "skipped",
            "error_count": 0,
            "warning_count": 0,
            "notice_count": 0,
            "findings": [],
        })
    };
    checks.push(erc_check);

    // DRC check
    let drc_check = if drc_path.exists() {
        if let Ok(content) = fs::read_to_string(drc_path) {
            match boardflow_kicad::report::DrcReport::parse(&content) {
                Ok(report) => {
                    let violations = report.actionable_violations();
                    let error_count =
                        violations.iter().filter(|v| v.severity == "error").count() as i32;
                    let warning_count = violations
                        .iter()
                        .filter(|v| v.severity == "warning")
                        .count() as i32;
                    let status = if error_count > 0 { "failed" } else { "passed" };

                    let findings: Vec<serde_json::Value> = report
                        .all_violations()
                        .into_iter()
                        .filter(|v| v.is_actionable())
                        .map(|v| {
                            let pos_mm = v.items.first().and_then(|item| {
                                item.pos
                                    .as_ref()
                                    .map(|p| serde_json::json!({"x": p.x, "y": p.y}))
                            });
                            let mut finding = serde_json::json!({
                                "severity": v.severity,
                                "rule_code": v.violation_type,
                                "title": v.description,
                                "subject_kind": "pcb",
                            });
                            if let Some(pos) = pos_mm {
                                finding
                                    .as_object_mut()
                                    .unwrap()
                                    .insert("pos_mm".to_string(), pos);
                            }
                            finding
                        })
                        .collect();

                    serde_json::json!({
                        "kind": "drc",
                        "status": status,
                        "error_count": error_count,
                        "warning_count": warning_count,
                        "notice_count": 0,
                        "tool_name": "kicad-cli",
                        "findings": findings,
                    })
                }
                Err(_) => serde_json::json!({
                    "kind": "drc",
                    "status": "skipped",
                    "error_count": 0,
                    "warning_count": 0,
                    "notice_count": 0,
                    "findings": [],
                }),
            }
        } else {
            serde_json::json!({
                "kind": "drc",
                "status": "skipped",
                "error_count": 0,
                "warning_count": 0,
                "notice_count": 0,
                "findings": [],
            })
        }
    } else {
        serde_json::json!({
            "kind": "drc",
            "status": "skipped",
            "error_count": 0,
            "warning_count": 0,
            "notice_count": 0,
            "findings": [],
        })
    };
    checks.push(drc_check);

    checks
}
