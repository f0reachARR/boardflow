use super::cached::{CachedGithubAccessChecker, SYNC_CACHE_TYPE, SYNC_TTL_SECONDS};

// ─── Fallback sync: Installation Repositories API ────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct InstallationInfo {
    id: u64,
    app_id: u64,
    suspended_at: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct InstallationsResponse {
    installations: Vec<InstallationInfo>,
}

#[derive(Debug, serde::Deserialize)]
struct InstallationRepo {
    id: i64,
    full_name: String,
}

#[derive(Debug, serde::Deserialize)]
struct InstallationReposResponse {
    repositories: Vec<InstallationRepo>,
}

impl CachedGithubAccessChecker {
    pub(super) async fn maybe_sync_installation_repos(
        &self,
        github_access_token: &str,
        user_id: uuid::Uuid,
        accessible_ids: &[i64],
    ) {
        let app_id = match self.github_app_id {
            Some(id) => id,
            None => return,
        };

        if accessible_ids.is_empty() {
            return;
        }

        // Check which ids are missing from DB
        let existing = match boardflow_db::queries::repository::find_existing_github_ids(
            &self.pool,
            accessible_ids,
        )
        .await
        {
            Ok(ids) => ids,
            Err(e) => {
                tracing::warn!(error = %e, "failed to check existing repos for sync");
                return;
            }
        };

        let existing_set: std::collections::HashSet<i64> = existing.into_iter().collect();
        let missing: Vec<i64> = accessible_ids
            .iter()
            .filter(|id| !existing_set.contains(id))
            .copied()
            .collect();

        if missing.is_empty() {
            return;
        }

        // Throttle check
        if let Ok(Some(_)) = boardflow_db::queries::github_api_cache::get_valid_cache(
            &self.pool,
            user_id,
            SYNC_CACHE_TYPE,
        )
        .await
        {
            return; // Already synced recently
        }

        tracing::info!(
            user_id = %user_id,
            missing_count = missing.len(),
            "triggering installation repos fallback sync"
        );

        // Fetch installations
        let client = reqwest::Client::new();
        let installations = match self
            .fetch_user_installations(&client, github_access_token)
            .await
        {
            Ok(installs) => installs,
            Err(e) => {
                tracing::warn!(error = %e, "fallback sync: failed to fetch installations");
                return;
            }
        };

        // Filter by app_id and not suspended
        let relevant_installations: Vec<&InstallationInfo> = installations
            .iter()
            .filter(|i| i.app_id == app_id && i.suspended_at.is_none())
            .collect();

        // For each installation, fetch repos and upsert
        for installation in relevant_installations {
            let repos = match self
                .fetch_installation_repos(&client, github_access_token, installation.id)
                .await
            {
                Ok(repos) => repos,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        installation_id = installation.id,
                        "fallback sync: failed to fetch repos for installation"
                    );
                    continue;
                }
            };

            for repo in &repos {
                if let Some((owner, name)) = repo.full_name.split_once('/') {
                    if let Err(e) = boardflow_db::queries::repository::upsert(
                        &self.pool,
                        repo.id,
                        owner,
                        name,
                        installation.id as i64,
                    )
                    .await
                    {
                        tracing::warn!(
                            error = %e,
                            repo_id = repo.id,
                            full_name = %repo.full_name,
                            "fallback sync: failed to upsert repository"
                        );
                    }
                }
            }
        }

        // Update throttle cache
        let _ = boardflow_db::queries::github_api_cache::upsert_cache(
            &self.pool,
            user_id,
            SYNC_CACHE_TYPE,
            &serde_json::json!({"synced": true}),
            SYNC_TTL_SECONDS,
        )
        .await;
    }

    async fn fetch_user_installations(
        &self,
        client: &reqwest::Client,
        github_access_token: &str,
    ) -> Result<Vec<InstallationInfo>, String> {
        let mut all_installations = Vec::new();
        let mut page = 1u32;

        loop {
            let url = format!(
                "{}/user/installations?per_page=100&page={page}",
                self.github_api_base_url
            );

            let resp = client
                .get(&url)
                .header("Authorization", format!("Bearer {}", github_access_token))
                .header("User-Agent", "BoardFlow")
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send()
                .await
                .map_err(|e| format!("request failed: {e}"))?;

            if !resp.status().is_success() {
                return Err(format!("status {}", resp.status()));
            }

            let data: InstallationsResponse = resp
                .json()
                .await
                .map_err(|e| format!("parse failed: {e}"))?;

            let count = data.installations.len();
            all_installations.extend(data.installations);

            if count < 100 {
                break;
            }
            page += 1;
        }

        Ok(all_installations)
    }

    async fn fetch_installation_repos(
        &self,
        client: &reqwest::Client,
        github_access_token: &str,
        installation_id: u64,
    ) -> Result<Vec<InstallationRepo>, String> {
        let mut all_repos = Vec::new();
        let mut page = 1u32;

        loop {
            let url = format!(
                "{}/user/installations/{installation_id}/repositories?per_page=100&page={page}",
                self.github_api_base_url
            );

            let resp = client
                .get(&url)
                .header("Authorization", format!("Bearer {}", github_access_token))
                .header("User-Agent", "BoardFlow")
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send()
                .await
                .map_err(|e| format!("request failed: {e}"))?;

            if !resp.status().is_success() {
                return Err(format!("status {}", resp.status()));
            }

            let data: InstallationReposResponse = resp
                .json()
                .await
                .map_err(|e| format!("parse failed: {e}"))?;

            let count = data.repositories.len();
            all_repos.extend(data.repositories);

            if count < 100 {
                break;
            }
            page += 1;
        }

        Ok(all_repos)
    }
}
