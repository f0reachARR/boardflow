use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::error::AppError;

// ─── Cursor payloads (private) ───────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct CursorPayload {
    ts: String,
    id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct RepositoryCursorPayload {
    ts: String,
    gid: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct FindingsCursorPayload {
    si: i32,
    id: String,
}

// ─── Cursor encoding/decoding ────────────────────────────────────────────────

pub(crate) fn encode_cursor(ts: &DateTime<Utc>, id: &Uuid) -> String {
    let payload = CursorPayload {
        ts: ts.to_rfc3339(),
        id: id.to_string(),
    };
    let json = serde_json::to_string(&payload).unwrap();
    URL_SAFE_NO_PAD.encode(json.as_bytes())
}

pub(crate) fn decode_cursor(cursor: &str) -> Option<(DateTime<Utc>, Uuid)> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let payload: CursorPayload = serde_json::from_slice(&bytes).ok()?;
    let ts = DateTime::parse_from_rfc3339(&payload.ts).ok()?.to_utc();
    let id = Uuid::parse_str(&payload.id).ok()?;
    Some((ts, id))
}

pub(crate) fn encode_repository_cursor(ts: &DateTime<Utc>, github_repository_id: i64) -> String {
    let payload = RepositoryCursorPayload {
        ts: ts.to_rfc3339(),
        gid: github_repository_id.to_string(),
    };
    let json = serde_json::to_string(&payload).unwrap();
    URL_SAFE_NO_PAD.encode(json.as_bytes())
}

pub(crate) fn decode_repository_cursor(cursor: &str) -> Option<(DateTime<Utc>, i64)> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let payload: RepositoryCursorPayload = serde_json::from_slice(&bytes).ok()?;
    let ts = DateTime::parse_from_rfc3339(&payload.ts).ok()?.to_utc();
    let gid: i64 = payload.gid.parse().ok()?;
    Some((ts, gid))
}

pub(crate) fn encode_findings_cursor(sort_index: i32, id: &Uuid) -> String {
    let payload = FindingsCursorPayload {
        si: sort_index,
        id: id.to_string(),
    };
    let json = serde_json::to_string(&payload).unwrap();
    URL_SAFE_NO_PAD.encode(json.as_bytes())
}

pub(crate) fn decode_findings_cursor(cursor: &str) -> Option<(i32, Uuid)> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let payload: FindingsCursorPayload = serde_json::from_slice(&bytes).ok()?;
    let id = Uuid::parse_str(&payload.id).ok()?;
    Some((payload.si, id))
}

// ─── Query parameters ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, IntoParams)]
pub struct PaginationParams {
    #[param(default = 50, minimum = 1, maximum = 100)]
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

impl PaginationParams {
    pub(crate) fn effective_limit(&self) -> i64 {
        self.limit.unwrap_or(50).clamp(1, 100)
    }

    pub(crate) fn decoded_cursor(
        &self,
        request_id: &str,
    ) -> Result<Option<(DateTime<Utc>, Uuid)>, AppError> {
        match &self.cursor {
            None => Ok(None),
            Some(c) => decode_cursor(c)
                .map(Some)
                .ok_or_else(|| AppError::validation_failed("invalid cursor", request_id)),
        }
    }

    pub(crate) fn decoded_repository_cursor(
        &self,
        request_id: &str,
    ) -> Result<Option<(DateTime<Utc>, i64)>, AppError> {
        match &self.cursor {
            None => Ok(None),
            Some(c) => decode_repository_cursor(c)
                .map(Some)
                .ok_or_else(|| AppError::validation_failed("invalid cursor", request_id)),
        }
    }
}

// ─── Response types ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, ToSchema)]
pub struct PaginatedResponse<T: Serialize> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    // ── encode_cursor / decode_cursor: 正常系 ─────────────────────────────

    #[test]
    fn test_encode_decode_cursor_roundtrip() {
        let ts = Utc::now();
        let id = Uuid::now_v7();
        let encoded = encode_cursor(&ts, &id);
        let (decoded_ts, decoded_id) = decode_cursor(&encoded).unwrap();
        assert_eq!(decoded_ts, ts);
        assert_eq!(decoded_id, id);
    }

    // ── decode_cursor: 異常系 ─────────────────────────────────────────────

    #[test]
    fn test_decode_cursor_invalid_base64() {
        assert!(decode_cursor("!!!invalid!!!").is_none());
    }

    #[test]
    fn test_decode_cursor_invalid_json() {
        let encoded = URL_SAFE_NO_PAD.encode(b"not json");
        assert!(decode_cursor(&encoded).is_none());
    }

    #[test]
    fn test_decode_cursor_invalid_uuid() {
        let payload = CursorPayload {
            ts: Utc::now().to_rfc3339(),
            id: "not-a-uuid".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let encoded = URL_SAFE_NO_PAD.encode(json.as_bytes());
        assert!(decode_cursor(&encoded).is_none());
    }

    #[test]
    fn test_decode_cursor_invalid_timestamp() {
        let payload = CursorPayload {
            ts: "not-a-timestamp".to_string(),
            id: Uuid::now_v7().to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let encoded = URL_SAFE_NO_PAD.encode(json.as_bytes());
        assert!(decode_cursor(&encoded).is_none());
    }

    #[test]
    fn test_decode_cursor_empty_string() {
        assert!(decode_cursor("").is_none());
    }

    // ── encode_repository_cursor / decode_repository_cursor: 正常系 ──────

    #[test]
    fn test_encode_decode_repository_cursor_roundtrip() {
        let ts = Utc::now();
        let gid: i64 = 123456789;
        let encoded = encode_repository_cursor(&ts, gid);
        let (decoded_ts, decoded_gid) = decode_repository_cursor(&encoded).unwrap();
        assert_eq!(decoded_ts, ts);
        assert_eq!(decoded_gid, gid);
    }

    // ── decode_repository_cursor: 異常系 ─────────────────────────────────

    #[test]
    fn test_decode_repository_cursor_invalid_base64() {
        assert!(decode_repository_cursor("!!!invalid!!!").is_none());
    }

    #[test]
    fn test_decode_repository_cursor_invalid_gid() {
        let payload = RepositoryCursorPayload {
            ts: Utc::now().to_rfc3339(),
            gid: "not-a-number".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let encoded = URL_SAFE_NO_PAD.encode(json.as_bytes());
        assert!(decode_repository_cursor(&encoded).is_none());
    }

    // ── encode_findings_cursor / decode_findings_cursor: 正常系 ──────────

    #[test]
    fn test_encode_decode_findings_cursor_roundtrip() {
        let si: i32 = 42;
        let id = Uuid::now_v7();
        let encoded = encode_findings_cursor(si, &id);
        let (decoded_si, decoded_id) = decode_findings_cursor(&encoded).unwrap();
        assert_eq!(decoded_si, si);
        assert_eq!(decoded_id, id);
    }

    // ── decode_findings_cursor: 異常系 ───────────────────────────────────

    #[test]
    fn test_decode_findings_cursor_invalid_base64() {
        assert!(decode_findings_cursor("!!!invalid!!!").is_none());
    }

    #[test]
    fn test_decode_findings_cursor_invalid_uuid() {
        let payload = FindingsCursorPayload {
            si: 1,
            id: "not-a-uuid".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let encoded = URL_SAFE_NO_PAD.encode(json.as_bytes());
        assert!(decode_findings_cursor(&encoded).is_none());
    }

    // ── PaginationParams: effective_limit ────────────────────────────────

    #[test]
    fn test_effective_limit_default() {
        let params = PaginationParams {
            limit: None,
            cursor: None,
        };
        assert_eq!(params.effective_limit(), 50);
    }

    #[test]
    fn test_effective_limit_clamped_min() {
        let params = PaginationParams {
            limit: Some(0),
            cursor: None,
        };
        assert_eq!(params.effective_limit(), 1);
    }

    #[test]
    fn test_effective_limit_clamped_max() {
        let params = PaginationParams {
            limit: Some(200),
            cursor: None,
        };
        assert_eq!(params.effective_limit(), 100);
    }

    #[test]
    fn test_effective_limit_normal() {
        let params = PaginationParams {
            limit: Some(25),
            cursor: None,
        };
        assert_eq!(params.effective_limit(), 25);
    }

    // ── PaginationParams: decoded_cursor ─────────────────────────────────

    #[test]
    fn test_decoded_cursor_none() {
        let params = PaginationParams {
            limit: None,
            cursor: None,
        };
        assert!(params.decoded_cursor("req-1").unwrap().is_none());
    }

    #[test]
    fn test_decoded_cursor_valid() {
        let ts = Utc::now();
        let id = Uuid::now_v7();
        let encoded = encode_cursor(&ts, &id);
        let params = PaginationParams {
            limit: None,
            cursor: Some(encoded),
        };
        let (decoded_ts, decoded_id) = params.decoded_cursor("req-1").unwrap().unwrap();
        assert_eq!(decoded_ts, ts);
        assert_eq!(decoded_id, id);
    }

    #[test]
    fn test_decoded_cursor_invalid_returns_error() {
        let params = PaginationParams {
            limit: None,
            cursor: Some("bad-cursor".to_string()),
        };
        let err = params.decoded_cursor("req-1").unwrap_err();
        assert_eq!(err.message, "invalid cursor");
    }

    // ── PaginationParams: decoded_repository_cursor ──────────────────────

    #[test]
    fn test_decoded_repository_cursor_none() {
        let params = PaginationParams {
            limit: None,
            cursor: None,
        };
        assert!(params.decoded_repository_cursor("req-1").unwrap().is_none());
    }

    #[test]
    fn test_decoded_repository_cursor_valid() {
        let ts = Utc::now();
        let gid: i64 = 999;
        let encoded = encode_repository_cursor(&ts, gid);
        let params = PaginationParams {
            limit: None,
            cursor: Some(encoded),
        };
        let (decoded_ts, decoded_gid) = params.decoded_repository_cursor("req-1").unwrap().unwrap();
        assert_eq!(decoded_ts, ts);
        assert_eq!(decoded_gid, gid);
    }

    #[test]
    fn test_decoded_repository_cursor_invalid_returns_error() {
        let params = PaginationParams {
            limit: None,
            cursor: Some("bad-cursor".to_string()),
        };
        let err = params.decoded_repository_cursor("req-1").unwrap_err();
        assert_eq!(err.message, "invalid cursor");
    }

    // ── cursor 互換性: 異なるカーソル型を誤って渡した場合 ─────────────────

    #[test]
    fn test_uuid_cursor_rejected_by_repository_decoder() {
        let ts = Utc::now();
        let id = Uuid::now_v7();
        let encoded = encode_cursor(&ts, &id);
        // UUID cursor を repository decoder に渡すとフィールド名不一致で None
        assert!(decode_repository_cursor(&encoded).is_none());
    }

    #[test]
    fn test_repository_cursor_rejected_by_uuid_decoder() {
        let ts = Utc::now();
        let gid: i64 = 42;
        let encoded = encode_repository_cursor(&ts, gid);
        // repository cursor を UUID decoder に渡すとフィールド名不一致で None
        assert!(decode_cursor(&encoded).is_none());
    }
}
