use aws_sdk_s3::Client as S3Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read};
use zip::ZipArchive;

#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error("S3 error: {0}")]
    S3(String),
    #[error("SHA256 mismatch: expected {expected}, got {actual}")]
    Sha256Mismatch { expected: String, actual: String },
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("manifest error: {0}")]
    Manifest(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("bundle too large: {size} bytes exceeds limit {limit}")]
    TooLarge { size: u64, limit: u64 },
    #[error("path traversal detected: {0}")]
    PathTraversal(String),
}

/// Maximum bundle size (500 MB)
pub const MAX_BUNDLE_SIZE: u64 = 500 * 1024 * 1024;

/// Manifest v1 schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleManifest {
    pub version: u32,
    pub project_path: String,
    pub tree_hash: String,
    pub commit_sha: String,
    pub files: Vec<ManifestFile>,
    pub artifacts: Vec<ManifestArtifact>,
    #[serde(default)]
    pub checks: Vec<ManifestCheck>,
    #[serde(default)]
    pub diff_metadata: Option<ManifestDiffMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestArtifact {
    pub r#type: String,
    pub filename: String,
    pub content_type: String,
    pub status: String,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub logical_name: Option<String>,
    #[serde(default)]
    pub status_reason: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestCheck {
    pub kind: String,
    pub status: String,
    #[serde(default)]
    pub error_count: i32,
    #[serde(default)]
    pub warning_count: i32,
    #[serde(default)]
    pub notice_count: i32,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_version: Option<String>,
    #[serde(default)]
    pub raw_summary: Option<serde_json::Value>,
    #[serde(default)]
    pub findings: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFinding {
    pub severity: String,
    pub rule_code: String,
    pub title: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub subject_kind: Option<String>,
    #[serde(default)]
    pub subject_ref: Option<String>,
    #[serde(default)]
    pub sheet_path: Option<String>,
    #[serde(default)]
    pub pcb_layer: Option<String>,
    #[serde(default)]
    pub pos_mm: Option<CoordinateMm>,
    #[serde(default)]
    pub raw: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinateMm {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestDiffMetadata {
    #[serde(default)]
    pub file_hashes: Option<serde_json::Value>,
    #[serde(default)]
    pub bom_summary: Option<serde_json::Value>,
    #[serde(default)]
    pub checks_summary: Option<serde_json::Value>,
    #[serde(default)]
    pub artifacts_summary: Option<serde_json::Value>,
    #[serde(default)]
    pub previews: Option<serde_json::Value>,
}

/// An extracted artifact file from the ZIP bundle
#[derive(Debug)]
pub struct ExtractedArtifact {
    pub path: String,
    pub data: Vec<u8>,
    pub sha256: String,
}

/// Download a bundle from S3
pub async fn download_bundle(
    s3_client: &S3Client,
    bucket: &str,
    key: &str,
) -> Result<Vec<u8>, ArtifactError> {
    let resp = s3_client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;

    let data = resp
        .body
        .collect()
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?
        .into_bytes()
        .to_vec();

    Ok(data)
}

/// Verify SHA256 of downloaded bundle
pub fn verify_sha256(data: &[u8], expected: &str) -> Result<(), ArtifactError> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let actual = format!("sha256:{:x}", hasher.finalize());
    if actual != expected {
        return Err(ArtifactError::Sha256Mismatch {
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}

/// Extract and validate ZIP bundle, returning manifest and artifact files
pub fn extract_bundle(
    data: &[u8],
) -> Result<(BundleManifest, Vec<ExtractedArtifact>), ArtifactError> {
    if data.len() as u64 > MAX_BUNDLE_SIZE {
        return Err(ArtifactError::TooLarge {
            size: data.len() as u64,
            limit: MAX_BUNDLE_SIZE,
        });
    }

    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor)?;

    // Read manifest.json first
    let manifest: BundleManifest = {
        let mut manifest_file = archive
            .by_name("manifest.json")
            .map_err(|_| ArtifactError::Manifest("manifest.json not found in bundle".into()))?;
        let mut buf = Vec::new();
        manifest_file.read_to_end(&mut buf)?;
        serde_json::from_slice(&buf)
            .map_err(|e| ArtifactError::Manifest(format!("invalid manifest.json: {e}")))?
    };

    if manifest.version != 1 {
        return Err(ArtifactError::Manifest(format!(
            "unsupported manifest version: {}",
            manifest.version
        )));
    }

    // Extract artifact files referenced in manifest
    let mut extracted = Vec::new();
    for entry in &manifest.artifacts {
        if entry.status != "available" {
            continue;
        }
        let source_path = entry.source_path.as_deref().ok_or_else(|| {
            ArtifactError::Manifest(format!("artifact {} has no source_path", entry.filename))
        })?;

        // Security: validate path
        let safe_path = validate_path(source_path)?;

        let mut file = archive.by_name(&safe_path).map_err(|_| {
            ArtifactError::Manifest(format!("artifact file not found in zip: {safe_path}"))
        })?;

        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        let mut hasher = Sha256::new();
        hasher.update(&buf);
        let sha256 = format!("sha256:{:x}", hasher.finalize());

        // Verify sha256 if specified in manifest
        if let Some(ref expected_sha256) = entry.sha256 {
            if &sha256 != expected_sha256 {
                return Err(ArtifactError::Sha256Mismatch {
                    expected: expected_sha256.clone(),
                    actual: sha256,
                });
            }
        }
        // Verify size if specified
        if let Some(expected_size) = entry.size_bytes {
            if buf.len() as i64 != expected_size {
                return Err(ArtifactError::Manifest(format!(
                    "artifact {} size mismatch: expected {}, got {}",
                    entry.filename, expected_size, buf.len()
                )));
            }
        }

        extracted.push(ExtractedArtifact {
            path: safe_path,
            data: buf,
            sha256,
        });
    }

    // Reject zip entries not declared in manifest
    let mut allowed_paths: std::collections::HashSet<&str> = std::collections::HashSet::new();
    allowed_paths.insert("manifest.json");
    for entry in &manifest.artifacts {
        if let Some(ref sp) = entry.source_path {
            allowed_paths.insert(sp.as_str());
        }
    }

    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        let name = file.name().to_string();
        if file.is_dir() {
            continue;
        }
        if !allowed_paths.contains(name.as_str()) {
            return Err(ArtifactError::Manifest(format!(
                "zip contains entry not declared in manifest: {}",
                name
            )));
        }
    }

    Ok((manifest, extracted))
}

/// Validate a path from zip archive for security
fn validate_path(path: &str) -> Result<String, ArtifactError> {
    // Reject absolute paths
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(ArtifactError::PathTraversal(path.to_string()));
    }
    // Reject path traversal
    if path.contains("..") {
        return Err(ArtifactError::PathTraversal(path.to_string()));
    }
    Ok(path.to_string())
}

/// Upload an artifact to the final S3 bucket
pub async fn upload_artifact(
    s3_client: &S3Client,
    bucket: &str,
    storage_key: &str,
    data: Vec<u8>,
    content_type: &str,
) -> Result<(), ArtifactError> {
    s3_client
        .put_object()
        .bucket(bucket)
        .key(storage_key)
        .body(data.into())
        .content_type(content_type)
        .send()
        .await
        .map_err(|e| ArtifactError::S3(e.to_string()))?;
    Ok(())
}

