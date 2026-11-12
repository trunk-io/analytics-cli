//! What `--token` / `TRUNK_API_TOKEN` holds. It is read before any call, so a token the API would
//! refuse, such as a pre-1.0 organization token, never reaches it.

use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};

/// Every v2 API key starts with this; the gateway exchanges it for a Trunk JWT.
pub const API_KEY_PREFIX: &str = "trunk_v2_";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// A v2 API key, sent as `Authorization: Bearer`.
    ApiKey,
    /// A Trunk JWT, such as `trunk flakytests upload` passes, sent on `x-trunk-token`. Sent as a
    /// bearer it would reach the gateway's API key lookup and fail.
    Jwt,
    /// Anything else, which 1.0.0 reads as the organization token it no longer takes.
    Legacy,
}

impl TokenKind {
    pub fn of(token: &str) -> Self {
        let token = token.trim();
        if token.starts_with(API_KEY_PREFIX) {
            Self::ApiKey
        } else if is_jwt(token) {
            Self::Jwt
        } else {
            Self::Legacy
        }
    }
}

/// Three non-empty base64url segments: a JWS in compact form. An organization token has no dots.
fn is_jwt(token: &str) -> bool {
    let segments: Vec<&str> = token.split('.').collect();
    segments.len() == 3
        && segments.iter().all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        })
}

/// The JWT's `exp`, read without checking its signature, for the message when Trunk refuses it.
pub fn jwt_expiry(token: &str) -> Option<DateTime<Utc>> {
    let payload = URL_SAFE_NO_PAD
        .decode(token.trim().split('.').nth(1)?)
        .ok()?;
    let exp = serde_json::from_slice::<serde_json::Value>(&payload)
        .ok()?
        .get("exp")?
        .as_i64()?;
    DateTime::from_timestamp(exp, 0)
}

/// The run's token is neither a v2 API key nor a Trunk JWT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegacyTokenRefused;

impl fmt::Display for LegacyTokenRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(LEGACY_TOKEN_MESSAGE)
    }
}

impl std::error::Error for LegacyTokenRefused {}

pub const LEGACY_TOKEN_MESSAGE: &str = concat!(
    "This Trunk token is not a v2 API key. analytics-cli 1.0.0 needs a v2 API key: create one in ",
    "the Trunk web app under Settings → Developer → API Keys, give it the Upload test results ",
    "permission, and pass it with --token or TRUNK_API_TOKEN. To keep using your organization ",
    "token, pin analytics-cli 0.17.x.",
);

#[cfg(test)]
mod tests {
    use super::{TokenKind, jwt_expiry};

    // {"alg":"RS256","typ":"JWT"}.{"sub":"org-token:1","exp":1762880400}.signature
    const JWT: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJvcmctdG9rZW46MSIsImV4cCI6MTc2Mjg4MDQwMH0.c2lnbmF0dXJl";

    #[test]
    fn reads_a_v2_api_key_by_its_prefix() {
        for token in [
            "trunk_v2_AbC123",
            "  trunk_v2_AbC123\n",
            // The prefix decides, whatever follows it.
            "trunk_v2_a.b.c",
        ] {
            assert_eq!(TokenKind::of(token), TokenKind::ApiKey, "{token:?}");
        }
    }

    #[test]
    fn reads_three_base64url_segments_as_a_jwt() {
        for token in [JWT, "a.b.c", "A-_9.x.y_-", &format!(" {JWT} ")] {
            assert_eq!(TokenKind::of(token), TokenKind::Jwt, "{token:?}");
        }
    }

    #[test]
    fn reads_anything_else_as_a_legacy_token() {
        for token in [
            "",
            "   ",
            "0123456789abcdef0123456789abcdef",
            "8f14e45f-ceea-467a-9575-2b8a3e2c0a11",
            "trunk_v1_AbC123",
            "TRUNK_V2_AbC123",
            // Dots, but not three segments.
            "a.b",
            "a.b.c.d",
            "token.with.dots.in.it",
            // Three segments, but one is empty or not base64url.
            "a..c",
            ".b.c",
            "a.b.",
            "a+b.c/d.e=",
            "a b.c.d",
        ] {
            assert_eq!(TokenKind::of(token), TokenKind::Legacy, "{token:?}");
        }
    }

    #[test]
    fn reads_a_jwt_expiry_without_its_signature() {
        assert_eq!(
            jwt_expiry(JWT).map(|expiry| expiry.timestamp()),
            Some(1_762_880_400)
        );
        for token in ["a.b.c", "trunk_v2_AbC123", ""] {
            assert_eq!(jwt_expiry(token), None, "{token:?}");
        }
    }
}
