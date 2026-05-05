/// Authentication credential for Trunk API requests.
///
/// Two flavors:
/// * `Token` — an org API token. Sent on every endpoint via the `x-api-token`
///   header. Used by regular CI on the upstream repo where repo secrets are
///   available.
/// * `PublicRepoId` — a non-secret per-repo identifier (the 8-character value
///   from the Trunk settings UI). Sent via the `X-Trunk-Public-Repo-Id` header
///   on the two endpoints that accept it (`createBundleUpload` and
///   `getQuarantineConfig`). Used on fork-PR runs where secrets are
///   unavailable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrunkApiCredential {
    Token(String),
    PublicRepoId(String),
}

pub const PUBLIC_REPO_ID_HEADER: &str = "x-trunk-public-repo-id";
pub const API_TOKEN_HEADER: &str = "x-api-token";

impl TrunkApiCredential {
    /// Resolve a credential from a token and a public-repo-id, with token-first
    /// ordering. Empty / whitespace-only strings count as absent.
    ///
    /// Token-first matters: it preserves existing behaviour for non-fork CI
    /// runs that have an org token configured, and only falls back to the
    /// public-id when the token is genuinely absent.
    pub fn resolve(token: Option<&str>, public_repo_id: Option<&str>) -> Option<Self> {
        let cleaned_token = token.map(str::trim).filter(|s| !s.is_empty());
        if let Some(token) = cleaned_token {
            return Some(Self::Token(token.to_string()));
        }
        let cleaned_public_id = public_repo_id.map(str::trim).filter(|s| !s.is_empty());
        if let Some(public_id) = cleaned_public_id {
            return Some(Self::PublicRepoId(public_id.to_string()));
        }
        None
    }

    pub fn header_name(&self) -> &'static str {
        match self {
            Self::Token(_) => API_TOKEN_HEADER,
            Self::PublicRepoId(_) => PUBLIC_REPO_ID_HEADER,
        }
    }

    pub fn header_value(&self) -> &str {
        match self {
            Self::Token(value) | Self::PublicRepoId(value) => value,
        }
    }

    pub fn is_token(&self) -> bool {
        matches!(self, Self::Token(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_only_returns_token() {
        let cred = TrunkApiCredential::resolve(Some("abc"), None).unwrap();
        assert_eq!(cred, TrunkApiCredential::Token("abc".to_string()));
    }

    #[test]
    fn public_id_only_returns_public_id() {
        let cred = TrunkApiCredential::resolve(None, Some("abcd1234")).unwrap();
        assert_eq!(
            cred,
            TrunkApiCredential::PublicRepoId("abcd1234".to_string())
        );
    }

    #[test]
    fn token_wins_when_both_present() {
        let cred = TrunkApiCredential::resolve(Some("the-token"), Some("abcd1234")).unwrap();
        assert_eq!(cred, TrunkApiCredential::Token("the-token".to_string()));
    }

    #[test]
    fn empty_token_falls_through_to_public_id() {
        let cred = TrunkApiCredential::resolve(Some(""), Some("abcd1234")).unwrap();
        assert_eq!(
            cred,
            TrunkApiCredential::PublicRepoId("abcd1234".to_string())
        );
    }

    #[test]
    fn whitespace_token_falls_through_to_public_id() {
        let cred = TrunkApiCredential::resolve(Some("   "), Some("abcd1234")).unwrap();
        assert_eq!(
            cred,
            TrunkApiCredential::PublicRepoId("abcd1234".to_string())
        );
    }

    #[test]
    fn neither_returns_none() {
        assert!(TrunkApiCredential::resolve(None, None).is_none());
        assert!(TrunkApiCredential::resolve(Some(""), Some("")).is_none());
        assert!(TrunkApiCredential::resolve(Some("  "), Some("  ")).is_none());
    }

    #[test]
    fn header_name_matches_credential_kind() {
        assert_eq!(
            TrunkApiCredential::Token("x".into()).header_name(),
            "x-api-token"
        );
        assert_eq!(
            TrunkApiCredential::PublicRepoId("x".into()).header_name(),
            "x-trunk-public-repo-id"
        );
    }
}
