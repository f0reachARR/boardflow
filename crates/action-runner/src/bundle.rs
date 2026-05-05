use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;
use zip::write::FileOptions;
use zip::ZipWriter;

use crate::error::{ActionError, Result};

/// Create a zip archive from all files in `staging_dir`, writing to `bundle_path`.
pub fn create_bundle_zip(staging_dir: &Path, bundle_path: &Path) -> Result<()> {
    let file = fs::File::create(bundle_path)?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for entry in WalkDir::new(staging_dir)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        let rel_path = path
            .strip_prefix(staging_dir)
            .unwrap_or(path)
            .to_str()
            .unwrap_or_default();

        if rel_path.is_empty() {
            continue;
        }

        if entry.file_type().is_dir() {
            zip.add_directory(format!("{rel_path}/"), options)
                .map_err(|e| ActionError::Bundle(format!("Failed to add directory: {e}")))?;
        } else {
            zip.start_file(rel_path, options)
                .map_err(|e| ActionError::Bundle(format!("Failed to start file: {e}")))?;
            let data = fs::read(path)?;
            zip.write_all(&data)?;
        }
    }

    zip.finish()
        .map_err(|e| ActionError::Bundle(format!("Failed to finalize zip: {e}")))?;
    Ok(())
}

/// Compute SHA256 of a file, returning hex string.
pub fn compute_bundle_sha256(bundle_path: &Path) -> Result<String> {
    let data = fs::read(bundle_path)?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash = hasher.finalize();
    Ok(hex::encode(hash))
}

/// Build the staging directory structure from generated artifacts.
pub fn build_staging_dir(
    output_dir: &Path,
    project_dir: &Path,
    rel_dir: &str,
    excludes: &[String],
    manifest_path: &Path,
) -> Result<PathBuf> {
    let staging = output_dir.join("staging");

    // Create directory structure
    fs::create_dir_all(staging.join("review"))?;
    fs::create_dir_all(staging.join("assembly"))?;
    fs::create_dir_all(staging.join("fabrication"))?;
    fs::create_dir_all(staging.join("checks"))?;
    fs::create_dir_all(staging.join("diff"))?;

    // review/
    copy_if_exists(output_dir, "pdf/schematic.pdf", &staging.join("review/schematic.pdf"));
    copy_if_exists(output_dir, "pdf/pcb.pdf", &staging.join("review/pcb.pdf"));
    copy_if_exists(output_dir, "svg/pcb_top.svg", &staging.join("review/pcb_top.svg"));
    copy_if_exists(output_dir, "svg/pcb_bottom.svg", &staging.join("review/pcb_bottom.svg"));
    copy_if_exists(output_dir, "3d/top.png", &staging.join("review/render_top.png"));
    copy_if_exists(output_dir, "3d/bottom.png", &staging.join("review/render_bottom.png"));

    // assembly/
    copy_if_exists(output_dir, "ibom/ibom.html", &staging.join("assembly/ibom.html"));
    // ibom might be named differently, find any .html
    if !staging.join("assembly/ibom.html").exists() {
        if let Ok(entries) = fs::read_dir(output_dir.join("ibom")) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|e| e == "html") {
                    let _ = fs::copy(entry.path(), staging.join("assembly/ibom.html"));
                    break;
                }
            }
        }
    }
    copy_if_exists(output_dir, "bom/bom.csv", &staging.join("assembly/bom.csv"));
    copy_if_exists(output_dir, "position/position.csv", &staging.join("assembly/position.csv"));

    // fabrication/
    copy_if_exists(output_dir, "gerbers.zip", &staging.join("fabrication/gerbers.zip"));
    copy_if_exists(output_dir, "drill.zip", &staging.join("fabrication/drill.zip"));
    copy_if_exists(output_dir, "fabrication.zip", &staging.join("fabrication/fabrication.zip"));

    // checks/
    copy_if_exists(output_dir, "erc.json", &staging.join("checks/erc.json"));
    copy_if_exists(output_dir, "drc.json", &staging.join("checks/drc.json"));

    // diff/
    copy_if_exists(output_dir, "diff/file_hashes.json", &staging.join("diff/file_hashes.json"));
    copy_if_exists(output_dir, "diff/bom_summary.json", &staging.join("diff/bom_summary.json"));
    copy_if_exists(
        output_dir,
        "diff/checks_summary.json",
        &staging.join("diff/checks_summary.json"),
    );
    copy_if_exists(
        output_dir,
        "diff/artifacts_summary.json",
        &staging.join("diff/artifacts_summary.json"),
    );
    copy_if_exists(output_dir, "diff/previews.json", &staging.join("diff/previews.json"));

    // kicad/ source files
    let kicad_staging = if rel_dir == "." {
        staging.join("kicad")
    } else {
        staging.join("kicad").join(rel_dir)
    };
    fs::create_dir_all(&kicad_staging)?;

    if let Ok(entries) = fs::read_dir(project_dir) {
        let globset = boardflow_kicad::hash::is_excluded;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if matches!(ext, "kicad_pro" | "kicad_sch" | "kicad_pcb" | "kicad_wks") {
                let file_name = path.file_name().unwrap_or_default().to_str().unwrap_or("");
                if !globset(file_name, excludes) {
                    let _ = fs::copy(&path, kicad_staging.join(file_name));
                }
            }
        }
    }

    // manifest.json at root
    if manifest_path.exists() {
        let _ = fs::copy(manifest_path, staging.join("manifest.json"));
    }

    Ok(staging)
}

/// Create a zip of gerber and drill directories combined.
pub fn create_fabrication_zip(gerber_dir: &Path, drill_dir: &Path, output_path: &Path) -> Result<()> {
    let file = fs::File::create(output_path)?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Deflated);

    add_dir_to_zip(&mut zip, gerber_dir, "", options)?;
    add_dir_to_zip(&mut zip, drill_dir, "", options)?;

    zip.finish()
        .map_err(|e| ActionError::Bundle(format!("Failed to finalize fabrication zip: {e}")))?;
    Ok(())
}

/// Generate file_hashes.json for diff metadata.
pub fn generate_file_hashes_json(
    project_dir: &Path,
    excludes: &[String],
    output: &Path,
) -> Result<()> {
    let files = boardflow_kicad::hash::list_project_files(project_dir, excludes)
        .map_err(|e| ActionError::Bundle(format!("Failed to list project files: {e}")))?;

    let mut entries: Vec<serde_json::Value> = Vec::new();
    for file_path in &files {
        let rel = file_path
            .strip_prefix(project_dir)
            .unwrap_or(file_path)
            .to_str()
            .unwrap_or_default();
        let hash = boardflow_kicad::hash::compute_file_sha256(file_path)
            .map_err(|e| ActionError::Bundle(format!("Failed to hash {rel}: {e}")))?;
        entries.push(serde_json::json!({
            "path": rel,
            "sha256": format!("sha256:{hash}"),
        }));
    }

    let json = serde_json::to_string_pretty(&serde_json::json!({ "files": entries }))?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, json)?;
    Ok(())
}

/// Generate bom_summary.json from BOM CSV.
pub fn generate_bom_summary_json(bom_csv: &Path, output: &Path) -> Result<()> {
    let summary = if bom_csv.exists() {
        let content = fs::read_to_string(bom_csv)?;
        let lines: Vec<&str> = content.lines().collect();
        let component_count = if lines.len() > 1 {
            lines.len() - 1 // exclude header
        } else {
            0
        };
        serde_json::json!({
            "component_count": component_count,
            "available": true,
        })
    } else {
        serde_json::json!({
            "component_count": 0,
            "available": false,
        })
    };

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&summary)?;
    fs::write(output, json)?;
    Ok(())
}

/// Generate checks_summary.json from ERC/DRC JSON files.
pub fn generate_checks_summary_json(
    erc_path: &Path,
    drc_path: &Path,
    output: &Path,
) -> Result<()> {
    let erc_summary = if erc_path.exists() {
        let content = fs::read_to_string(erc_path)?;
        match boardflow_kicad::report::ErcReport::parse(&content) {
            Ok(report) => {
                let violations = report.all_violations();
                let errors = violations.iter().filter(|v| v.severity == "error").count();
                let warnings = violations.iter().filter(|v| v.severity == "warning").count();
                serde_json::json!({
                    "available": true,
                    "error_count": errors,
                    "warning_count": warnings,
                })
            }
            Err(_) => serde_json::json!({ "available": false, "error_count": 0, "warning_count": 0 }),
        }
    } else {
        serde_json::json!({ "available": false, "error_count": 0, "warning_count": 0 })
    };

    let drc_summary = if drc_path.exists() {
        let content = fs::read_to_string(drc_path)?;
        match boardflow_kicad::report::DrcReport::parse(&content) {
            Ok(report) => {
                let violations = report.all_violations();
                let errors = violations.iter().filter(|v| v.severity == "error").count();
                let warnings = violations.iter().filter(|v| v.severity == "warning").count();
                serde_json::json!({
                    "available": true,
                    "error_count": errors,
                    "warning_count": warnings,
                })
            }
            Err(_) => serde_json::json!({ "available": false, "error_count": 0, "warning_count": 0 }),
        }
    } else {
        serde_json::json!({ "available": false, "error_count": 0, "warning_count": 0 })
    };

    let summary = serde_json::json!({
        "erc": erc_summary,
        "drc": drc_summary,
    });

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&summary)?;
    fs::write(output, json)?;
    Ok(())
}

/// Generate artifacts_summary.json.
pub fn generate_artifacts_summary_json(
    artifacts: &[serde_json::Value],
    output: &Path,
) -> Result<()> {
    let available = artifacts
        .iter()
        .filter(|a| a.get("status").and_then(|s| s.as_str()) == Some("available"))
        .count();
    let failed = artifacts
        .iter()
        .filter(|a| a.get("status").and_then(|s| s.as_str()) == Some("failed"))
        .count();

    let summary = serde_json::json!({
        "total": artifacts.len(),
        "available": available,
        "failed": failed,
        "artifacts": artifacts,
    });

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&summary)?;
    fs::write(output, json)?;
    Ok(())
}

/// Generate previews.json listing preview images in output_dir.
pub fn generate_previews_json(output_dir: &Path, output: &Path) -> Result<()> {
    let mut previews = Vec::new();

    let preview_files = [
        ("pcb_top_svg", "svg/pcb_top.svg"),
        ("pcb_bottom_svg", "svg/pcb_bottom.svg"),
        ("render_top_png", "3d/top.png"),
        ("render_bottom_png", "3d/bottom.png"),
    ];

    for (name, rel) in &preview_files {
        let path = output_dir.join(rel);
        if path.exists() {
            previews.push(serde_json::json!({
                "name": name,
                "path": rel,
                "available": true,
            }));
        } else {
            previews.push(serde_json::json!({
                "name": name,
                "path": rel,
                "available": false,
            }));
        }
    }

    let json = serde_json::to_string_pretty(&serde_json::json!({ "previews": previews }))?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, json)?;
    Ok(())
}

/// Create manifest.json compatible with crates/artifact BundleManifest struct.
pub fn create_manifest(
    project_path: &str,
    tree_hash: &str,
    commit_sha: &str,
    checks_summary_path: &Path,
    artifacts: &[serde_json::Value],
    checks: &[serde_json::Value],
    files: &[serde_json::Value],
    output: &Path,
) -> Result<()> {
    // Build diff_metadata entries with sha256 and size_bytes from actual files
    let diff_dir = checks_summary_path.parent().unwrap_or(Path::new("."));
    let diff_metadata = build_diff_metadata(diff_dir);

    // Convert artifact entries to the format expected by crates/artifact's ManifestArtifact:
    // { type, filename, content_type, status, source_path?, sha256?, size_bytes? }
    let manifest_artifacts: Vec<serde_json::Value> = artifacts
        .iter()
        .map(|a| {
            let mut entry = serde_json::Map::new();
            entry.insert("type".to_string(), a["type"].clone());
            entry.insert("status".to_string(), a["status"].clone());

            // filename = the display name (basename of path)
            if let Some(path) = a["path"].as_str() {
                let filename = Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(path);
                entry.insert("filename".to_string(), serde_json::json!(filename));
                // source_path = zip entry path (same as path in staging)
                entry.insert("source_path".to_string(), serde_json::json!(path));
            } else {
                entry.insert("filename".to_string(), serde_json::json!(""));
            }

            if let Some(ct) = a.get("content_type") {
                entry.insert("content_type".to_string(), ct.clone());
            } else {
                entry.insert("content_type".to_string(), serde_json::json!("application/octet-stream"));
            }

            if let Some(sha) = a.get("sha256") {
                entry.insert("sha256".to_string(), sha.clone());
            }
            if let Some(size) = a.get("size_bytes") {
                entry.insert("size_bytes".to_string(), size.clone());
            }
            if let Some(err) = a.get("error_message") {
                entry.insert("status_reason".to_string(), err.clone());
            }

            serde_json::Value::Object(entry)
        })
        .collect();

    let manifest = serde_json::json!({
        "version": 1,
        "project_path": project_path,
        "tree_hash": tree_hash,
        "commit_sha": commit_sha,
        "files": files,
        "artifacts": manifest_artifacts,
        "checks": checks,
        "diff_metadata": diff_metadata,
    });

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&manifest)?;
    fs::write(output, json)?;
    Ok(())
}

/// Build diff_metadata object with path, sha256, and size_bytes for each diff file.
fn build_diff_metadata(diff_dir: &Path) -> serde_json::Value {
    let entries = [
        ("file_hashes", "diff/file_hashes.json"),
        ("bom_summary", "diff/bom_summary.json"),
        ("checks_summary", "diff/checks_summary.json"),
        ("artifacts_summary", "diff/artifacts_summary.json"),
        ("previews", "diff/previews.json"),
    ];

    let mut metadata = serde_json::Map::new();
    for (key, rel_path) in &entries {
        let file_name = Path::new(rel_path).file_name().unwrap_or_default();
        let full_path = diff_dir.join(file_name);
        if full_path.exists() {
            if let Ok(data) = fs::read(&full_path) {
                let size_bytes = data.len() as u64;
                let mut hasher = Sha256::new();
                hasher.update(&data);
                let hash = hex::encode(hasher.finalize());
                metadata.insert(
                    key.to_string(),
                    serde_json::json!({
                        "path": rel_path,
                        "sha256": format!("sha256:{hash}"),
                        "size_bytes": size_bytes,
                    }),
                );
            }
        }
    }
    serde_json::Value::Object(metadata)
}

fn copy_if_exists(base: &Path, rel: &str, dest: &Path) {
    let src = base.join(rel);
    if src.exists() {
        let _ = fs::copy(src, dest);
    }
}

fn add_dir_to_zip(
    zip: &mut ZipWriter<fs::File>,
    dir: &Path,
    prefix: &str,
    options: FileOptions<()>,
) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in WalkDir::new(dir)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let rel = path.strip_prefix(dir).unwrap_or(path);
        let name = if prefix.is_empty() {
            rel.to_str().unwrap_or_default().to_string()
        } else {
            format!("{prefix}/{}", rel.to_str().unwrap_or_default())
        };
        zip.start_file(&name, options)
            .map_err(|e| ActionError::Bundle(format!("Failed to add {name}: {e}")))?;
        let data = fs::read(path)?;
        zip.write_all(&data)?;
    }
    Ok(())
}
