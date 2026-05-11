//! Playwright trace discovery + packing helpers.
//!
//! The CLI extracts `[[ATTACHMENT|<path>]]` references from JUnit `<system-out>`
//! payloads (the syntax Playwright's reporter writes), maps each `.zip` attachment
//! to the test case that produced it, and packs the archive into the bundle
//! tarball at `traces/<identity_hash>.zip`. The server-side ingester
//! (`bundle-ingestion-lib`) recomputes the same identity hash to match the
//! archive back to a `test_case_id` row.
//!
//! `<identity_hash>` is the SHA-256 hex digest of the joined identity tuple
//! `file|className|parentName|name|variant`. This format is fixed by the
//! CLI/ingester contract — changing it breaks trace matching for any bundle
//! produced by an older CLI.

use std::path::PathBuf;

use sha2::{Digest, Sha256};

/// Tar prefix every Playwright trace archive lives under inside the bundle.
pub const TRACES_PREFIX: &str = "traces/";
/// Suffix every Playwright trace archive carries inside the bundle.
pub const TRACE_ARCHIVE_SUFFIX: &str = ".zip";

/// One Playwright trace file, ready to be packed into the bundle tarball.
#[derive(Debug, Clone)]
pub struct DiscoveredTrace {
    /// SHA-256 hex of the identity tuple. Used as the in-tarball filename
    /// stem and as the matching key on the ingester side.
    pub identity_hash: String,
    /// Absolute path on disk where the source `trace.zip` lives.
    pub source_path: PathBuf,
}

/// Returns the in-tarball filename for a trace with the given identity hash.
pub fn trace_archive_name(identity_hash: &str) -> String {
    format!("{TRACES_PREFIX}{identity_hash}{TRACE_ARCHIVE_SUFFIX}")
}

/// Computes the canonical identity hash for a test case. Both the CLI and
/// the bundle ingester compute this exactly the same way; the ingester won't
/// match a trace whose filename stem isn't byte-identical to this output.
pub fn compute_trace_identity_hash(
    file: Option<&str>,
    classname: Option<&str>,
    parent_name: &str,
    name: &str,
    variant: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(file.unwrap_or("").as_bytes());
    hasher.update(b"|");
    hasher.update(classname.unwrap_or("").as_bytes());
    hasher.update(b"|");
    hasher.update(parent_name.as_bytes());
    hasher.update(b"|");
    hasher.update(name.as_bytes());
    hasher.update(b"|");
    hasher.update(variant.as_bytes());
    hex::encode(hasher.finalize())
}

/// Extracts attachment paths emitted by Playwright's JUnit reporter into a
/// `<system-out>` block. Playwright wraps each attachment as
/// `[[ATTACHMENT|<absolute or relative path>]]` on its own line; lines that
/// don't match are ignored. Returns paths in document order, deduped.
pub fn extract_attachment_paths(system_out: &str) -> Vec<&str> {
    const PREFIX: &str = "[[ATTACHMENT|";
    const SUFFIX: &str = "]]";

    let mut out: Vec<&str> = Vec::new();
    for line in system_out.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix(PREFIX) else {
            continue;
        };
        let Some(path) = rest.strip_suffix(SUFFIX) else {
            continue;
        };
        let path = path.trim();
        if path.is_empty() || out.contains(&path) {
            continue;
        }
        out.push(path);
    }
    out
}

/// Returns true when `path` looks like a Playwright trace archive (i.e. ends
/// in `.zip`, case-insensitive). Other attachments — screenshots, videos,
/// plain `.txt` traces — are ignored in v1.
pub fn is_trace_archive_path(path: &str) -> bool {
    path.len() >= TRACE_ARCHIVE_SUFFIX.len()
        && path[path.len() - TRACE_ARCHIVE_SUFFIX.len()..]
            .eq_ignore_ascii_case(TRACE_ARCHIVE_SUFFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_hash_is_64_hex_chars() {
        let hash = compute_trace_identity_hash(
            Some("tests/login.spec.ts"),
            Some("LoginPage"),
            "auth flow",
            "logs in successfully",
            "",
        );
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn identity_hash_is_deterministic() {
        let inputs = (Some("a.ts"), Some("Login"), "suite", "test", "chromium");
        let a = compute_trace_identity_hash(inputs.0, inputs.1, inputs.2, inputs.3, inputs.4);
        let b = compute_trace_identity_hash(inputs.0, inputs.1, inputs.2, inputs.3, inputs.4);
        assert_eq!(a, b);
    }

    #[test]
    fn identity_hash_distinguishes_variant() {
        let a = compute_trace_identity_hash(Some("a.ts"), None, "suite", "case", "");
        let b = compute_trace_identity_hash(Some("a.ts"), None, "suite", "case", "chromium");
        assert_ne!(a, b);
    }

    #[test]
    fn identity_hash_treats_missing_optional_fields_as_empty_strings() {
        let a = compute_trace_identity_hash(None, None, "suite", "case", "");
        let b = compute_trace_identity_hash(Some(""), Some(""), "suite", "case", "");
        assert_eq!(a, b);
    }

    #[test]
    fn extract_attachment_paths_handles_basic_block() {
        let system_out = "\
            Some preamble line\n\
            [[ATTACHMENT|test-results/foo/trace.zip]]\n\
            Other diagnostic output\n\
            [[ATTACHMENT|test-results/foo/screenshot.png]]\n\
        ";
        let paths = extract_attachment_paths(system_out);
        assert_eq!(
            paths,
            vec![
                "test-results/foo/trace.zip",
                "test-results/foo/screenshot.png",
            ]
        );
    }

    #[test]
    fn extract_attachment_paths_dedupes() {
        let system_out =
            "[[ATTACHMENT|trace.zip]]\n[[ATTACHMENT|other.png]]\n[[ATTACHMENT|trace.zip]]\n";
        assert_eq!(
            extract_attachment_paths(system_out),
            vec!["trace.zip", "other.png"]
        );
    }

    #[test]
    fn extract_attachment_paths_ignores_garbage() {
        assert!(extract_attachment_paths("").is_empty());
        assert!(extract_attachment_paths("plain text only").is_empty());
        assert!(extract_attachment_paths("[[ATTACHMENT|]]").is_empty());
    }

    #[test]
    fn is_trace_archive_path_matches_zip() {
        assert!(is_trace_archive_path("trace.zip"));
        assert!(is_trace_archive_path("a/b/trace.ZIP"));
        assert!(!is_trace_archive_path("screenshot.png"));
        assert!(!is_trace_archive_path("zip"));
    }

    #[test]
    fn trace_archive_name_matches_ingester_layout() {
        assert_eq!(trace_archive_name("abc123"), "traces/abc123.zip");
    }
}
