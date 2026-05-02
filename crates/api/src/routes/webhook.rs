use axum::Extension;
use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use sqlx::PgPool;

use crate::WebhookSecret;

type HmacSha256 = Hmac<Sha256>;

fn verify_signature(secret: &[u8], body: &[u8], signature_header: &str) -> bool {
    let hex_signature = match signature_header.strip_prefix("sha256=") {
        Some(hex) => hex,
        None => return false,
    };

    let expected = match hex::decode(hex_signature) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };

    let mut mac = match HmacSha256::new_from_slice(secret) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

// --- Payload types ---

#[derive(Debug, Deserialize)]
struct WebhookRepository {
    id: i64,
    #[allow(dead_code)]
    name: String,
    full_name: String,
}

#[derive(Debug, Deserialize)]
struct WebhookInstallation {
    id: i64,
}

#[derive(Debug, Deserialize)]
struct InstallationEvent {
    action: String,
    installation: WebhookInstallation,
    #[serde(default)]
    repositories: Vec<WebhookRepository>,
}

#[derive(Debug, Deserialize)]
struct InstallationRepositoriesEvent {
    action: String,
    installation: WebhookInstallation,
    #[serde(default)]
    repositories_added: Vec<WebhookRepository>,
    #[serde(default)]
    repositories_removed: Vec<WebhookRepository>,
}

// --- Handler ---

pub async fn github_webhook(
    Extension(pool): Extension<PgPool>,
    Extension(webhook_secret): Extension<WebhookSecret>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let secret = match &webhook_secret.0 {
        Some(s) => s.clone(),
        None => {
            tracing::error!("webhook secret is not configured");
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    };

    let signature = match headers
        .get("X-Hub-Signature-256")
        .and_then(|v| v.to_str().ok())
    {
        Some(sig) => sig.to_string(),
        None => {
            tracing::warn!("missing X-Hub-Signature-256 header");
            return StatusCode::UNAUTHORIZED;
        }
    };

    if !verify_signature(secret.as_bytes(), &body, &signature) {
        tracing::warn!("invalid webhook signature");
        return StatusCode::UNAUTHORIZED;
    }

    let event = headers
        .get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let delivery_id = headers
        .get("X-GitHub-Delivery")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    tracing::info!(
        event = event,
        delivery_id = delivery_id,
        "received webhook event"
    );

    match event {
        "ping" => StatusCode::OK,
        "installation" => handle_installation_event(&pool, &body).await,
        "installation_repositories" => handle_installation_repositories_event(&pool, &body).await,
        _ => {
            tracing::debug!(event = event, "ignoring unhandled webhook event");
            StatusCode::OK
        }
    }
}

async fn handle_installation_event(pool: &PgPool, body: &[u8]) -> StatusCode {
    let event: InstallationEvent = match serde_json::from_slice(body) {
        Ok(e) => e,
        Err(err) => {
            tracing::error!(error = %err, "failed to parse installation event");
            return StatusCode::BAD_REQUEST;
        }
    };

    match event.action.as_str() {
        "created" => {
            for repo in &event.repositories {
                if let Some((owner, name)) = repo.full_name.split_once('/') {
                    if let Err(err) = boardflow_db::queries::repository::upsert(
                        pool,
                        repo.id,
                        owner,
                        name,
                        event.installation.id,
                    )
                    .await
                    {
                        tracing::error!(
                            error = %err,
                            repo_id = repo.id,
                            full_name = %repo.full_name,
                            "failed to upsert repository"
                        );
                        return StatusCode::INTERNAL_SERVER_ERROR;
                    }
                } else {
                    tracing::warn!(
                        full_name = %repo.full_name,
                        "invalid full_name format, expected owner/name"
                    );
                }
            }
            StatusCode::OK
        }
        "deleted" => {
            if let Err(err) =
                boardflow_db::queries::repository::clear_installation(pool, event.installation.id)
                    .await
            {
                tracing::error!(
                    error = %err,
                    installation_id = event.installation.id,
                    "failed to clear installation"
                );
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
            StatusCode::OK
        }
        _ => {
            tracing::debug!(action = %event.action, "ignoring installation action");
            StatusCode::OK
        }
    }
}

async fn handle_installation_repositories_event(pool: &PgPool, body: &[u8]) -> StatusCode {
    let event: InstallationRepositoriesEvent = match serde_json::from_slice(body) {
        Ok(e) => e,
        Err(err) => {
            tracing::error!(error = %err, "failed to parse installation_repositories event");
            return StatusCode::BAD_REQUEST;
        }
    };

    match event.action.as_str() {
        "added" => {
            for repo in &event.repositories_added {
                if let Some((owner, name)) = repo.full_name.split_once('/') {
                    if let Err(err) = boardflow_db::queries::repository::upsert(
                        pool,
                        repo.id,
                        owner,
                        name,
                        event.installation.id,
                    )
                    .await
                    {
                        tracing::error!(
                            error = %err,
                            repo_id = repo.id,
                            full_name = %repo.full_name,
                            "failed to upsert repository"
                        );
                        return StatusCode::INTERNAL_SERVER_ERROR;
                    }
                }
            }
            StatusCode::OK
        }
        "removed" => {
            for repo in &event.repositories_removed {
                if let Err(err) = boardflow_db::queries::repository::clear_installation_for_repo(
                    pool,
                    repo.id,
                    event.installation.id,
                )
                .await
                {
                    tracing::error!(
                        error = %err,
                        repo_id = repo.id,
                        "failed to clear installation for repository"
                    );
                    return StatusCode::INTERNAL_SERVER_ERROR;
                }
            }
            StatusCode::OK
        }
        _ => {
            tracing::debug!(
                action = %event.action,
                "ignoring installation_repositories action"
            );
            StatusCode::OK
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- verify_signature tests ---

    #[test]
    fn test_verify_signature_valid() {
        // GitHub official test values
        let secret = b"It's a Secret to Everybody";
        let payload = b"Hello, World!";
        let signature = "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17";
        assert!(verify_signature(secret, payload, signature));
    }

    #[test]
    fn test_verify_signature_invalid() {
        let secret = b"It's a Secret to Everybody";
        let payload = b"Hello, World!";
        let signature = "sha256=0000000000000000000000000000000000000000000000000000000000000000";
        assert!(!verify_signature(secret, payload, signature));
    }

    #[test]
    fn test_verify_signature_missing_prefix() {
        let secret = b"It's a Secret to Everybody";
        let payload = b"Hello, World!";
        let signature = "757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17";
        assert!(!verify_signature(secret, payload, signature));
    }

    #[test]
    fn test_verify_signature_invalid_hex() {
        let secret = b"It's a Secret to Everybody";
        let payload = b"Hello, World!";
        let signature = "sha256=not-valid-hex";
        assert!(!verify_signature(secret, payload, signature));
    }

    #[test]
    fn test_verify_signature_wrong_secret() {
        let secret = b"wrong-secret";
        let payload = b"Hello, World!";
        let signature = "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17";
        assert!(!verify_signature(secret, payload, signature));
    }

    #[test]
    fn test_verify_signature_empty_body() {
        let secret = b"test-secret";
        let payload = b"";
        // Compute expected signature for empty body with "test-secret"
        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(payload);
        let result = mac.finalize().into_bytes();
        let signature = format!("sha256={}", hex::encode(result));
        assert!(verify_signature(secret, payload, signature.as_str()));
    }

    // --- Payload deserialization tests ---

    #[test]
    fn test_deserialize_installation_event_created() {
        let json = r#"{
            "action": "created",
            "installation": { "id": 12345 },
            "repositories": [
                { "id": 100, "name": "repo1", "full_name": "owner/repo1" },
                { "id": 200, "name": "repo2", "full_name": "owner/repo2" }
            ]
        }"#;
        let event: InstallationEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.action, "created");
        assert_eq!(event.installation.id, 12345);
        assert_eq!(event.repositories.len(), 2);
        assert_eq!(event.repositories[0].id, 100);
        assert_eq!(event.repositories[0].full_name, "owner/repo1");
    }

    #[test]
    fn test_deserialize_installation_event_deleted_no_repos() {
        let json = r#"{
            "action": "deleted",
            "installation": { "id": 99999 }
        }"#;
        let event: InstallationEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.action, "deleted");
        assert_eq!(event.installation.id, 99999);
        assert!(event.repositories.is_empty());
    }

    #[test]
    fn test_deserialize_installation_repositories_event_added() {
        let json = r#"{
            "action": "added",
            "installation": { "id": 12345 },
            "repositories_added": [
                { "id": 300, "name": "repo3", "full_name": "org/repo3" }
            ],
            "repositories_removed": []
        }"#;
        let event: InstallationRepositoriesEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.action, "added");
        assert_eq!(event.repositories_added.len(), 1);
        assert_eq!(event.repositories_added[0].full_name, "org/repo3");
        assert!(event.repositories_removed.is_empty());
    }

    #[test]
    fn test_deserialize_installation_repositories_event_removed() {
        let json = r#"{
            "action": "removed",
            "installation": { "id": 12345 },
            "repositories_added": [],
            "repositories_removed": [
                { "id": 400, "name": "repo4", "full_name": "org/repo4" }
            ]
        }"#;
        let event: InstallationRepositoriesEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.action, "removed");
        assert!(event.repositories_added.is_empty());
        assert_eq!(event.repositories_removed.len(), 1);
        assert_eq!(event.repositories_removed[0].id, 400);
    }

    #[test]
    fn test_deserialize_installation_event_ignores_extra_fields() {
        let json = r#"{
            "action": "created",
            "installation": { "id": 1, "account": { "login": "user" } },
            "repositories": [],
            "sender": { "login": "user" }
        }"#;
        let event: InstallationEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.action, "created");
        assert_eq!(event.installation.id, 1);
    }
}
