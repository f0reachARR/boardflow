#[derive(Debug, thiserror::Error)]
pub enum GitHubClientError {
    #[error("GitHub API authentication failed: {0}")]
    Auth(String),

    #[error("GitHub API rate limited (retry after {retry_after_secs:?}s)")]
    RateLimited {
        /// Seconds until rate limit resets (from Retry-After or x-ratelimit-reset header).
        /// None if the information is unavailable.
        retry_after_secs: Option<u64>,
    },

    #[error("GitHub resource not found: {0}")]
    NotFound(String),

    #[error("GitHub API validation failed: {0}")]
    Validation(String),

    #[error("GitHub API error: {0}")]
    Api(String),
}

fn map_status_to_error(status: u16, message: String) -> GitHubClientError {
    match status {
        401 => GitHubClientError::Auth(message),
        403 => {
            if message.to_lowercase().contains("rate limit") {
                GitHubClientError::RateLimited { retry_after_secs: None }
            } else {
                GitHubClientError::Auth(message)
            }
        }
        404 => GitHubClientError::NotFound(message),
        422 => GitHubClientError::Validation(message),
        429 => GitHubClientError::RateLimited { retry_after_secs: None },
        _ => GitHubClientError::Api(message),
    }
}

impl From<octocrab::Error> for GitHubClientError {
    fn from(err: octocrab::Error) -> Self {
        match &err {
            octocrab::Error::GitHub { source, .. } => {
                map_status_to_error(source.status_code.as_u16(), source.message.clone())
            }
            _ => GitHubClientError::Api(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_401_maps_to_auth() {
        let err = map_status_to_error(401, "Bad credentials".to_string());
        assert!(matches!(err, GitHubClientError::Auth(msg) if msg == "Bad credentials"));
    }

    #[test]
    fn test_403_rate_limit_maps_to_rate_limited() {
        let err = map_status_to_error(403, "API rate limit exceeded".to_string());
        assert!(matches!(err, GitHubClientError::RateLimited { retry_after_secs: None }));
    }

    #[test]
    fn test_403_non_rate_limit_maps_to_auth() {
        let err = map_status_to_error(403, "Forbidden".to_string());
        assert!(matches!(err, GitHubClientError::Auth(msg) if msg == "Forbidden"));
    }

    #[test]
    fn test_404_maps_to_not_found() {
        let err = map_status_to_error(404, "Not Found".to_string());
        assert!(matches!(err, GitHubClientError::NotFound(msg) if msg == "Not Found"));
    }

    #[test]
    fn test_422_maps_to_validation() {
        let err = map_status_to_error(422, "Validation Failed".to_string());
        assert!(matches!(err, GitHubClientError::Validation(msg) if msg == "Validation Failed"));
    }

    #[test]
    fn test_429_maps_to_rate_limited() {
        let err = map_status_to_error(429, "rate limited".to_string());
        assert!(matches!(err, GitHubClientError::RateLimited { retry_after_secs: None }));
    }

    #[test]
    fn test_500_maps_to_api() {
        let err = map_status_to_error(500, "Internal Server Error".to_string());
        assert!(matches!(err, GitHubClientError::Api(msg) if msg == "Internal Server Error"));
    }

    #[test]
    fn test_502_maps_to_api() {
        let err = map_status_to_error(502, "Bad Gateway".to_string());
        assert!(matches!(err, GitHubClientError::Api(msg) if msg == "Bad Gateway"));
    }

    #[test]
    fn test_403_secondary_rate_limit_maps_to_rate_limited() {
        let err = map_status_to_error(
            403,
            "You have exceeded a secondary rate limit".to_string(),
        );
        assert!(matches!(err, GitHubClientError::RateLimited { retry_after_secs: None }));
    }
}
