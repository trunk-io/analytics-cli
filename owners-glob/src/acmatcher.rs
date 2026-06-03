//! Aho-Corasick-based multi-pattern matcher.
//!
//! Instead of hashing a 4-byte fingerprint per pattern, [`AcMatcher`] extracts
//! the longest *required literal* from each pattern (the literal prefix before
//! the first wildcard, or the literal suffix after the last wildcard, whichever
//! is longer) and builds a single [`AhoCorasick`] automaton over all of them.
//!
//! At match time the path is fed through the automaton once — O(path_len) —
//! and every pattern whose required literal appears in the path becomes a
//! candidate.  Only candidates run the per-pattern check.  Patterns with no
//! extractable literal (e.g. bare `*` or `**`) are always candidates.

use std::collections::HashMap;

use aho_corasick::AhoCorasick;

use crate::fast::{
    MatchFn, classify, implies_children_in, match_anchored_dir, match_unanchored_dir,
    match_unanchored_file,
};
use crate::{PatternOptions, normalize};

/// Pre-built glob strings for a single `MatchFn::Regex` pattern.  The
/// `children` variant is `Some` when implied-children semantics apply
/// (e.g. `/foo/**/bar` also matches `foo/x/bar/sub.txt`).
#[derive(Debug, Clone)]
struct GlobPattern {
    norm: Box<str>,
    children: Option<Box<str>>,
}

// ── Literal extraction ────────────────────────────────────────────────────────

/// Extract the single best required literal from a raw glob pattern.
///
/// Splits on wildcard characters (`* ? [ ] { }`) and picks the longest
/// resulting segment (after stripping surrounding `/`).  Interior literals
/// like `lib` in `/**/lib/**/*` are extracted correctly.
///
/// Returns `None` for patterns that are entirely wildcard (e.g. `*`, `**/*`).
fn extract_literal(raw: &str) -> Option<String> {
    // Strip the leading '/' used for root-anchoring; it's not part of the path.
    let body = raw.strip_prefix('/').unwrap_or(raw);

    // Split on wildcard characters and pick the longest literal segment.
    // Require at least 2 bytes — single characters (`.`, `/`) hit almost
    // everything and provide no useful filtering.
    let best = body
        .split(|c: char| matches!(c, '*' | '?' | '[' | ']' | '{' | '}'))
        .map(|s| s.trim_matches('/'))
        .filter(|s| s.len() >= 2)
        .max_by_key(|s| s.len())?;

    Some(best.to_owned())
}

// ── FastGlobMatcher ───────────────────────────────────────────────────────────

/// Whether a normalised pattern requires an additional `/**` check to satisfy
/// "implied children" semantics (e.g. `/src/components` also matches
/// `src/components/button.rs`).
fn needs_implied_children(norm: &str, implied_children: bool) -> bool {
    implied_children && !norm.ends_with('*') && !norm.ends_with('/')
}

/// AC pre-filter + [`fast_glob`] check — no regex engine anywhere.
///
/// Unlike [`AcMatcher`], no patterns are pre-compiled to regex; the only
/// build-time work is literal extraction and the AC automaton.  Each
/// surviving candidate is matched by calling [`fast_glob::glob_match`]
/// directly on the normalised pattern string.
#[derive(Debug, Clone)]
pub struct FastGlobMatcher {
    /// Normalised glob strings (fast-glob format), one per pattern.
    normalized: Vec<String>,
    /// For patterns with implied-children semantics, the `{norm}/**` variant
    /// so we avoid a `format!` allocation on every check.
    children_pat: Vec<Option<String>>,
    fns: Vec<MatchFn>,
    ac: AhoCorasick,
    ac_to_patterns: Vec<Vec<usize>>,
    always_candidates: Vec<usize>,
    fold_case: bool,
    len: usize,
}

impl FastGlobMatcher {
    pub fn new<'a, I>(raw_patterns: I, opts: PatternOptions) -> Result<Self, crate::Error>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let fold_case = !opts.case_sensitive;
        let mut normalized = Vec::new();
        let mut children_pat = Vec::new();
        let mut fns = Vec::new();
        let mut always_candidates = Vec::new();
        let mut literal_map: HashMap<String, Vec<usize>> = HashMap::new();

        for raw in raw_patterns {
            let idx = normalized.len();
            let norm = normalize(raw);
            let raw_key = if fold_case {
                raw.to_lowercase()
            } else {
                raw.to_owned()
            };
            let norm_key = if fold_case { norm.to_lowercase() } else { norm };

            let child = if needs_implied_children(&norm_key, opts.implied_children) {
                Some(format!("{norm_key}/**"))
            } else {
                None
            };

            fns.push(classify(&norm_key, opts.implied_children));
            normalized.push(norm_key);
            children_pat.push(child);

            match extract_literal(&raw_key) {
                Some(lit) => literal_map.entry(lit).or_default().push(idx),
                None => always_candidates.push(idx),
            }
        }

        let len = normalized.len();
        let (literals, ac_to_patterns): (Vec<String>, Vec<Vec<usize>>) =
            literal_map.into_iter().unzip();
        let ac = AhoCorasick::new(&literals)
            .map_err(|e| crate::Error::AhoCorasickBuild(e.to_string()))?;

        Ok(Self {
            normalized,
            children_pat,
            fns,
            ac,
            ac_to_patterns,
            always_candidates,
            fold_case,
            len,
        })
    }

    pub fn first_match(&self, path: &str) -> Option<usize> {
        let path = path.strip_prefix("./").unwrap_or(path);
        let path = path.strip_prefix('/').unwrap_or(path);
        let lowered;
        let path: &str = if self.fold_case {
            lowered = path.to_lowercase();
            &lowered
        } else {
            path
        };

        let mut candidates: Vec<usize> = self.always_candidates.clone();
        // Overlapping search: a shorter literal (e.g. "foo/bar") must not
        // shadow a longer one starting at the same position ("foo/bar/baz").
        for mat in self.ac.find_overlapping_iter(path) {
            candidates.extend_from_slice(&self.ac_to_patterns[mat.pattern().as_usize()]);
        }
        candidates.sort_unstable();
        candidates.dedup();
        candidates.into_iter().find(|&idx| self.check(idx, path))
    }

    #[inline(always)]
    fn check(&self, idx: usize, path: &str) -> bool {
        match &self.fns[idx] {
            MatchFn::Always => true,
            MatchFn::EndsWith(ext) => path.ends_with(ext.as_ref()),
            MatchFn::StartsAndEndsWith(pfx, ext) => {
                path.starts_with(pfx.as_ref()) && path.ends_with(ext.as_ref())
            }
            MatchFn::AnchoredDir(s) => match_anchored_dir(s, path),
            MatchFn::AnchoredExact(s) => path == s.as_ref(),
            MatchFn::UnanchoredDir(s) => match_unanchored_dir(s, path),
            MatchFn::UnanchoredFile(s) => match_unanchored_file(s, path),
            MatchFn::Regex => {
                let norm = &self.normalized[idx];
                if fast_glob::glob_match(norm, path) {
                    return true;
                }
                // Implied-children: also match anything nested under this entry.
                self.children_pat[idx]
                    .as_deref()
                    .is_some_and(|cp| fast_glob::glob_match(cp, path))
            }
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

// ── AcMatcher ─────────────────────────────────────────────────────────────────

/// Aho-Corasick-based matcher for a fixed list of patterns.
///
/// Build once per CODEOWNERS file with [`AcMatcher::new`], then call
/// [`AcMatcher::first_match`] for each path.
///
/// Patterns are routed at build time to whichever index handles them most
/// efficiently:
/// - Anchored literals (`/foo/bar`) → `prefix_map` (HashMap, O(path_depth) lookup)
/// - Unanchored literals (`foo/bar`) → AC automaton (substring scan with boundary verification)
/// - Extension patterns (`*.ext`, `/x/**/*.ext`) → AC keyed on the extension/prefix literal
/// - Pure wildcards (no extractable literal) → `always_candidates`
#[derive(Debug, Clone)]
pub struct AcMatcher {
    /// Pre-built glob strings per pattern index, `Some` only when the `MatchFn`
    /// at that index is `MatchFn::Regex` (i.e. needs a real glob check rather
    /// than a string-shape fast path).  We hold normalised glob strings —
    /// matched via [`fast_glob::glob_match`] — instead of compiled regex
    /// because compilation is the dominant cost in `new()` and fast_glob hits
    /// the cross-over at ~850K matches per parse.
    globs: Vec<Option<GlobPattern>>,
    fns: Vec<MatchFn>,
    /// Aho-Corasick automaton built from one literal per pattern group.
    ac: AhoCorasick,
    /// `ac_to_patterns[ac_pattern_id]` = glob pattern indices that share this literal.
    ac_to_patterns: Vec<Vec<usize>>,
    /// Pattern indices with no extractable literal — always candidates.
    always_candidates: Vec<usize>,
    /// Map from anchored literal prefix → pattern indices.  Used for O(path_depth)
    /// longest-prefix lookup of `AnchoredDir` / `AnchoredExact` patterns.
    prefix_map: HashMap<Box<str>, Vec<usize>>,
    /// Pattern indices routed through the AC index — kept around so a benchmark
    /// alternative can do a linear scan over the same set without AC.
    literal_indices: Vec<usize>,
    fold_case: bool,
    len: usize,
}

impl AcMatcher {
    /// Build an `AcMatcher` from an ordered list of raw pattern strings.
    ///
    /// Patterns at lower indices have higher priority (pass them highest-priority-first,
    /// i.e. the reversed order of the CODEOWNERS file).
    pub fn new<'a, I>(raw_patterns: I, opts: PatternOptions) -> Result<Self, crate::Error>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let fold_case = !opts.case_sensitive;
        let mut globs: Vec<Option<GlobPattern>> = Vec::new();
        let mut fns = Vec::new();
        let mut always_candidates = Vec::new();
        let mut literal_indices: Vec<usize> = Vec::new();

        // literal substring → list of pattern indices that require it (for AC scan)
        let mut literal_map: HashMap<String, Vec<usize>> = HashMap::new();
        // anchored prefix → list of pattern indices (for prefix lookup)
        let mut prefix_map: HashMap<Box<str>, Vec<usize>> = HashMap::new();

        for raw in raw_patterns {
            let idx = globs.len();
            let normalised = normalize(raw);
            let norm_key = if fold_case {
                normalised.to_lowercase()
            } else {
                normalised
            };
            let raw_key = if fold_case {
                raw.to_lowercase()
            } else {
                raw.to_owned()
            };

            let fn_ = classify(&norm_key, opts.implied_children);
            let needs_glob = matches!(fn_, MatchFn::Regex);

            // Route to the right index based on match function shape.
            match &fn_ {
                MatchFn::AnchoredDir(s) | MatchFn::AnchoredExact(s) => {
                    prefix_map.entry(s.clone()).or_default().push(idx);
                }
                MatchFn::UnanchoredDir(s) | MatchFn::UnanchoredFile(s) => {
                    literal_map.entry(s.to_string()).or_default().push(idx);
                    literal_indices.push(idx);
                }
                MatchFn::EndsWith(_)
                | MatchFn::StartsAndEndsWith(_, _)
                | MatchFn::Always
                | MatchFn::Regex => {
                    // Best-effort literal extraction for AC pre-filtering.
                    match extract_literal(&raw_key) {
                        Some(lit) => {
                            literal_map.entry(lit).or_default().push(idx);
                            literal_indices.push(idx);
                        }
                        None => always_candidates.push(idx),
                    }
                }
            }

            // Pre-build the glob strings only for the Regex variant.  Skipping
            // this for the literal/extension fast paths is the bulk of the
            // parse-time win — fast_glob avoids regex compilation entirely.
            let glob_pat = if needs_glob {
                let last = norm_key.rsplit('/').next().unwrap_or(&norm_key);
                let children = if opts.implied_children && implies_children_in(last) {
                    Some(format!("{}/**", norm_key).into_boxed_str())
                } else {
                    None
                };
                Some(GlobPattern {
                    norm: norm_key.into_boxed_str(),
                    children,
                })
            } else {
                None
            };
            globs.push(glob_pat);
            fns.push(fn_);
        }

        let len = globs.len();

        let (literals, ac_to_patterns): (Vec<String>, Vec<Vec<usize>>) =
            literal_map.into_iter().unzip();

        let ac = AhoCorasick::new(&literals)
            .map_err(|e| crate::Error::AhoCorasickBuild(e.to_string()))?;

        literal_indices.sort_unstable();

        Ok(Self {
            globs,
            fns,
            ac,
            ac_to_patterns,
            always_candidates,
            prefix_map,
            literal_indices,
            fold_case,
            len,
        })
    }

    /// Return the index of the first (highest-priority) pattern that matches
    /// `path`, or `None` if nothing matches.
    pub fn first_match(&self, path: &str) -> Option<usize> {
        let path = path.strip_prefix("./").unwrap_or(path);
        let path = path.strip_prefix('/').unwrap_or(path);

        let lowered;
        let path: &str = if self.fold_case {
            lowered = path.to_lowercase();
            &lowered
        } else {
            path
        };

        let mut candidates: Vec<usize> = self.always_candidates.clone();

        // Prefix-map lookup: walk path prefixes from longest to shortest and
        // collect any anchored-literal patterns that match.  O(path_depth)
        // hash probes — typically 5–8 per path.
        if !self.prefix_map.is_empty() {
            let mut p = path;
            loop {
                if let Some(indices) = self.prefix_map.get(p) {
                    candidates.extend_from_slice(indices);
                }
                match p.rfind('/') {
                    Some(i) => p = &p[..i],
                    None => break,
                }
            }
        }

        // AC scan for unanchored literals + extension/regex patterns.  Skip
        // entirely when no patterns were routed through the AC index — the
        // automaton call costs ~150ns even with zero literals, which is a
        // measurable loss when every pattern lives in `prefix_map`.  Use
        // overlapping search so a shorter literal (e.g. "foo/bar") never
        // shadows a longer one starting at the same position ("foo/bar/baz").
        if !self.ac_to_patterns.is_empty() {
            for mat in self.ac.find_overlapping_iter(path) {
                candidates.extend_from_slice(&self.ac_to_patterns[mat.pattern().as_usize()]);
            }
        }

        // Evaluate in priority order (lowest index = highest priority).
        candidates.sort_unstable();
        candidates.dedup();
        candidates.into_iter().find(|&idx| self.check(idx, path))
    }

    #[inline(always)]
    fn check(&self, idx: usize, path: &str) -> bool {
        match &self.fns[idx] {
            MatchFn::Always => true,
            MatchFn::EndsWith(ext) => path.ends_with(ext.as_ref()),
            MatchFn::StartsAndEndsWith(pfx, ext) => {
                path.starts_with(pfx.as_ref()) && path.ends_with(ext.as_ref())
            }
            MatchFn::AnchoredDir(s) => match_anchored_dir(s, path),
            MatchFn::AnchoredExact(s) => path == s.as_ref(),
            MatchFn::UnanchoredDir(s) => match_unanchored_dir(s, path),
            MatchFn::UnanchoredFile(s) => match_unanchored_file(s, path),
            MatchFn::Regex => {
                let g = self.globs[idx]
                    .as_ref()
                    .expect("MatchFn::Regex must have a pre-built glob");
                if fast_glob::glob_match(&*g.norm, path) {
                    return true;
                }
                g.children
                    .as_deref()
                    .is_some_and(|c| fast_glob::glob_match(c, path))
            }
        }
    }

    /// Benchmark variant: keep the prefix-map but linearly scan every
    /// AC-routed pattern instead of using the AC automaton.  Used to measure
    /// what the AC pre-filter is actually buying us on a given workload.
    pub fn first_match_no_ac(&self, path: &str) -> Option<usize> {
        let path = path.strip_prefix("./").unwrap_or(path);
        let path = path.strip_prefix('/').unwrap_or(path);
        let lowered;
        let path: &str = if self.fold_case {
            lowered = path.to_lowercase();
            &lowered
        } else {
            path
        };

        let mut candidates: Vec<usize> = self.always_candidates.clone();
        if !self.prefix_map.is_empty() {
            let mut p = path;
            loop {
                if let Some(indices) = self.prefix_map.get(p) {
                    candidates.extend_from_slice(indices);
                }
                match p.rfind('/') {
                    Some(i) => p = &p[..i],
                    None => break,
                }
            }
        }
        candidates.extend_from_slice(&self.literal_indices);
        candidates.sort_unstable();
        candidates.dedup();
        candidates.into_iter().find(|&idx| self.check(idx, path))
    }

    /// Benchmark variant: no AC, no prefix-map — pure linear scan over every
    /// pattern.  The naive baseline.
    pub fn first_match_linear(&self, path: &str) -> Option<usize> {
        let path = path.strip_prefix("./").unwrap_or(path);
        let path = path.strip_prefix('/').unwrap_or(path);
        let lowered;
        let path: &str = if self.fold_case {
            lowered = path.to_lowercase();
            &lowered
        } else {
            path
        };
        (0..self.len).find(|&idx| self.check(idx, path))
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}
