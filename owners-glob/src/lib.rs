#![feature(portable_simd)]
//! CODEOWNERS-style glob pattern matching.
//!
//! Compiles each pattern to a `Regex` at construction time so repeated
//! matching is fast.  Supports the gitignore syntax used by GitHub and
//! GitLab CODEOWNERS files:
//!
//! | Syntax      | Meaning                                              |
//! |-------------|------------------------------------------------------|
//! | `*`         | Any sequence of characters except `/`               |
//! | `**`        | Any sequence of characters including `/`            |
//! | `?`         | Any single character except `/`                     |
//! | `[abc]`     | Character class                                     |
//! | Leading `/` | Anchored to the repository root                     |
//! | Trailing `/`| Matches the directory and all its contents          |
//! | No `/`      | Matches the name anywhere in the tree (unanchored)  |
//!
//! Plain-name patterns (e.g. `foo/bar`, `**/widget`) that end with no
//! extension and no wildcard also implicitly match everything nested inside
//! that entry, mirroring the "implied children" behaviour of CODEOWNERS.

use std::collections::HashMap;

use regex::{Regex, RegexBuilder, RegexSet, RegexSetBuilder};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid glob pattern `{pattern}`: {source}")]
    InvalidRegex {
        pattern: String,
        #[source]
        source: regex::Error,
    },
    #[error("failed to build Aho-Corasick automaton: {0}")]
    AhoCorasickBuild(String),
}

/// A compiled CODEOWNERS glob pattern.
#[derive(Debug, Clone)]
pub struct Pattern {
    original: String,
    regex: Regex,
    /// When true, paths are lowercased before matching (case-insensitive mode).
    /// The pattern's literal parts are lowercased at compile time so the regex
    /// itself stays case-sensitive, keeping both compile and match cost low.
    fold_case: bool,
}

impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        self.original == other.original
    }
}
impl Eq for Pattern {}

impl std::fmt::Display for Pattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.original)
    }
}

/// Options controlling how a pattern is compiled and matched.
#[derive(Debug, Clone, Copy)]
pub struct PatternOptions {
    /// Whether matching is case-sensitive (default: `false` — GitHub style).
    pub case_sensitive: bool,
    /// Whether a plain-name pattern (e.g. `foo/bar`) implicitly also matches
    /// everything nested inside that entry (default: `true` — GitHub style).
    ///
    /// Set to `false` for GitLab, which requires an explicit trailing `/` or
    /// `/**` to match directory contents.
    pub implied_children: bool,
}

impl Default for PatternOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            implied_children: true,
        }
    }
}

impl PatternOptions {
    /// Options matching GitLab CODEOWNERS semantics.
    pub fn gitlab() -> Self {
        Self {
            case_sensitive: true,
            implied_children: false,
        }
    }
}

/// A per-file compile cache for CODEOWNERS patterns.
///
/// Create one per CODEOWNERS file and call [`PatternCache::compile`] instead
/// of [`Pattern::with_options`] directly.  Repeated calls with the same
/// `(raw, opts)` pair skip regex compilation and return a clone of the
/// previously built [`Pattern`].
///
/// [`Pattern`] (and thus [`Regex`]) is arc-backed, so cloning is O(1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PatternCache {
    cache: HashMap<(String, bool, bool), Pattern>,
}

impl PatternCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Look up a previously compiled pattern by its raw string and options.
    /// Returns `None` if the pattern has not been compiled yet.
    pub fn get(&self, raw: &str, opts: PatternOptions) -> Option<&Pattern> {
        self.cache
            .get(&(raw.to_owned(), opts.case_sensitive, opts.implied_children))
    }

    /// Return a compiled [`Pattern`] for `(raw, opts)`, compiling it once and
    /// caching the result for all subsequent calls with the same pair.
    pub fn compile(&mut self, raw: &str, opts: PatternOptions) -> Result<Pattern, Error> {
        let key = (raw.to_owned(), opts.case_sensitive, opts.implied_children);
        if let Some(p) = self.cache.get(&key) {
            return Ok(p.clone());
        }
        let pattern = Pattern::with_options(raw, opts)?;
        self.cache.insert(key, pattern.clone());
        Ok(pattern)
    }
}

impl Pattern {
    /// Compile a raw CODEOWNERS pattern with default (GitHub) options:
    /// case-insensitive with implied-children matching.
    pub fn new(raw: &str) -> Result<Self, Error> {
        Self::with_options(raw, PatternOptions::default())
    }

    /// Compile a raw CODEOWNERS pattern with explicit options.
    pub fn with_options(raw: &str, opts: PatternOptions) -> Result<Self, Error> {
        let normalized = normalize(raw);
        // For case-insensitive matching, lowercase the pattern's literal parts
        // now and lowercase the path at match time.  This keeps the regex
        // case-sensitive (cheaper to compile and execute) while preserving the
        // same semantics as RegexBuilder::case_insensitive(true).
        let pattern_str = if opts.case_sensitive {
            normalized
        } else {
            normalized.to_lowercase()
        };
        let regex_str = to_regex(&pattern_str, opts.implied_children);
        let regex = RegexBuilder::new(&regex_str)
            .build()
            .map_err(|e| Error::InvalidRegex {
                pattern: raw.to_owned(),
                source: e,
            })?;
        Ok(Self {
            original: raw.to_owned(),
            regex,
            fold_case: !opts.case_sensitive,
        })
    }

    /// Returns `true` if `path` matches this pattern.
    ///
    /// Leading `./` and `/` are stripped from the path before matching so
    /// that `./src/foo.rs` and `src/foo.rs` behave identically.
    pub fn matches(&self, path: &str) -> bool {
        let path = path.strip_prefix("./").unwrap_or(path);
        let path = path.strip_prefix('/').unwrap_or(path);
        if self.fold_case {
            self.regex.is_match(&path.to_lowercase())
        } else {
            self.regex.is_match(path)
        }
    }

    /// Whether this pattern folds case (i.e. is case-insensitive).
    /// Use this with [`Pattern::matches_prepared`] to lowercase the path once
    /// before matching against many patterns.
    pub fn fold_case(&self) -> bool {
        self.fold_case
    }

    /// Match against a path that has already been stripped of leading `./`/`/`
    /// and lowercased if [`Pattern::fold_case`] is `true`.
    ///
    /// Use this in hot loops where the same path is matched against many
    /// patterns to avoid redundant normalization and allocation per call.
    pub fn matches_prepared(&self, prepared_path: &str) -> bool {
        self.regex.is_match(prepared_path)
    }

    /// The original (unmodified) pattern string.
    pub fn original(&self) -> &str {
        &self.original
    }
}

/// A compiled set of CODEOWNERS patterns backed by a single `RegexSet` NFA.
///
/// Matching a path against N patterns costs O(path_len) — one NFA pass —
/// rather than O(N × path_len) individual checks.  Use this when matching
/// the same path against a fixed list of patterns many times (e.g. the full
/// rule list from a CODEOWNERS file).
#[derive(Debug, Clone)]
pub struct PatternSet {
    set: RegexSet,
    fold_case: bool,
}

impl PatternSet {
    /// Build a `PatternSet` from raw CODEOWNERS pattern strings.
    ///
    /// Patterns are compiled with `opts` and merged into one NFA via
    /// [`RegexSet`].  The order of `patterns` determines index values
    /// returned by [`PatternSet::first_match`].
    pub fn new<'a, I>(patterns: I, opts: PatternOptions) -> Result<Self, Error>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let regex_strs: Vec<String> = patterns
            .into_iter()
            .map(|raw| {
                let normalized = normalize(raw);
                let s = if opts.case_sensitive {
                    normalized
                } else {
                    normalized.to_lowercase()
                };
                to_regex(&s, opts.implied_children)
            })
            .collect();
        let set = RegexSetBuilder::new(&regex_strs)
            .size_limit(256 * 1024 * 1024) // 256 MB — generous for large CODEOWNERS files
            .build()
            .map_err(|e| Error::InvalidRegex {
                pattern: String::from("<pattern set>"),
                source: e,
            })?;
        Ok(Self {
            set,
            fold_case: !opts.case_sensitive,
        })
    }

    /// Return the index of the **first** (lowest-index) pattern that matches
    /// `path`, or `None` if no pattern matches.
    ///
    /// Strips leading `./` and `/` from the path; lowercases it when the set
    /// was compiled with case-insensitive options.
    pub fn first_match(&self, path: &str) -> Option<usize> {
        let path = path.strip_prefix("./").unwrap_or(path);
        let path = path.strip_prefix('/').unwrap_or(path);
        if self.fold_case {
            self.set.matches(&path.to_lowercase()).into_iter().next()
        } else {
            self.set.matches(path).into_iter().next()
        }
    }
}

pub mod acmatcher;
pub mod fast;

/// Normalize a raw CODEOWNERS pattern into a canonical form ready for
/// [`to_regex`]:
///
/// 1. Strip leading `/` (anchored patterns match root-relative paths).
/// 2. Add a `**/` prefix so unanchored patterns match anywhere in the tree.
/// 3. Expand a trailing `/` to `/**`.
pub(crate) fn normalize(raw: &str) -> String {
    let anchored = raw.starts_with('/');
    let body = if anchored { &raw[1..] } else { raw };

    let add_prefix = !anchored && {
        let inner = body.trim_end_matches('/');
        if inner.starts_with("**") {
            // Already globally scoped.
            false
        } else if inner.starts_with('*') {
            // Single-star prefix (e.g. `*.js`): add `**/` only when there is
            // no `/` in the body so the `*` doesn't need to cross separators.
            !inner.contains('/')
        } else {
            // Plain name or relative path without an explicit anchor
            // (e.g. `docs/*`, `Makefile`, `lib/`): prepend `**/`.
            true
        }
    };

    let mut result = if add_prefix {
        format!("**/{}", body)
    } else {
        body.to_owned()
    };

    // Trailing `/` means "this directory and all its contents".
    if result.ends_with('/') {
        result.push_str("**");
    }

    result
}

/// Translate a normalized glob pattern into an anchored regex string.
fn to_regex(pattern: &str, implied_children: bool) -> String {
    let mut body = String::with_capacity(pattern.len() * 2);
    let b = pattern.as_bytes();
    let n = b.len();
    let mut i = 0;

    while i < n {
        // ── Double star ──────────────────────────────────────────────────
        if b[i] == b'*' && i + 1 < n && b[i + 1] == b'*' {
            let at_start = i == 0;
            let after_slash = i > 0 && b[i - 1] == b'/';
            let next_is_slash = i + 2 < n && b[i + 2] == b'/';
            let at_end = i + 2 >= n;

            if at_start && next_is_slash {
                // `**/` at the very start → optional any-depth prefix.
                body.push_str("(.+/)?");
                i += 3;
            } else if after_slash && next_is_slash {
                // `/**/` in the middle → the preceding `/` is already in
                // `body`; add an optional any-depth bridge.
                body.push_str("(.*/)?");
                i += 3;
            } else if after_slash && at_end {
                // `/**` at the end → match everything after the slash.
                body.push_str(".*");
                i += 2;
            } else {
                // `**` in any other position.
                body.push_str(".*");
                i += 2;
            }
        }
        // ── Single star ──────────────────────────────────────────────────
        else if b[i] == b'*' {
            body.push_str("[^/]*");
            i += 1;
        }
        // ── Question mark ────────────────────────────────────────────────
        else if b[i] == b'?' {
            body.push_str("[^/]");
            i += 1;
        }
        // ── Character class ──────────────────────────────────────────────
        else if b[i] == b'[' {
            body.push('[');
            i += 1;
            if i < n && (b[i] == b'!' || b[i] == b'^') {
                body.push('^');
                i += 1;
            }
            // A `]` as the first character inside `[` is treated as literal.
            if i < n && b[i] == b']' {
                body.push(']');
                i += 1;
            }
            while i < n && b[i] != b']' {
                if b[i] == b'\\' && i + 1 < n {
                    body.push('\\');
                    body.push(b[i + 1] as char);
                    i += 2;
                } else {
                    body.push(b[i] as char);
                    i += 1;
                }
            }
            if i < n {
                body.push(']');
                i += 1;
            }
        }
        // ── Backslash escape → literal next character ────────────────────
        else if b[i] == b'\\' && i + 1 < n {
            let next = b[i + 1];
            if is_regex_meta(next) {
                body.push('\\');
            }
            body.push(next as char);
            i += 2;
        }
        // ── Literal character ────────────────────────────────────────────
        else {
            if is_regex_meta(b[i]) {
                body.push('\\');
            }
            body.push(b[i] as char);
            i += 1;
        }
    }

    let last = pattern.rsplit('/').next().unwrap_or(pattern);
    let needs_children = implied_children
        && !last.is_empty()
        && !last.contains('*')
        && !last.contains('?')
        && !last.contains('[')
        && {
            let rest = last.strip_prefix('.').unwrap_or(last);
            !rest.is_empty() && !rest.contains('.')
        };

    if needs_children {
        format!("^{}(/.*)?$", body)
    } else {
        format!("^{}$", body)
    }
}

#[inline]
fn is_regex_meta(b: u8) -> bool {
    matches!(
        b,
        b'.' | b'+' | b'(' | b')' | b'{' | b'}' | b'^' | b'$' | b'|' | b'\\'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pattern: &str, path: &str) -> bool {
        Pattern::new(pattern)
            .unwrap_or_else(|e| panic!("bad pattern `{pattern}`: {e}"))
            .matches(path)
    }

    // ── Single star ───────────────────────────────────────────────────────

    #[test]
    fn star_matches_any_file_at_any_depth() {
        assert!(m("*", "foo.txt"));
        assert!(m("*", "foo/bar.txt"));
        assert!(m("*", "a/b/c/d.ts"));
    }

    #[test]
    fn star_extension_matches_anywhere() {
        assert!(m("*.js", "foo.js"));
        assert!(m("*.js", "foo/bar.js"));
        assert!(m("*.js", "a/b/c/d.js"));
        assert!(!m("*.js", "foo.ts"));
        assert!(!m("*.js", "foo/bar.ts"));
    }

    #[test]
    fn question_mark_matches_single_non_separator_char() {
        assert!(m("foo.?s", "foo.ts"));
        assert!(m("foo.?s", "foo.js"));
        assert!(!m("foo.?s", "foo.tsx")); // two chars, not one
        assert!(m("foo/?s", "foo/ts")); // ? matches non-separator char t
        assert!(!m("src?lib", "src/lib")); // ? doesn't match /
    }

    // ── Double star ───────────────────────────────────────────────────────

    #[test]
    fn doublestar_prefix_matches_any_depth() {
        assert!(m("**/*.md", "README.md"));
        assert!(m("**/*.md", "docs/README.md"));
        assert!(m("**/*.md", "a/b/c/README.md"));
        assert!(!m("**/*.md", "README.rs"));
    }

    #[test]
    fn doublestar_suffix_matches_all_contents() {
        assert!(m("docs/**", "docs/index.md"));
        assert!(m("docs/**", "docs/api/reference.md"));
        assert!(!m("docs/**", "src/index.md"));
    }

    #[test]
    fn doublestar_middle_spans_depth() {
        assert!(m("src/**/handler.rs", "src/handler.rs"));
        assert!(m("src/**/handler.rs", "src/api/handler.rs"));
        assert!(m("src/**/handler.rs", "src/api/v2/handler.rs"));
        assert!(!m("src/**/handler.rs", "lib/api/handler.rs"));
    }

    #[test]
    fn doublestar_alone_matches_everything() {
        assert!(m("**", "foo.txt"));
        assert!(m("**", "a/b/c/foo.txt"));
    }

    // ── Anchoring ─────────────────────────────────────────────────────────

    #[test]
    fn leading_slash_anchors_to_root() {
        assert!(m("/docs/*.md", "docs/foo.md"));
        assert!(!m("/docs/*.md", "src/docs/foo.md"));
    }

    #[test]
    fn no_leading_slash_matches_anywhere() {
        assert!(m("docs/*.md", "docs/foo.md"));
        assert!(m("docs/*.md", "src/docs/foo.md"));
        assert!(m("docs/*.md", "a/b/docs/foo.md"));
    }

    // ── Trailing slash ────────────────────────────────────────────────────

    #[test]
    fn anchored_trailing_slash_matches_subtree() {
        assert!(m("/docs/", "docs/foo.md"));
        assert!(m("/docs/", "docs/sub/foo.md"));
        assert!(!m("/docs/", "src/docs/foo.md")); // anchored
    }

    #[test]
    fn unanchored_trailing_slash_matches_anywhere() {
        assert!(m("lib/", "lib/foo.rs"));
        assert!(m("lib/", "src/lib/foo.rs"));
        assert!(m("lib/", "a/b/lib/foo.rs"));
    }

    // ── Implied children ──────────────────────────────────────────────────

    #[test]
    fn plain_name_implies_children() {
        assert!(m("foo/bar", "foo/bar"));
        assert!(m("foo/bar", "foo/bar/baz.rs"));
        assert!(m("foo/bar", "foo/bar/deep/baz.rs"));
        assert!(!m("foo/bar", "foo/baz.rs"));
    }

    #[test]
    fn anchored_plain_name_implies_children() {
        assert!(m("/src/components", "src/components/button.rs"));
        assert!(m("/src/components", "src/components/ui/deep/button.rs"));
        assert!(!m("/src/components", "lib/src/components/button.rs"));
    }

    #[test]
    fn doublestar_plain_name_implies_children() {
        // **/another should match the name anywhere AND its contents.
        assert!(m("**/another", "another"));
        assert!(m("**/another", "foo/another"));
        assert!(m("**/another", "foo/bar/another"));
        assert!(m("**/another", "foo/another/README.md"));
    }

    // ── Extension patterns ────────────────────────────────────────────────

    #[test]
    fn dotfile_has_no_implied_children() {
        // `.gitignore` looks like a plain name but contains a `.` after
        // stripping the leading dot — no implicit `/**` suffix.
        assert!(m("*.gitignore", "foo.gitignore"));
        assert!(!m("*.gitignore", "foo.gitignore/bar")); // not a dir
    }

    #[test]
    fn extension_pattern_has_no_implied_children() {
        assert!(m("*.js", "foo.js"));
        assert!(!m("*.js", "foo.js/bar")); // not a dir
    }

    // ── Relative path prefix stripping ────────────────────────────────────

    #[test]
    fn dot_slash_prefix_stripped_before_match() {
        assert!(m("*.js", "./foo.js"));
        assert!(m("/docs/", "./docs/foo.md"));
        assert!(m("apps/", "./apps/foo.js"));
    }

    #[test]
    fn leading_slash_stripped_from_path() {
        assert!(m("*.js", "/foo.js"));
        assert!(m("/docs/", "/docs/foo.md"));
    }

    // ── Character classes ─────────────────────────────────────────────────

    #[test]
    fn character_class_positive() {
        assert!(m("*.[ch]", "foo.c"));
        assert!(m("*.[ch]", "foo.h"));
        assert!(!m("*.[ch]", "foo.rs"));
    }

    #[test]
    fn character_class_negated() {
        assert!(m("*.[!ch]", "foo.r"));
        assert!(!m("*.[!ch]", "foo.c"));
    }

    // ── Backslash escape ──────────────────────────────────────────────────

    #[test]
    fn backslash_escapes_space() {
        assert!(m(r"path\ with\ spaces/", "path with spaces/foo.txt"));
        assert!(m(r"path\ with\ spaces/", "path with spaces/sub/foo.txt"));
    }

    // ── Fixture-driven scenarios ───────────────────────────────────────────

    #[test]
    fn global_wildcard_owns_everything() {
        assert!(m("*", "foo.txt"));
        assert!(m("*", "foo/bar/baz.txt"));
    }

    #[test]
    fn anchored_build_logs_is_root_relative() {
        assert!(m("/build/logs/", "build/logs/foo.go"));
        assert!(m("/build/logs/", "build/logs/sub/foo.go"));
        assert!(!m("/build/logs/", "foo/build/logs/foo.go")); // anchored
    }

    #[test]
    fn direct_children_glob_does_not_match_deeper() {
        // docs/* matches direct children but not deeper nesting
        assert!(m("docs/*", "foo/docs/bar.md")); // unanchored → matches anywhere
        assert!(!m("docs/*", "foo/docs/sub/bar.md")); // * stops at /
    }

    #[test]
    fn unanchored_dir_matches_anywhere() {
        assert!(m("apps/", "foo/apps/main.ts"));
        assert!(m("apps/", "apps/main.ts"));
    }

    #[test]
    fn case_insensitive() {
        assert!(m("*.JS", "foo.js"));
        assert!(m("*.js", "foo.JS"));
        assert!(m("/Docs/", "docs/foo.md"));
    }
}
