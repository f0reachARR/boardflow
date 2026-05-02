use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// Generate a short-lived artifact access token.
/// Format: base64url({artifact_id}:{user_id}:{expires_unix}:{hmac_signature})
pub fn generate_artifact_token(artifact_id: Uuid, user_id: Uuid, secret: &[u8]) -> String {
    let expires = chrono::Utc::now().timestamp() + 3600; // 1 hour
    let payload = format!("{artifact_id}:{user_id}:{expires}");
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(payload.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    let token_raw = format!("{payload}:{sig}");
    URL_SAFE_NO_PAD.encode(token_raw.as_bytes())
}

/// Verify an artifact token and return (artifact_id, user_id) if valid.
pub fn verify_artifact_token(token: &str, secret: &[u8]) -> Option<(Uuid, Uuid)> {
    let bytes = URL_SAFE_NO_PAD.decode(token).ok()?;
    let raw = std::str::from_utf8(&bytes).ok()?;
    let parts: Vec<&str> = raw.splitn(4, ':').collect();
    if parts.len() != 4 {
        return None;
    }
    let artifact_id = Uuid::parse_str(parts[0]).ok()?;
    let user_id = Uuid::parse_str(parts[1]).ok()?;
    let expires: i64 = parts[2].parse().ok()?;
    let sig = parts[3];

    // Check expiry
    if chrono::Utc::now().timestamp() > expires {
        return None;
    }

    // Verify HMAC
    let payload = format!("{artifact_id}:{user_id}:{expires}");
    let mut mac = HmacSha256::new_from_slice(secret).ok()?;
    mac.update(payload.as_bytes());
    let expected_sig = hex::encode(mac.finalize().into_bytes());
    if sig != expected_sig {
        return None;
    }

    Some((artifact_id, user_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_verify_token() {
        let secret = b"test-secret-key-for-artifacts";
        let artifact_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();

        let token = generate_artifact_token(artifact_id, user_id, secret);
        let result = verify_artifact_token(&token, secret);

        assert_eq!(result, Some((artifact_id, user_id)));
    }

    #[test]
    fn test_invalid_secret_fails() {
        let secret = b"correct-secret";
        let wrong_secret = b"wrong-secret";
        let artifact_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();

        let token = generate_artifact_token(artifact_id, user_id, secret);
        let result = verify_artifact_token(&token, wrong_secret);

        assert_eq!(result, None);
    }

    #[test]
    fn test_tampered_token_fails() {
        let secret = b"test-secret";
        let artifact_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();

        let token = generate_artifact_token(artifact_id, user_id, secret);
        // Tamper with the token
        let mut tampered = token.clone();
        tampered.push('x');
        let result = verify_artifact_token(&tampered, secret);

        assert_eq!(result, None);
    }

    #[test]
    fn test_garbage_token_fails() {
        let secret = b"test-secret";
        assert_eq!(verify_artifact_token("not-a-valid-token", secret), None);
        assert_eq!(verify_artifact_token("", secret), None);
    }
}
