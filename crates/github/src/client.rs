use secrecy::{ExposeSecret, SecretString};

use crate::config::GitHubAppConfig;
use crate::error::GitHubClientError;
use crate::types::{CreatedComment, CreatedIssue, InstallationRepoInfo, IssueInfo, IssueState};

/// Trait for GitHub App client operations.
/// Production implementation uses octocrab; tests can mock this trait.
#[async_trait::async_trait]
pub trait GitHubAppClient: Send + Sync {
    /// Obtain an installation access token for the given installation.
    async fn get_installation_token(
        &self,
        installation_id: u64,
    ) -> Result<SecretString, GitHubClientError>;

    /// Create a new issue in the specified repository.
    async fn create_issue(
        &self,
        installation_id: u64,
        owner: &str,
        repo: &str,
        title: &str,
        body: &str,
    ) -> Result<CreatedIssue, GitHubClientError>;

    /// Get information about an existing issue.
    async fn get_issue(
        &self,
        installation_id: u64,
        owner: &str,
        repo: &str,
        issue_number: u64,
    ) -> Result<IssueInfo, GitHubClientError>;

    /// Create a comment on an issue.
    async fn create_comment(
        &self,
        installation_id: u64,
        owner: &str,
        repo: &str,
        issue_number: u64,
        body: &str,
    ) -> Result<CreatedComment, GitHubClientError>;

    /// Update an existing comment.
    async fn update_comment(
        &self,
        installation_id: u64,
        owner: &str,
        repo: &str,
        comment_id: u64,
        body: &str,
    ) -> Result<(), GitHubClientError>;

    /// List installations for the authenticated GitHub App.
    /// Default returns an empty list; the worker reconciler tolerates that as "nothing to sync".
    async fn list_installation_ids(&self) -> Result<Vec<u64>, GitHubClientError> {
        Ok(Vec::new())
    }

    /// List repositories accessible to the given installation.
    /// Default returns an empty list.
    async fn list_installation_repositories(
        &self,
        _installation_id: u64,
    ) -> Result<Vec<InstallationRepoInfo>, GitHubClientError> {
        Ok(Vec::new())
    }
}

/// Production implementation backed by octocrab.
pub struct OctocrabGitHubAppClient {
    octocrab: octocrab::Octocrab,
}

impl OctocrabGitHubAppClient {
    /// Create a new client from a GitHub App configuration.
    ///
    /// This initializes an octocrab instance authenticated as the GitHub App
    /// using the RS256 private key for JWT signing.
    pub fn new(config: &GitHubAppConfig) -> Result<Self, GitHubClientError> {
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(
            config.private_key_pem.expose_secret().as_bytes(),
        )
        .map_err(|e| GitHubClientError::Auth(format!("invalid RSA private key: {e}")))?;

        let octocrab = octocrab::Octocrab::builder()
            .app(octocrab::models::AppId(config.app_id), key)
            .build()
            .map_err(|e| {
                GitHubClientError::Auth(format!("failed to build octocrab client: {e}"))
            })?;

        Ok(Self { octocrab })
    }
}

#[async_trait::async_trait]
impl GitHubAppClient for OctocrabGitHubAppClient {
    async fn get_installation_token(
        &self,
        installation_id: u64,
    ) -> Result<SecretString, GitHubClientError> {
        let (_crab, token) = self
            .octocrab
            .installation_and_token(octocrab::models::InstallationId(installation_id))
            .await
            .map_err(GitHubClientError::from)?;

        Ok(token)
    }

    async fn create_issue(
        &self,
        installation_id: u64,
        owner: &str,
        repo: &str,
        title: &str,
        body: &str,
    ) -> Result<CreatedIssue, GitHubClientError> {
        let installation_crab = self
            .octocrab
            .installation(octocrab::models::InstallationId(installation_id))
            .map_err(GitHubClientError::from)?;

        let issue = installation_crab
            .issues(owner, repo)
            .create(title)
            .body(body)
            .send()
            .await
            .map_err(GitHubClientError::from)?;

        Ok(CreatedIssue {
            number: issue.number,
            node_id: issue.node_id,
            html_url: issue.html_url.to_string(),
        })
    }

    async fn get_issue(
        &self,
        installation_id: u64,
        owner: &str,
        repo: &str,
        issue_number: u64,
    ) -> Result<IssueInfo, GitHubClientError> {
        let installation_crab = self
            .octocrab
            .installation(octocrab::models::InstallationId(installation_id))
            .map_err(GitHubClientError::from)?;

        let issue = installation_crab
            .issues(owner, repo)
            .get(issue_number)
            .await
            .map_err(GitHubClientError::from)?;

        let state = match issue.state {
            octocrab::models::IssueState::Open => IssueState::Open,
            _ => IssueState::Closed,
        };

        Ok(IssueInfo {
            number: issue.number,
            node_id: issue.node_id,
            state,
            html_url: issue.html_url.to_string(),
        })
    }

    async fn create_comment(
        &self,
        installation_id: u64,
        owner: &str,
        repo: &str,
        issue_number: u64,
        body: &str,
    ) -> Result<CreatedComment, GitHubClientError> {
        let installation_crab = self
            .octocrab
            .installation(octocrab::models::InstallationId(installation_id))
            .map_err(GitHubClientError::from)?;

        let comment = installation_crab
            .issues(owner, repo)
            .create_comment(issue_number, body)
            .await
            .map_err(GitHubClientError::from)?;

        Ok(CreatedComment {
            id: comment.id.into_inner(),
        })
    }

    async fn update_comment(
        &self,
        installation_id: u64,
        owner: &str,
        repo: &str,
        comment_id: u64,
        body: &str,
    ) -> Result<(), GitHubClientError> {
        let installation_crab = self
            .octocrab
            .installation(octocrab::models::InstallationId(installation_id))
            .map_err(GitHubClientError::from)?;

        installation_crab
            .issues(owner, repo)
            .update_comment(octocrab::models::CommentId(comment_id), body)
            .await
            .map_err(GitHubClientError::from)?;

        Ok(())
    }

    async fn list_installation_ids(&self) -> Result<Vec<u64>, GitHubClientError> {
        let mut ids = Vec::new();
        let mut page = 1u32;
        loop {
            let result = self
                .octocrab
                .apps()
                .installations()
                .per_page(100u8)
                .page(page)
                .send()
                .await
                .map_err(GitHubClientError::from)?;

            let items = result.items;
            if items.is_empty() {
                break;
            }
            for inst in &items {
                ids.push(inst.id.0);
            }
            if items.len() < 100 {
                break;
            }
            page += 1;
        }
        Ok(ids)
    }

    async fn list_installation_repositories(
        &self,
        installation_id: u64,
    ) -> Result<Vec<InstallationRepoInfo>, GitHubClientError> {
        let installation_crab = self
            .octocrab
            .installation(octocrab::models::InstallationId(installation_id))
            .map_err(GitHubClientError::from)?;

        let mut repos = Vec::new();
        let mut page = 1u32;
        loop {
            let route = format!("/installation/repositories?per_page=100&page={page}");
            let resp: octocrab::models::InstallationRepositories = installation_crab
                .get(&route, None::<&()>)
                .await
                .map_err(GitHubClientError::from)?;

            let count = resp.repositories.len();
            for r in &resp.repositories {
                let owner = match r.owner.as_ref().map(|a| a.login.clone()) {
                    Some(o) => o,
                    None => match r.full_name.as_ref().and_then(|fn_| fn_.split_once('/')) {
                        Some((o, _)) => o.to_string(),
                        None => continue,
                    },
                };
                repos.push(InstallationRepoInfo {
                    id: r.id.0 as i64,
                    owner,
                    name: r.name.clone(),
                });
            }
            if count < 100 {
                break;
            }
            page += 1;
        }
        Ok(repos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GitHubAppConfig;

    /// Verify that `GitHubAppClient` is object-safe and can be used as `Arc<dyn GitHubAppClient>`.
    fn _assert_object_safe(_: &dyn GitHubAppClient) {}

    #[test]
    fn trait_is_object_safe() {
        // Compilation of _assert_object_safe is sufficient proof.
        // This test exists to prevent accidental breakage.
    }

    #[test]
    fn new_with_invalid_pem_returns_auth_error() {
        let config = GitHubAppConfig {
            app_id: 12345,
            private_key_pem: secrecy::SecretString::from("not-a-valid-pem"),
        };

        let result = OctocrabGitHubAppClient::new(&config);
        match result {
            Err(GitHubClientError::Auth(msg)) => {
                assert!(msg.contains("invalid RSA private key"));
            }
            Err(other) => panic!("expected Auth error, got: {other}"),
            Ok(_) => panic!("expected error, got Ok"),
        }
    }
}
