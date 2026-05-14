use boardflow_artifact::{
    ArtifactError, BundleManifest, download_bundle, extract_bundle, upload_artifact, verify_sha256,
};
use uuid::Uuid;

use crate::config::WorkerConfig;

/// Downloaded artifact ready for DB insertion.
pub(super) struct UploadedArtifact {
    pub storage_key: String,
    pub sha256: String,
    pub size: i64,
    pub manifest_idx: usize,
}

/// Download the bundle from the staging bucket and verify its SHA-256 digest.
pub(super) async fn download_and_verify(
    s3_client: &aws_sdk_s3::Client,
    config: &WorkerConfig,
    staging_object_key: &str,
    expected_sha256: &str,
) -> Result<Vec<u8>, ArtifactError> {
    tracing::info!(key = %staging_object_key, "Downloading bundle from S3");
    let data = download_bundle(s3_client, &config.s3.staging_bucket, staging_object_key).await?;
    verify_sha256(&data, expected_sha256)?;
    Ok(data)
}

/// Extract the bundle ZIP and upload each available artifact to the final bucket.
pub(super) async fn extract_and_upload(
    s3_client: &aws_sdk_s3::Client,
    config: &WorkerConfig,
    data: &[u8],
    board_run_id: Uuid,
) -> Result<(BundleManifest, Vec<UploadedArtifact>), ArtifactError> {
    tracing::info!("Extracting bundle");
    let (manifest, extracted_artifacts) = extract_bundle(data)?;

    tracing::info!(
        count = extracted_artifacts.len(),
        "Uploading artifacts to final bucket"
    );

    let mut uploaded: Vec<UploadedArtifact> = Vec::new();

    for (idx, manifest_entry) in manifest.artifacts.iter().enumerate() {
        if manifest_entry.status != "available" {
            continue;
        }
        let storage_key = format!(
            "artifacts/{board_run_id}/{}/{}",
            manifest_entry.r#type, manifest_entry.filename
        );

        if let Some(extracted) = extracted_artifacts
            .iter()
            .find(|a| manifest_entry.source_path.as_deref() == Some(&a.path))
        {
            upload_artifact(
                s3_client,
                &config.s3.final_bucket,
                &storage_key,
                extracted.data.clone(),
                &manifest_entry.content_type,
            )
            .await?;

            uploaded.push(UploadedArtifact {
                storage_key,
                sha256: extracted.sha256.clone(),
                size: extracted.data.len() as i64,
                manifest_idx: idx,
            });
        }
    }

    Ok((manifest, uploaded))
}
