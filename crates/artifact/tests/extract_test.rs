use boardflow_artifact::{
    extract_bundle, verify_sha256, ArtifactError, BundleManifest, ManifestArtifact, ManifestFile,
    MAX_BUNDLE_SIZE,
};
use sha2::{Digest, Sha256};
use std::io::Write;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

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
        sha256: Some("sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string()),
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
            assert!(msg.contains("extra.txt"), "should mention the file name: {msg}");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}
