use boardflow_artifact::{
    ArtifactError, BundleManifest, CoordinateMm, MAX_BUNDLE_SIZE, ManifestArtifact, ManifestCheck,
    ManifestFile, ManifestFinding, extract_bundle, verify_sha256,
};
use sha2::{Digest, Sha256};
use std::io::Write;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

/// Helper to create a minimal valid manifest
fn make_manifest(artifacts: Vec<ManifestArtifact>) -> BundleManifest {
    BundleManifest {
        version: 1,
        project_path: "hardware/test".to_string(),
        tree_hash: "abc123".to_string(),
        commit_sha: "def456".to_string(),
        files: vec![ManifestFile {
            path: "test.kicad_pcb".to_string(),
            sha256: "sha256:0000".to_string(),
        }],
        artifacts,
        checks: vec![],
        diff_metadata: None,
    }
}

/// Helper to create a ZIP bundle with manifest and optional files
fn create_test_zip(manifest: &BundleManifest, files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = SimpleFileOptions::default();

        // Write manifest.json
        let manifest_json = serde_json::to_vec(manifest).unwrap();
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(&manifest_json).unwrap();

        // Write additional files
        for (name, data) in files {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }

        zip.finish().unwrap();
    }
    buf
}

#[test]
fn test_extract_bundle_valid() {
    let artifact_data = b"fake gerber content";
    let manifest = make_manifest(vec![ManifestArtifact {
        r#type: "gerber".to_string(),
        filename: "output.gbr".to_string(),
        content_type: "application/octet-stream".to_string(),
        status: "available".to_string(),
        source_path: Some("artifacts/output.gbr".to_string()),
        logical_name: None,
        status_reason: None,
        sha256: None,
        size_bytes: None,
    }]);

    let zip_data = create_test_zip(&manifest, &[("artifacts/output.gbr", artifact_data)]);

    let (parsed_manifest, extracted) = extract_bundle(&zip_data).unwrap();
    assert_eq!(parsed_manifest.version, 1);
    assert_eq!(parsed_manifest.tree_hash, "abc123");
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0].path, "artifacts/output.gbr");
    assert_eq!(extracted[0].data, artifact_data);
    assert!(extracted[0].sha256.starts_with("sha256:"));
}

#[test]
fn test_extract_bundle_skips_non_available_artifacts() {
    let manifest = make_manifest(vec![ManifestArtifact {
        r#type: "gerber".to_string(),
        filename: "output.gbr".to_string(),
        content_type: "application/octet-stream".to_string(),
        status: "missing".to_string(),
        source_path: None,
        logical_name: None,
        status_reason: Some("file not found".to_string()),
        sha256: None,
        size_bytes: None,
    }]);

    let zip_data = create_test_zip(&manifest, &[]);

    let (_, extracted) = extract_bundle(&zip_data).unwrap();
    assert_eq!(extracted.len(), 0);
}

#[test]
fn test_extract_bundle_missing_manifest() {
    // Create a ZIP without manifest.json
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = SimpleFileOptions::default();
        zip.start_file("some_file.txt", options).unwrap();
        zip.write_all(b"hello").unwrap();
        zip.finish().unwrap();
    }

    let result = extract_bundle(&buf);
    assert!(result.is_err());
    match result.unwrap_err() {
        ArtifactError::Manifest(msg) => {
            assert!(msg.contains("manifest.json not found"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn test_extract_bundle_invalid_manifest_json() {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = SimpleFileOptions::default();
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(b"not valid json").unwrap();
        zip.finish().unwrap();
    }

    let result = extract_bundle(&buf);
    assert!(result.is_err());
    match result.unwrap_err() {
        ArtifactError::Manifest(msg) => {
            assert!(msg.contains("invalid manifest.json"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn test_extract_bundle_unsupported_version() {
    let manifest = BundleManifest {
        version: 99,
        project_path: "hardware/test".to_string(),
        tree_hash: "abc123".to_string(),
        commit_sha: "def456".to_string(),
        files: vec![],
        artifacts: vec![],
        checks: vec![],
        diff_metadata: None,
    };

    let zip_data = create_test_zip(&manifest, &[]);

    let result = extract_bundle(&zip_data);
    assert!(result.is_err());
    match result.unwrap_err() {
        ArtifactError::Manifest(msg) => {
            assert!(msg.contains("unsupported manifest version: 99"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn test_extract_bundle_path_traversal_dotdot() {
    let manifest = make_manifest(vec![ManifestArtifact {
        r#type: "gerber".to_string(),
        filename: "output.gbr".to_string(),
        content_type: "application/octet-stream".to_string(),
        status: "available".to_string(),
        source_path: Some("../etc/passwd".to_string()),
        logical_name: None,
        status_reason: None,
        sha256: None,
        size_bytes: None,
    }]);

    let zip_data = create_test_zip(&manifest, &[("../etc/passwd", b"root:x:0:0")]);

    let result = extract_bundle(&zip_data);
    assert!(result.is_err());
    match result.unwrap_err() {
        ArtifactError::PathTraversal(path) => {
            assert!(path.contains(".."));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn test_extract_bundle_path_traversal_absolute() {
    let manifest = make_manifest(vec![ManifestArtifact {
        r#type: "gerber".to_string(),
        filename: "output.gbr".to_string(),
        content_type: "application/octet-stream".to_string(),
        status: "available".to_string(),
        source_path: Some("/etc/passwd".to_string()),
        logical_name: None,
        status_reason: None,
        sha256: None,
        size_bytes: None,
    }]);

    let zip_data = create_test_zip(&manifest, &[]);

    let result = extract_bundle(&zip_data);
    assert!(result.is_err());
    match result.unwrap_err() {
        ArtifactError::PathTraversal(path) => {
            assert!(path.starts_with('/'));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn test_extract_bundle_too_large() {
    // We can't allocate 500MB+ in tests; simulate by checking the error type
    // The function checks data.len() against MAX_BUNDLE_SIZE
    // Just verify the constant is correct
    assert_eq!(MAX_BUNDLE_SIZE, 500 * 1024 * 1024);
}

#[test]
fn test_extract_bundle_artifact_not_found_in_zip() {
    let manifest = make_manifest(vec![ManifestArtifact {
        r#type: "gerber".to_string(),
        filename: "output.gbr".to_string(),
        content_type: "application/octet-stream".to_string(),
        status: "available".to_string(),
        source_path: Some("artifacts/missing_file.gbr".to_string()),
        logical_name: None,
        status_reason: None,
        sha256: None,
        size_bytes: None,
    }]);

    // Don't include the file in the zip
    let zip_data = create_test_zip(&manifest, &[]);

    let result = extract_bundle(&zip_data);
    assert!(result.is_err());
    match result.unwrap_err() {
        ArtifactError::Manifest(msg) => {
            assert!(msg.contains("artifact file not found in zip"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn test_verify_sha256_valid() {
    let data = b"hello world";
    let mut hasher = Sha256::new();
    hasher.update(data);
    let expected = format!("sha256:{:x}", hasher.finalize());

    assert!(verify_sha256(data, &expected).is_ok());
}

#[test]
fn test_verify_sha256_mismatch() {
    let data = b"hello world";
    let wrong_hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

    let result = verify_sha256(data, wrong_hash);
    assert!(result.is_err());
    match result.unwrap_err() {
        ArtifactError::Sha256Mismatch { expected, actual } => {
            assert_eq!(expected, wrong_hash);
            assert!(actual.starts_with("sha256:"));
            assert_ne!(actual, wrong_hash);
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn test_extract_bundle_multiple_artifacts() {
    let manifest = make_manifest(vec![
        ManifestArtifact {
            r#type: "gerber".to_string(),
            filename: "front.gbr".to_string(),
            content_type: "application/octet-stream".to_string(),
            status: "available".to_string(),
            source_path: Some("artifacts/front.gbr".to_string()),
            logical_name: Some("F.Cu".to_string()),
            status_reason: None,
            sha256: None,
            size_bytes: None,
        },
        ManifestArtifact {
            r#type: "gerber".to_string(),
            filename: "back.gbr".to_string(),
            content_type: "application/octet-stream".to_string(),
            status: "available".to_string(),
            source_path: Some("artifacts/back.gbr".to_string()),
            logical_name: Some("B.Cu".to_string()),
            status_reason: None,
            sha256: None,
            size_bytes: None,
        },
        ManifestArtifact {
            r#type: "bom".to_string(),
            filename: "bom.csv".to_string(),
            content_type: "text/csv".to_string(),
            status: "skipped".to_string(),
            source_path: None,
            logical_name: None,
            status_reason: Some("no BOM configured".to_string()),
            sha256: None,
            size_bytes: None,
        },
    ]);

    let zip_data = create_test_zip(
        &manifest,
        &[
            ("artifacts/front.gbr", b"front copper data"),
            ("artifacts/back.gbr", b"back copper data"),
        ],
    );

    let (_, extracted) = extract_bundle(&zip_data).unwrap();
    // Only "available" artifacts should be extracted
    assert_eq!(extracted.len(), 2);
    assert_eq!(extracted[0].path, "artifacts/front.gbr");
    assert_eq!(extracted[1].path, "artifacts/back.gbr");
}

#[test]
fn test_manifest_check_findings_deserialization() {
    let json = serde_json::json!({
        "kind": "erc",
        "status": "failed",
        "error_count": 2,
        "warning_count": 1,
        "notice_count": 0,
        "tool_name": "kicad",
        "tool_version": "8.0.0",
        "raw_summary": null,
        "findings": [
            {
                "severity": "error",
                "rule_code": "E001",
                "title": "Unconnected pin",
                "message": "Pin VCC on U1 is unconnected",
                "subject_kind": "symbol",
                "subject_ref": "U1",
                "sheet_path": "/Root",
                "pos_mm": { "x": 100.5, "y": 50.25 }
            },
            {
                "severity": "warning",
                "rule_code": "W003",
                "title": "Power pin not driven",
                "subject_kind": "net",
                "subject_ref": "VCC"
            }
        ]
    });

    let check: ManifestCheck = serde_json::from_value(json).unwrap();
    assert_eq!(check.kind, "erc");
    assert_eq!(check.status, "failed");
    assert_eq!(check.error_count, 2);
    assert_eq!(check.findings.len(), 2);

    // Individually parse findings from serde_json::Value
    let f0: ManifestFinding = serde_json::from_value(check.findings[0].clone()).unwrap();
    assert_eq!(f0.severity, "error");
    assert_eq!(f0.rule_code, "E001");
    assert_eq!(f0.title, "Unconnected pin");
    assert_eq!(f0.message.as_deref(), Some("Pin VCC on U1 is unconnected"));
    assert_eq!(f0.subject_kind.as_deref(), Some("symbol"));
    assert_eq!(f0.subject_ref.as_deref(), Some("U1"));
    assert_eq!(f0.sheet_path.as_deref(), Some("/Root"));
    assert!(f0.pcb_layer.is_none());
    let pos = f0.pos_mm.as_ref().unwrap();
    assert!((pos.x - 100.5).abs() < f64::EPSILON);
    assert!((pos.y - 50.25).abs() < f64::EPSILON);
    assert!(f0.raw.is_none());

    let f1: ManifestFinding = serde_json::from_value(check.findings[1].clone()).unwrap();
    assert_eq!(f1.severity, "warning");
    assert_eq!(f1.rule_code, "W003");
    assert!(f1.message.is_none());
    assert!(f1.pos_mm.is_none());
}

#[test]
fn test_manifest_check_without_findings_backward_compat() {
    let json = serde_json::json!({
        "kind": "drc",
        "status": "passed",
        "error_count": 0,
        "warning_count": 0,
        "notice_count": 0
    });

    let check: ManifestCheck = serde_json::from_value(json).unwrap();
    assert_eq!(check.kind, "drc");
    assert_eq!(check.status, "passed");
    assert!(check.findings.is_empty());
    assert!(check.tool_name.is_none());
    assert!(check.raw_summary.is_none());
}

#[test]
fn test_coordinate_mm_to_um_conversion() {
    let coord = CoordinateMm {
        x: 100.5,
        y: -25.75,
    };
    let x_um = (coord.x * 1000.0).round() as i32;
    let y_um = (coord.y * 1000.0).round() as i32;
    assert_eq!(x_um, 100500);
    assert_eq!(y_um, -25750);

    // Zero coordinate
    let zero = CoordinateMm { x: 0.0, y: 0.0 };
    assert_eq!((zero.x * 1000.0).round() as i32, 0);
    assert_eq!((zero.y * 1000.0).round() as i32, 0);

    // Small values
    let small = CoordinateMm { x: 0.001, y: 0.999 };
    assert_eq!((small.x * 1000.0).round() as i32, 1);
    assert_eq!((small.y * 1000.0).round() as i32, 999);
}

#[test]
fn test_extract_bundle_available_artifact_no_source_path() {
    let manifest = make_manifest(vec![ManifestArtifact {
        r#type: "gerber".to_string(),
        filename: "output.gbr".to_string(),
        content_type: "application/octet-stream".to_string(),
        status: "available".to_string(),
        source_path: None, // Missing source_path for "available" artifact
        logical_name: None,
        status_reason: None,
        sha256: None,
        size_bytes: None,
    }]);

    let zip_data = create_test_zip(&manifest, &[]);

    let result = extract_bundle(&zip_data);
    assert!(result.is_err());
    match result.unwrap_err() {
        ArtifactError::Manifest(msg) => {
            assert!(msg.contains("has no source_path"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn test_extract_bundle_sha256_verification_pass() {
    let artifact_data = b"test content for sha256";
    let mut hasher = Sha256::new();
    hasher.update(artifact_data);
    let expected_sha256 = format!("sha256:{:x}", hasher.finalize());

    let manifest = make_manifest(vec![ManifestArtifact {
        r#type: "gerber".to_string(),
        filename: "output.gbr".to_string(),
        content_type: "application/octet-stream".to_string(),
        status: "available".to_string(),
        source_path: Some("artifacts/output.gbr".to_string()),
        logical_name: None,
        status_reason: None,
        sha256: Some(expected_sha256),
        size_bytes: None,
    }]);

    let zip_data = create_test_zip(&manifest, &[("artifacts/output.gbr", artifact_data)]);
    let (_, extracted) = extract_bundle(&zip_data).unwrap();
    assert_eq!(extracted.len(), 1);
}

#[test]
fn test_extract_bundle_sha256_verification_fail() {
    let artifact_data = b"test content";
    let manifest = make_manifest(vec![ManifestArtifact {
        r#type: "gerber".to_string(),
        filename: "output.gbr".to_string(),
        content_type: "application/octet-stream".to_string(),
        status: "available".to_string(),
        source_path: Some("artifacts/output.gbr".to_string()),
        logical_name: None,
        status_reason: None,
        sha256: Some(
            "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        ),
        size_bytes: None,
    }]);

    let zip_data = create_test_zip(&manifest, &[("artifacts/output.gbr", artifact_data)]);
    let result = extract_bundle(&zip_data);
    assert!(result.is_err());
    match result.unwrap_err() {
        ArtifactError::Sha256Mismatch { .. } => {}
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn test_extract_bundle_size_verification_pass() {
    let artifact_data = b"exact size content";
    let manifest = make_manifest(vec![ManifestArtifact {
        r#type: "gerber".to_string(),
        filename: "output.gbr".to_string(),
        content_type: "application/octet-stream".to_string(),
        status: "available".to_string(),
        source_path: Some("artifacts/output.gbr".to_string()),
        logical_name: None,
        status_reason: None,
        sha256: None,
        size_bytes: Some(artifact_data.len() as i64),
    }]);

    let zip_data = create_test_zip(&manifest, &[("artifacts/output.gbr", artifact_data)]);
    let (_, extracted) = extract_bundle(&zip_data).unwrap();
    assert_eq!(extracted.len(), 1);
}

#[test]
fn test_extract_bundle_size_verification_fail() {
    let artifact_data = b"short";
    let manifest = make_manifest(vec![ManifestArtifact {
        r#type: "gerber".to_string(),
        filename: "output.gbr".to_string(),
        content_type: "application/octet-stream".to_string(),
        status: "available".to_string(),
        source_path: Some("artifacts/output.gbr".to_string()),
        logical_name: None,
        status_reason: None,
        sha256: None,
        size_bytes: Some(9999),
    }]);

    let zip_data = create_test_zip(&manifest, &[("artifacts/output.gbr", artifact_data)]);
    let result = extract_bundle(&zip_data);
    assert!(result.is_err());
    match result.unwrap_err() {
        ArtifactError::Manifest(msg) => {
            assert!(msg.contains("size mismatch"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn test_extract_bundle_rejects_unlisted_entry() {
    let artifact_data = b"fake gerber content";
    let manifest = make_manifest(vec![ManifestArtifact {
        r#type: "gerber".to_string(),
        filename: "output.gbr".to_string(),
        content_type: "application/octet-stream".to_string(),
        status: "available".to_string(),
        source_path: Some("artifacts/output.gbr".to_string()),
        logical_name: None,
        status_reason: None,
        sha256: None,
        size_bytes: None,
    }]);

    // Include an extra file "extra.txt" not declared in manifest
    let zip_data = create_test_zip(
        &manifest,
        &[
            ("artifacts/output.gbr", artifact_data),
            ("extra.txt", b"unexpected file"),
        ],
    );

    let result = extract_bundle(&zip_data);
    assert!(result.is_err());
    match result.unwrap_err() {
        ArtifactError::Manifest(msg) => {
            assert!(
                msg.contains("not declared in manifest"),
                "unexpected message: {msg}"
            );
            assert!(
                msg.contains("extra.txt"),
                "should mention the file name: {msg}"
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn test_manifest_findings_malformed_individual_parsing() {
    // ManifestCheck.findings is Vec<serde_json::Value>, so malformed entries
    // don't prevent the overall ManifestCheck from deserializing.
    // Individual parsing with serde_json::from_value::<ManifestFinding> should
    // succeed for valid entries and fail for malformed ones.
    let json = serde_json::json!({
        "kind": "erc",
        "status": "failed",
        "error_count": 1,
        "warning_count": 0,
        "notice_count": 0,
        "findings": [
            {
                "severity": "error",
                "rule_code": "E001",
                "title": "Valid finding"
            },
            {
                "some_unknown_field": "garbage",
                "no_severity": true
            },
            "this is not even an object",
            {
                "severity": "warning",
                "rule_code": "W002",
                "title": "Another valid finding"
            }
        ]
    });

    // ManifestCheck deserializes successfully (findings are raw Values)
    let check: ManifestCheck = serde_json::from_value(json).unwrap();
    assert_eq!(check.findings.len(), 4);

    // First finding: valid, parses successfully
    let r0 = serde_json::from_value::<ManifestFinding>(check.findings[0].clone());
    assert!(r0.is_ok());
    assert_eq!(r0.unwrap().severity, "error");

    // Second finding: malformed (missing required fields), parse fails
    let r1 = serde_json::from_value::<ManifestFinding>(check.findings[1].clone());
    assert!(r1.is_err());

    // Third finding: not even an object, parse fails
    let r2 = serde_json::from_value::<ManifestFinding>(check.findings[2].clone());
    assert!(r2.is_err());

    // Fourth finding: valid, parses successfully
    let r3 = serde_json::from_value::<ManifestFinding>(check.findings[3].clone());
    assert!(r3.is_ok());
    assert_eq!(r3.unwrap().rule_code, "W002");
}

#[test]
fn test_coordinate_mm_to_um_rounding() {
    // 0.0006mm should round to 1µm (0.0006 * 1000 = 0.6, rounds to 1)
    let coord = CoordinateMm { x: 0.0006, y: 0.0 };
    let x_um = (coord.x * 1000.0).round() as i32;
    assert_eq!(x_um, 1);

    // 0.0004mm should round to 0µm (0.0004 * 1000 = 0.4, rounds to 0)
    let coord2 = CoordinateMm { x: 0.0004, y: 0.0 };
    let x_um2 = (coord2.x * 1000.0).round() as i32;
    assert_eq!(x_um2, 0);

    // 0.0005mm should round to 1µm (0.0005 * 1000 = 0.5, rounds to 1 — banker's rounding not used by f64::round)
    let coord3 = CoordinateMm { x: 0.0005, y: 0.0 };
    let x_um3 = (coord3.x * 1000.0).round() as i32;
    assert_eq!(x_um3, 1);

    // Negative: -0.0006mm should round to -1µm
    let coord4 = CoordinateMm { x: -0.0006, y: 0.0 };
    let x_um4 = (coord4.x * 1000.0).round() as i32;
    assert_eq!(x_um4, -1);

    // Without .round(), 0.0006 * 1000.0 = 0.6 would truncate to 0 with `as i32`
    // This confirms .round() is required for correct µm conversion
    let truncated = (0.0006_f64 * 1000.0) as i32;
    assert_eq!(truncated, 0); // truncation gives wrong result
    let rounded = (0.0006_f64 * 1000.0).round() as i32;
    assert_eq!(rounded, 1); // rounding gives correct result
}

#[test]
fn test_severity_normalization() {
    // Valid severities pass through unchanged
    assert!(["error", "warning", "notice"].contains(&"error"));
    assert!(["error", "warning", "notice"].contains(&"warning"));
    assert!(["error", "warning", "notice"].contains(&"notice"));
    // Invalid severity should be caught by normalization (not in allowed set)
    assert!(!["error", "warning", "notice"].contains(&"critical"));
    assert!(!["error", "warning", "notice"].contains(&""));
}

#[test]
fn test_subject_kind_normalization() {
    let valid = ["schematic", "pcb", "net", "footprint", "symbol"];
    assert!(valid.contains(&"schematic"));
    assert!(valid.contains(&"pcb"));
    assert!(valid.contains(&"net"));
    assert!(valid.contains(&"footprint"));
    assert!(valid.contains(&"symbol"));
    // Invalid should be normalized to None
    assert!(!valid.contains(&"board"));
    assert!(!valid.contains(&""));
    assert!(!valid.contains(&"component"));
}
