use std::fs;
use std::io::Read;
use std::path::Path;
use tempfile::TempDir;

#[path = "../src/error.rs"]
mod error;
#[path = "../src/bundle.rs"]
mod bundle;

#[test]
fn test_create_bundle_zip_and_sha256() {
    let dir = TempDir::new().unwrap();
    let staging = dir.path().join("staging");
    fs::create_dir_all(staging.join("review")).unwrap();
    fs::create_dir_all(staging.join("checks")).unwrap();
    fs::write(staging.join("manifest.json"), r#"{"version":1}"#).unwrap();
    fs::write(staging.join("review/pcb.pdf"), b"fake pdf content").unwrap();
    fs::write(staging.join("checks/erc.json"), r#"{"sheets":[]}"#).unwrap();

    let bundle_path = dir.path().join("bundle.zip");
    bundle::create_bundle_zip(&staging, &bundle_path).unwrap();

    assert!(bundle_path.exists());
    assert!(fs::metadata(&bundle_path).unwrap().len() > 0);

    // Verify it's a valid zip
    let file = fs::File::open(&bundle_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_string())
        .collect();
    assert!(names.iter().any(|n| n == "manifest.json"));
    assert!(names.iter().any(|n| n.contains("review/pcb.pdf")));
    assert!(names.iter().any(|n| n.contains("checks/erc.json")));

    // SHA256
    let sha = bundle::compute_bundle_sha256(&bundle_path).unwrap();
    assert_eq!(sha.len(), 64); // hex-encoded SHA256
    // Same file = same hash
    let sha2 = bundle::compute_bundle_sha256(&bundle_path).unwrap();
    assert_eq!(sha, sha2);
}

#[test]
fn test_create_fabrication_zip() {
    let dir = TempDir::new().unwrap();
    let gerber_dir = dir.path().join("gerber");
    let drill_dir = dir.path().join("drill");
    fs::create_dir_all(&gerber_dir).unwrap();
    fs::create_dir_all(&drill_dir).unwrap();

    fs::write(gerber_dir.join("front.gbr"), b"gerber data").unwrap();
    fs::write(gerber_dir.join("back.gbr"), b"gerber back").unwrap();
    fs::write(drill_dir.join("drill.drl"), b"drill data").unwrap();

    let output = dir.path().join("fabrication.zip");
    bundle::create_fabrication_zip(&gerber_dir, &drill_dir, &output).unwrap();

    assert!(output.exists());
    let file = fs::File::open(&output).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_string())
        .collect();
    assert!(names.iter().any(|n| n.contains("front.gbr")));
    assert!(names.iter().any(|n| n.contains("drill.drl")));
}

#[test]
fn test_create_fabrication_zip_missing_dir() {
    let dir = TempDir::new().unwrap();
    let gerber_dir = dir.path().join("nonexist_gerber");
    let drill_dir = dir.path().join("nonexist_drill");

    let output = dir.path().join("fab.zip");
    // Should not panic, just create empty zip
    bundle::create_fabrication_zip(&gerber_dir, &drill_dir, &output).unwrap();
    assert!(output.exists());
}

#[test]
fn test_generate_file_hashes_json() {
    let dir = TempDir::new().unwrap();
    let project = dir.path().join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("file1.txt"), b"hello").unwrap();
    fs::write(project.join("file2.txt"), b"world").unwrap();

    let output = dir.path().join("file_hashes.json");
    bundle::generate_file_hashes_json(&project, &[], &output).unwrap();

    let content = fs::read_to_string(&output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    let files = parsed["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert!(files[0]["path"].as_str().is_some());
    assert!(files[0]["sha256"].as_str().unwrap().starts_with("sha256:"));
}

#[test]
fn test_generate_bom_summary_json_existing_csv() {
    let dir = TempDir::new().unwrap();
    let csv_path = dir.path().join("bom.csv");
    fs::write(&csv_path, "Ref,Value,Qty\nR1,10k,1\nC1,100nF,2\n").unwrap();

    let output = dir.path().join("bom_summary.json");
    bundle::generate_bom_summary_json(&csv_path, &output).unwrap();

    let content = fs::read_to_string(&output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["component_count"].as_u64().unwrap(), 2);
    assert_eq!(parsed["available"].as_bool().unwrap(), true);
}

#[test]
fn test_generate_bom_summary_json_missing_csv() {
    let dir = TempDir::new().unwrap();
    let csv_path = dir.path().join("nonexist.csv");

    let output = dir.path().join("bom_summary.json");
    bundle::generate_bom_summary_json(&csv_path, &output).unwrap();

    let content = fs::read_to_string(&output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["available"].as_bool().unwrap(), false);
}

#[test]
fn test_generate_checks_summary_json() {
    let dir = TempDir::new().unwrap();

    // Write minimal ERC report
    let erc = dir.path().join("erc.json");
    fs::write(&erc, r#"{"sheets":[{"path":"/","violations":[{"type":"pin_not_connected","description":"test","severity":"error","items":[],"excluded":false}]}]}"#).unwrap();

    // Write minimal DRC report
    let drc = dir.path().join("drc.json");
    fs::write(&drc, r#"{"violations":[{"type":"clearance","description":"test","severity":"warning","items":[],"excluded":false}],"unconnected_items":[],"schematic_parity":[]}"#).unwrap();

    let output = dir.path().join("checks_summary.json");
    bundle::generate_checks_summary_json(&erc, &drc, &output).unwrap();

    let content = fs::read_to_string(&output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["erc"]["error_count"].as_u64().unwrap(), 1);
    assert_eq!(parsed["erc"]["warning_count"].as_u64().unwrap(), 0);
    assert_eq!(parsed["drc"]["error_count"].as_u64().unwrap(), 0);
    assert_eq!(parsed["drc"]["warning_count"].as_u64().unwrap(), 1);
}

#[test]
fn test_generate_artifacts_summary_json() {
    let dir = TempDir::new().unwrap();
    let artifacts = vec![
        serde_json::json!({"type": "erc_report", "status": "available"}),
        serde_json::json!({"type": "drc_report", "status": "failed", "error_message": "DRC failed"}),
        serde_json::json!({"type": "pcb_pdf", "status": "available"}),
    ];

    let output = dir.path().join("artifacts_summary.json");
    bundle::generate_artifacts_summary_json(&artifacts, &output).unwrap();

    let content = fs::read_to_string(&output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["total"].as_u64().unwrap(), 3);
    assert_eq!(parsed["available"].as_u64().unwrap(), 2);
    assert_eq!(parsed["failed"].as_u64().unwrap(), 1);
}

#[test]
fn test_generate_previews_json() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("svg")).unwrap();
    fs::create_dir_all(dir.path().join("3d")).unwrap();
    fs::write(dir.path().join("svg/pcb_top.svg"), b"<svg/>").unwrap();
    // pcb_bottom.svg missing
    fs::write(dir.path().join("3d/top.png"), b"PNG").unwrap();
    // 3d/bottom.png missing

    let output = dir.path().join("previews.json");
    bundle::generate_previews_json(dir.path(), &output).unwrap();

    let content = fs::read_to_string(&output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    let previews = parsed["previews"].as_array().unwrap();
    assert_eq!(previews.len(), 4);

    let top_svg = previews.iter().find(|p| p["name"] == "pcb_top_svg").unwrap();
    assert_eq!(top_svg["available"].as_bool().unwrap(), true);

    let bottom_svg = previews.iter().find(|p| p["name"] == "pcb_bottom_svg").unwrap();
    assert_eq!(bottom_svg["available"].as_bool().unwrap(), false);
}

#[test]
fn test_create_manifest() {
    let dir = TempDir::new().unwrap();
    let checks_path = dir.path().join("checks_summary.json");
    fs::write(&checks_path, r#"{"erc":{"available":true},"drc":{"available":false}}"#).unwrap();

    let artifacts = vec![
        serde_json::json!({"type": "erc_report", "status": "available", "path": "checks/erc.json", "content_type": "application/json"}),
    ];

    let checks = vec![
        serde_json::json!({
            "kind": "erc",
            "status": "passed",
            "error_count": 0,
            "warning_count": 1,
            "notice_count": 0,
            "tool_name": "kicad-cli",
            "findings": [{"severity": "warning", "rule_code": "W001", "title": "test warning"}]
        }),
    ];

    let files = vec![
        serde_json::json!({"path": "board.kicad_pro", "sha256": "sha256:abc123"}),
        serde_json::json!({"path": "board.kicad_pcb", "sha256": "sha256:def456"}),
    ];

    let output = dir.path().join("manifest.json");
    bundle::create_manifest(
        "board/board.kicad_pro",
        "sha256:abcdef",
        "abc123",
        &checks_path,
        &artifacts,
        &checks,
        &files,
        &output,
    ).unwrap();

    let content = fs::read_to_string(&output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["version"].as_u64().unwrap(), 1);
    assert_eq!(parsed["project_path"].as_str().unwrap(), "board/board.kicad_pro");
    assert_eq!(parsed["tree_hash"].as_str().unwrap(), "sha256:abcdef");
    assert_eq!(parsed["commit_sha"].as_str().unwrap(), "abc123");
    assert!(parsed["files"].is_array());
    assert_eq!(parsed["files"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["files"][0]["path"].as_str().unwrap(), "board.kicad_pro");
    assert!(parsed["artifacts"].is_array());
    assert_eq!(parsed["artifacts"].as_array().unwrap().len(), 1);
    // Check artifact conversion to ManifestArtifact format
    let art = &parsed["artifacts"][0];
    assert_eq!(art["type"].as_str().unwrap(), "erc_report");
    assert_eq!(art["status"].as_str().unwrap(), "available");
    assert_eq!(art["filename"].as_str().unwrap(), "erc.json");
    assert!(parsed["diff_metadata"].is_object());
    assert!(parsed["checks"].is_array());
    assert_eq!(parsed["checks"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["checks"][0]["kind"].as_str().unwrap(), "erc");
    assert_eq!(parsed["checks"][0]["status"].as_str().unwrap(), "passed");
    assert_eq!(parsed["checks"][0]["warning_count"].as_i64().unwrap(), 1);
    assert_eq!(parsed["checks"][0]["findings"].as_array().unwrap().len(), 1);
}

#[test]
fn test_create_manifest_source_path_is_zip_entry() {
    let dir = TempDir::new().unwrap();
    let checks_path = dir.path().join("checks_summary.json");
    fs::write(&checks_path, "{}").unwrap();

    // Source artifact has both "path" (zip entry) and "source_path" (repo-relative)
    let artifacts = vec![
        serde_json::json!({
            "type": "kicad_pcb",
            "status": "available",
            "path": "kicad/board/board.kicad_pcb",
            "source_path": "board/board.kicad_pcb",
            "content_type": "application/octet-stream",
            "sha256": "sha256:abc",
            "size_bytes": 100
        }),
    ];

    let output = dir.path().join("manifest.json");
    bundle::create_manifest(
        "board/board.kicad_pro",
        "sha256:abcdef",
        "abc123",
        &checks_path,
        &artifacts,
        &[],
        &[],
        &output,
    ).unwrap();

    let content = fs::read_to_string(&output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    let art = &parsed["artifacts"][0];
    // source_path in manifest must be the zip entry path, NOT the repo-relative path
    assert_eq!(art["source_path"].as_str().unwrap(), "kicad/board/board.kicad_pcb");
}

#[test]
fn test_build_staging_dir() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("output");
    let project = dir.path().join("project");

    // Setup output structure
    fs::create_dir_all(output.join("pdf")).unwrap();
    fs::create_dir_all(output.join("svg")).unwrap();
    fs::create_dir_all(output.join("diff")).unwrap();
    fs::write(output.join("pdf/pcb.pdf"), b"pdf").unwrap();
    fs::write(output.join("svg/pcb_top.svg"), b"svg").unwrap();
    fs::write(output.join("erc.json"), b"{}").unwrap();
    fs::write(output.join("diff/file_hashes.json"), b"{}").unwrap();

    // Setup project with kicad files
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("test.kicad_pro"), b"pro").unwrap();
    fs::write(project.join("test.kicad_sch"), b"sch").unwrap();
    fs::write(project.join("test.kicad_pcb"), b"pcb").unwrap();

    // Create manifest
    let manifest = output.join("manifest.json");
    fs::write(&manifest, r#"{"schema_version":1}"#).unwrap();

    let staging = bundle::build_staging_dir(&output, &project, ".", &[], &manifest).unwrap();

    assert!(staging.join("manifest.json").exists());
    assert!(staging.join("review/pcb.pdf").exists());
    assert!(staging.join("review/pcb_top.svg").exists());
    assert!(staging.join("checks/erc.json").exists());
    assert!(staging.join("diff/file_hashes.json").exists());
    assert!(staging.join("kicad/test.kicad_pro").exists());
    assert!(staging.join("kicad/test.kicad_sch").exists());
    assert!(staging.join("kicad/test.kicad_pcb").exists());
}

#[test]
fn test_build_staging_dir_nested_project() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("output");
    let project = dir.path().join("boards/myboard");

    fs::create_dir_all(&output).unwrap();
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("hw.kicad_pro"), b"pro").unwrap();

    let manifest = output.join("manifest.json");
    fs::write(&manifest, r#"{}"#).unwrap();

    let staging = bundle::build_staging_dir(&output, &project, "boards/myboard", &[], &manifest).unwrap();
    assert!(staging.join("kicad/boards/myboard/hw.kicad_pro").exists());
}
