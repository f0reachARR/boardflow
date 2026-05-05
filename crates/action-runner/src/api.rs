use std::path::Path;
use std::time::Duration;

use reqwest::Client;
use serde::Serialize;
use serde_json::Value;

use crate::error::{ActionError, Result};

pub struct ApiClient {
    client: Client,
    base_url: String,
    token: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProjectDecision {
    pub project_path: String,
    pub decision: String,
    #[serde(default)]
    pub board_project_id: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CreateBoardRunResponse {
    pub board_run_id: String,
    pub artifact_bundle: ArtifactBundleInfo,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ArtifactBundleInfo {
    pub upload_url: String,
    pub object_key: String,
}

impl ApiClient {
    pub fn new(base_url: &str, token: &str) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(60))
            .use_rustls_tls()
            .build()
            .expect("failed to build HTTP client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }
    }

    async fn request(
        &self,
        method: reqwest::Method,
        endpoint: &str,
        body: Option<&Value>,
    ) -> Result<Value> {
        let url = format!("{}{}", self.base_url, endpoint);
        let max_retries = 3;
        let mut last_error = None;

        for attempt in 0..max_retries {
            let mut req = self
                .client
                .request(method.clone(), &url)
                .header("Authorization", format!("Bearer {}", self.token))
                .header("Content-Type", "application/json");

            if let Some(b) = body {
                req = req.json(b);
            }

            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let json = resp.json::<Value>().await.map_err(|e| {
                            ActionError::Api(format!("Failed to parse response: {e}"))
                        })?;
                        return Ok(json);
                    }
                    if status.is_client_error() {
                        let body_text = resp.text().await.unwrap_or_default();
                        return Err(ActionError::Api(format!(
                            "HTTP {status} from {endpoint}: {body_text}"
                        )));
                    }
                    // 5xx → retry
                    last_error = Some(ActionError::Api(format!("HTTP {status} from {endpoint}")));
                }
                Err(e) => {
                    if e.is_timeout() || e.is_connect() {
                        last_error = Some(ActionError::Api(format!(
                            "Request to {endpoint} failed: {e}"
                        )));
                    } else {
                        return Err(ActionError::Api(format!(
                            "Request to {endpoint} failed: {e}"
                        )));
                    }
                }
            }

            if attempt < max_retries - 1 {
                let backoff = Duration::from_secs(1 << attempt);
                tokio::time::sleep(backoff).await;
            }
        }

        Err(last_error.unwrap_or_else(|| ActionError::Api("Unknown error".to_string())))
    }

    pub async fn plan(&self, payload: &Value) -> Result<Vec<ProjectDecision>> {
        let resp = self
            .request(reqwest::Method::POST, "/api/v1/runs/plan", Some(payload))
            .await?;

        let projects = resp
            .get("projects")
            .ok_or_else(|| ActionError::Api("Plan response missing 'projects' field".into()))?;

        let decisions: Vec<ProjectDecision> = serde_json::from_value(projects.clone())
            .map_err(|e| ActionError::Api(format!("Failed to parse plan decisions: {e}")))?;

        Ok(decisions)
    }

    pub async fn create_board_run(&self, payload: &Value) -> Result<CreateBoardRunResponse> {
        let resp = self
            .request(reqwest::Method::POST, "/api/v1/board-runs", Some(payload))
            .await?;

        let response: CreateBoardRunResponse = serde_json::from_value(resp)
            .map_err(|e| ActionError::Api(format!("Failed to parse create_board_run: {e}")))?;

        Ok(response)
    }

    pub async fn import(&self, board_run_id: &str, payload: &Value) -> Result<()> {
        let endpoint = format!("/api/v1/board-runs/{board_run_id}/artifact-bundles/import");
        self.request(reqwest::Method::POST, &endpoint, Some(payload))
            .await?;
        Ok(())
    }

    pub async fn fail(&self, board_run_id: &str, message: &str, details: &str) -> Result<()> {
        let endpoint = format!("/api/v1/board-runs/{board_run_id}/fail");
        let payload = serde_json::json!({
            "status": "failed",
            "error": {
                "message": message,
                "details": details,
            }
        });
        self.request(reqwest::Method::POST, &endpoint, Some(&payload))
            .await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn upload_bundle(&self, upload_url: &str, bundle_path: &Path) -> Result<()> {
        let data = tokio::fs::read(bundle_path)
            .await
            .map_err(|e| ActionError::Upload(format!("Failed to read bundle: {e}")))?;

        let upload_client = Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(600))
            .use_rustls_tls()
            .build()
            .map_err(|e| ActionError::Upload(format!("Failed to build upload client: {e}")))?;

        let resp = upload_client
            .put(upload_url)
            .header("Content-Type", "application/zip")
            .body(data)
            .send()
            .await
            .map_err(|e| ActionError::Upload(format!("Upload request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(ActionError::Upload(format!(
                "Upload failed with HTTP {}",
                resp.status()
            )));
        }

        Ok(())
    }
}

#[derive(Serialize)]
#[allow(dead_code)]
pub struct PlanProject {
    pub project_path: String,
    pub config_path: String,
    pub project_dir: String,
    pub tree_hash: String,
    pub files: Vec<PlanFile>,
}

#[derive(Serialize)]
#[allow(dead_code)]
pub struct PlanFile {
    pub path: String,
    pub sha256: String,
}
