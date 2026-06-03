//! SIMD-accelerated CODEOWNERS pattern matching.
//!
//! [`FastMatcher`] builds two fingerprint arrays alongside the compiled
//! patterns — one for the file extension and one for the anchored root prefix.
//! Each call to [`FastMatcher::first_match`]:
//!
//! 1. Extracts the path's 4-byte extension and prefix fingerprints (a few
//!    nanoseconds).
//! 2. Scans both fingerprint arrays eight lanes at a time with `u32x8` SIMD
//!    comparisons, producing a bitmask of candidate patterns.
//! 3. Only candidates run the per-pattern check, which is a fast
//!    `ends_with` / `starts_with` for the common cases and a regex fall-back
//!    for complex patterns.
//!
//! For N patterns the inner loop executes in O(N/8) SIMD ops rather than
//! O(N) regex evaluations, and the fast-path checks avoid regex overhead
//! entirely for the most common CODEOWNERS pattern shapes.

use std::simd::prelude::*;

use crate::{Pattern, PatternOptions, normalize};

// ── Fingerprints ─────────────────────────────────────────────────────────────

/// Extract a 4-byte extension fingerprint from a path or normalised pattern.
///
/// Returns `0` if there is no deterministic literal extension (wildcards in
/// the extension, no extension, etc.).  `0` is the "unconstrained" sentinel
/// so patterns / paths without an extension always stay in the candidate set.
pub(crate) fn ext_fp(s: &str) -> u32 {
    // Strip trailing slashes/wildcards so `src/` and `**/*` give fp = 0.
    let s = s.trim_end_matches(|c| c == '/' || c == '*');
    let last_slash = s.rfind('/').map_or(0, |i| i + 1);
    let last = &s[last_slash..];
    let Some(dot) = last.rfind('.') else { return 0 };
    let ext = &last[dot..];
    if ext.bytes().any(|b| matches!(b, b'*' | b'?' | b'[')) {
        return 0;
    }
    let b = ext.as_bytes();
    let mut fp = [0u8; 4];
    fp[..b.len().min(4)].copy_from_slice(&b[..b.len().min(4)]);
    u32::from_le_bytes(fp)
}

/// 4-byte fingerprint of the first path component (for paths, not patterns).
pub(crate) fn path_prefix_fp(path: &str) -> u32 {
    let end = path.find('/').unwrap_or(path.len());
    let b = path[..end].as_bytes();
    let mut fp = [0u8; 4];
    fp[..b.len().min(4)].copy_from_slice(&b[..b.len().min(4)]);
    u32::from_le_bytes(fp)
}

/// 4-byte fingerprint of the first component of a ROOT-ANCHORED pattern.
/// Returns `0` for unanchored patterns (they can match any first component).
///
/// Anchoring follows the same rule as [`normalize`]: only a leading `/`
/// anchors a pattern.  `docs/*` and `src/api/` are both unanchored here
/// because `normalize` prepends `**/` to them.
pub(crate) fn anchored_prefix_fp(raw: &str) -> u32 {
    let Some(body) = raw.strip_prefix('/') else {
        return 0;
    };
    let end = body.find('/').unwrap_or(body.len());
    let first = &body[..end];
    if first.bytes().any(|b| matches!(b, b'*' | b'?' | b'[')) {
        return 0;
    }
    let b = first.as_bytes();
    let mut fp = [0u8; 4];
    fp[..b.len().min(4)].copy_from_slice(&b[..b.len().min(4)]);
    u32::from_le_bytes(fp)
}

// ── Pattern classification ────────────────────────────────────────────────────

/// Fast per-pattern dispatch — avoids regex for the common pattern shapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MatchFn {
    /// `**` / `**/*` — matches everything.
    Always,
    /// `*.ext` / `**/*.ext` — `path.ends_with(ext)`.
    EndsWith(Box<str>),
    /// `/prefix/**/*.ext` (anchored) — `starts_with(prefix) && ends_with(ext)`.
    StartsAndEndsWith(Box<str>, Box<str>),
    /// Anchored literal with implied children (e.g. `/foo/bar`, `/foo/bar/`).
    /// Matches `path == s` or `path.starts_with(s + "/")`.
    AnchoredDir(Box<str>),
    /// Anchored literal that does NOT imply children (e.g. `/foo.ext`).
    /// Matches only `path == s`.
    AnchoredExact(Box<str>),
    /// Unanchored literal with implied children (e.g. `foo/bar`, `foo/bar/`).
    /// Matches when `s` appears as an aligned path-segment sequence anywhere
    /// in `path`, with implied children below it.
    UnanchoredDir(Box<str>),
    /// Unanchored literal that does NOT imply children (e.g. `foo.ext`).
    /// Matches when `path` ends with `/{s}` or equals `s`.
    UnanchoredFile(Box<str>),
    /// Everything else — run the compiled regex.
    Regex,
}

/// Whether a single path component implies children (i.e. a plain name without
/// a `.` extension and without wildcards).  Mirrors the rule used by
/// [`crate::to_regex`] when deciding whether to append `(/.*)?`.
pub(crate) fn implies_children_in(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if name.contains(|c: char| "*?[".contains(c)) {
        return false;
    }
    let rest = name.strip_prefix('.').unwrap_or(name);
    !rest.is_empty() && !rest.contains('.')
}

/// Classify a pattern from its NORMALISED form (output of [`normalize`]).
///
/// The normalised form has no leading `/` (anchored patterns just start
/// with the literal component).  Trailing `/` is expanded to `/**`.
pub(crate) fn classify(normalised: &str, implied_children: bool) -> MatchFn {
    // ── Always ────────────────────────────────────────────────────────────
    // `"*"` is excluded: it is the result of normalising `/*` (anchored
    // single-star) which only matches root-level files (regex `^[^/]*$`),
    // not all paths.  Only `**` and `**/*` genuinely match everything.
    if matches!(normalised, "**" | "**/*") {
        return MatchFn::Always;
    }

    // ── EndsWith: `**/*.ext` (unanchored extension wildcard) ──────────────
    if let Some(after) = normalised.strip_prefix("**/*.") {
        if !after.is_empty() && !after.contains(|c: char| "/.*?[".contains(c)) {
            return MatchFn::EndsWith(format!(".{after}").into());
        }
    }

    // ── Unanchored literal (`**/foo`, `**/foo/**`) ────────────────────────
    if let Some(after) = normalised.strip_prefix("**/") {
        let trailing_dstar = after.ends_with("/**");
        let body = after.strip_suffix("/**").unwrap_or(after);
        if !body.is_empty() && !body.contains(|c: char| "*?[".contains(c)) {
            let last = body.rsplit('/').next().unwrap_or(body);
            let with_children = trailing_dstar || (implied_children && implies_children_in(last));
            return if with_children {
                MatchFn::UnanchoredDir(body.into())
            } else {
                MatchFn::UnanchoredFile(body.into())
            };
        }
    }

    // ── StartsAndEndsWith: `prefix/**/*.ext` (anchored) ───────────────────
    if !normalised.starts_with("**/") {
        if let Some(slash) = normalised.find('/') {
            let prefix = &normalised[..slash];
            if !prefix.is_empty() && !prefix.contains(|c: char| "*?[".contains(c)) {
                let rest = &normalised[slash + 1..];
                let inner = rest.strip_prefix("**/").unwrap_or(rest);
                if let Some(ext) = inner.strip_prefix("*.") {
                    if !ext.is_empty() && !ext.contains(|c: char| "/.*?[".contains(c)) {
                        return MatchFn::StartsAndEndsWith(
                            format!("{prefix}/").into(),
                            format!(".{ext}").into(),
                        );
                    }
                }
            }
        }
    }

    // ── Anchored literal (`foo`, `foo/bar`, `foo/**`) ─────────────────────
    if !normalised.starts_with("**/") {
        let trailing_dstar = normalised.ends_with("/**");
        let body = normalised.strip_suffix("/**").unwrap_or(normalised);
        if !body.is_empty() && !body.contains(|c: char| "*?[".contains(c)) {
            let last = body.rsplit('/').next().unwrap_or(body);
            let with_children = trailing_dstar || (implied_children && implies_children_in(last));
            return if with_children {
                MatchFn::AnchoredDir(body.into())
            } else {
                MatchFn::AnchoredExact(body.into())
            };
        }
    }

    MatchFn::Regex
}

// ── Match-function execution helpers ──────────────────────────────────────────

/// `path == s` OR `path` starts with `s + "/"`.  Both arguments are assumed to
/// be in the same case (the caller lowercased `path` if needed).
#[inline]
pub(crate) fn match_anchored_dir(s: &str, path: &str) -> bool {
    if path.len() == s.len() {
        return path == s;
    }
    if path.len() > s.len() && path.starts_with(s) {
        // path is longer; the byte at s.len() must be '/'
        return path.as_bytes()[s.len()] == b'/';
    }
    false
}

/// `path` ends with `s` *aligned to a path boundary* (i.e. `path == s` or the
/// byte immediately before the suffix is `/`).
#[inline]
pub(crate) fn match_unanchored_file(s: &str, path: &str) -> bool {
    if path.len() == s.len() {
        return path == s;
    }
    if path.len() > s.len() && path.ends_with(s) {
        return path.as_bytes()[path.len() - s.len() - 1] == b'/';
    }
    false
}

/// `s` appears as an aligned path-segment sequence anywhere in `path`, with
/// implied children below it.  Equivalent to checking whether `/{path}/`
/// contains `/{s}/`.
#[inline]
pub(crate) fn match_unanchored_dir(s: &str, path: &str) -> bool {
    let sb = s.as_bytes();
    let pb = path.as_bytes();
    let m = sb.len();
    let n = pb.len();
    if m == 0 || m > n {
        return path == s;
    }
    // Check leftmost position: path starts with s and (path == s OR next byte is '/')
    if pb.starts_with(sb) && (m == n || pb[m] == b'/') {
        return true;
    }
    // Scan interior positions: byte before must be '/' and byte after must be '/' or end.
    let mut i = 1usize;
    while i + m <= n {
        if pb[i - 1] == b'/' && &pb[i..i + m] == sb {
            let after = i + m;
            if after == n || pb[after] == b'/' {
                return true;
            }
        }
        i += 1;
    }
    false
}

// ── FastMatcher ───────────────────────────────────────────────────────────────

const LANE: usize = 8; // u32x8 — 8 patterns per SIMD compare

/// SIMD-accelerated matcher for a fixed list of patterns.
///
/// Build once per CODEOWNERS file, call [`FastMatcher::first_match`] for
/// every path.  The returned index is the position of the highest-priority
/// matching pattern in the list supplied to [`FastMatcher::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastMatcher {
    patterns: Vec<Pattern>,
    fns: Vec<MatchFn>,
    /// Extension fingerprints, padded to a multiple of LANE with u32::MAX.
    ext_fps: Vec<u32>,
    /// Anchored-prefix fingerprints, same padding.
    pfx_fps: Vec<u32>,
    fold_case: bool,
    len: usize,
}

impl FastMatcher {
    /// Build a `FastMatcher` from an ordered slice of raw pattern strings.
    ///
    /// Patterns at lower indices have higher priority (pass them
    /// highest-priority-first, which is the reversed order from a CODEOWNERS
    /// file).
    pub fn new<'a, I>(raw_patterns: I, opts: PatternOptions) -> Result<Self, crate::Error>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let fold_case = !opts.case_sensitive;
        let mut patterns = Vec::new();
        let mut fns = Vec::new();
        let mut ext_fps = Vec::new();
        let mut pfx_fps = Vec::new();

        for raw in raw_patterns {
            let normalised = normalize(raw);
            // Fingerprints are computed on the (optionally lowercased) raw
            // pattern because normalize() transforms leading `/` in ways that
            // affect anchored_prefix_fp.
            let raw_key = if fold_case {
                raw.to_lowercase()
            } else {
                raw.to_owned()
            };
            let norm_key = if fold_case {
                normalised.to_lowercase()
            } else {
                normalised.clone()
            };

            ext_fps.push(ext_fp(&raw_key));
            pfx_fps.push(anchored_prefix_fp(&raw_key));
            fns.push(classify(&norm_key, opts.implied_children));
            patterns.push(Pattern::with_options(raw, opts)?);
        }

        let len = patterns.len();
        // Pad to a multiple of LANE with sentinel u32::MAX (never matches any
        // real fingerprint, so padded lanes are always rejected by the SIMD
        // compare).
        let pad = (LANE - len % LANE) % LANE;
        ext_fps.extend(std::iter::repeat(u32::MAX).take(pad));
        pfx_fps.extend(std::iter::repeat(u32::MAX).take(pad));

        Ok(Self {
            patterns,
            fns,
            ext_fps,
            pfx_fps,
            fold_case,
            len,
        })
    }

    /// Return the index of the first (highest-priority) pattern that matches
    /// `path`, or `None` if nothing matches.
    ///
    /// Strips leading `./` and `/`; lowercases when compiled case-insensitively.
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

        let path_ext = u32x8::splat(ext_fp(path));
        let path_pfx = u32x8::splat(path_prefix_fp(path));
        let zero = u32x8::splat(0);
        let _sentinel = u32x8::splat(u32::MAX);

        let padded = self.ext_fps.len(); // always a multiple of LANE
        let mut base = 0usize;

        while base < padded {
            let ext_batch = u32x8::from_slice(&self.ext_fps[base..base + LANE]);
            let pfx_batch = u32x8::from_slice(&self.pfx_fps[base..base + LANE]);

            // A pattern is a candidate if:
            //   (ext_fp == path_ext  OR  ext_fp == 0 [unconstrained])
            //   AND
            //   (pfx_fp == path_pfx  OR  pfx_fp == 0 [unconstrained])
            //
            // Padding lanes have fp = u32::MAX which never equals path_ext,
            // path_pfx, or 0, so they are always rejected.
            let ext_ok = ext_batch.simd_eq(path_ext) | ext_batch.simd_eq(zero);
            let pfx_ok = pfx_batch.simd_eq(path_pfx) | pfx_batch.simd_eq(zero);
            let mut mask = (ext_ok & pfx_ok).to_bitmask();

            // Iterate set bits in ascending order (= priority order).
            while mask != 0 {
                let bit = mask.trailing_zeros() as usize;
                mask &= mask - 1; // clear lowest set bit
                let idx = base + bit;
                if idx < self.len && self.check(idx, path) {
                    return Some(idx);
                }
            }

            base += LANE;
        }

        None
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
            MatchFn::Regex => self.patterns[idx].matches_prepared(path),
        }
    }

    /// Linear scan with `MatchFn` dispatch — no SIMD filtering.
    /// Isolates the "avoid regex" contribution without SIMD.
    pub fn first_match_linear_matchfn(&self, path: &str) -> Option<usize> {
        let path = path.strip_prefix("./").unwrap_or(path);
        let path = path.strip_prefix('/').unwrap_or(path);
        let lowered;
        let path: &str = if self.fold_case {
            lowered = path.to_lowercase();
            &lowered
        } else {
            path
        };
        (0..self.len).find(|&i| self.check(i, path))
    }

    /// SIMD fingerprint filtering but always falls back to regex — no MatchFn dispatch.
    /// Isolates the SIMD contribution without regex avoidance.
    pub fn first_match_simd_regex(&self, path: &str) -> Option<usize> {
        let path = path.strip_prefix("./").unwrap_or(path);
        let path = path.strip_prefix('/').unwrap_or(path);
        let lowered;
        let path: &str = if self.fold_case {
            lowered = path.to_lowercase();
            &lowered
        } else {
            path
        };

        let path_ext = u32x8::splat(ext_fp(path));
        let path_pfx = u32x8::splat(path_prefix_fp(path));
        let zero = u32x8::splat(0);
        let padded = self.ext_fps.len();
        let mut base = 0usize;

        while base < padded {
            let ext_batch = u32x8::from_slice(&self.ext_fps[base..base + LANE]);
            let pfx_batch = u32x8::from_slice(&self.pfx_fps[base..base + LANE]);
            let ext_ok = ext_batch.simd_eq(path_ext) | ext_batch.simd_eq(zero);
            let pfx_ok = pfx_batch.simd_eq(path_pfx) | pfx_batch.simd_eq(zero);
            let mut mask = (ext_ok & pfx_ok).to_bitmask();
            while mask != 0 {
                let bit = mask.trailing_zeros() as usize;
                mask &= mask - 1;
                let idx = base + bit;
                if idx < self.len && self.patterns[idx].matches_prepared(path) {
                    return Some(idx);
                }
            }
            base += LANE;
        }
        None
    }

    /// Pure linear regex scan — no SIMD, no MatchFn.  The pre-SIMD baseline.
    pub fn first_match_linear_regex(&self, path: &str) -> Option<usize> {
        let path = path.strip_prefix("./").unwrap_or(path);
        let path = path.strip_prefix('/').unwrap_or(path);
        let lowered;
        let path: &str = if self.fold_case {
            lowered = path.to_lowercase();
            &lowered
        } else {
            path
        };
        (0..self.len).find(|&i| self.patterns[i].matches_prepared(path))
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn pattern(&self, idx: usize) -> &Pattern {
        &self.patterns[idx]
    }
}
