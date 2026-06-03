//! Apples-to-apples comparison: build cost and per-match cost for the three
//! glob backends we've used or considered:
//!
//! * `fast_glob::glob_match`  — no compile step, char-by-char matcher
//! * `glob::Pattern`          — popular crate, compiles a small AST
//! * `regex::Regex`           — what we used until this change
//!
//! Patterns and paths come from the real Brex CODEOWNERS dump so the numbers
//! reflect realistic shapes.

use std::{fs, io::BufRead, path::PathBuf, time::Instant};

fn normalize_for_glob(raw: &str) -> String {
    // Mirror the GitHub `**/`-prefixing rule for unanchored patterns and the
    // trailing-`/`  → `/**` expansion, so all three engines see the same
    // canonical glob.
    let anchored = raw.starts_with('/');
    let body = if anchored { &raw[1..] } else { raw };
    let inner = body.trim_end_matches('/');
    let add_prefix =
        !anchored && !inner.starts_with("**") && !(inner.starts_with('*') && !inner.contains('/'));
    let mut s = if add_prefix {
        format!("**/{}", body)
    } else {
        body.to_owned()
    };
    if s.ends_with('/') {
        s.push_str("**");
    }
    s
}

fn to_regex(glob: &str) -> String {
    // Cheap glob → regex translator just for the benchmark; production code
    // lives in `owners_glob::to_regex`.
    let mut out = String::from("^");
    let b = glob.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i < n {
        if b[i] == b'*' && i + 1 < n && b[i + 1] == b'*' {
            out.push_str(".*");
            i += 2;
        } else if b[i] == b'*' {
            out.push_str("[^/]*");
            i += 1;
        } else if b[i] == b'?' {
            out.push_str("[^/]");
            i += 1;
        } else {
            let c = b[i] as char;
            if matches!(
                c,
                '.' | '+' | '(' | ')' | '{' | '}' | '^' | '$' | '|' | '\\'
            ) {
                out.push('\\');
            }
            out.push(c);
            i += 1;
        }
    }
    out.push('$');
    out
}

fn main() {
    let codeowners_path = std::env::args().nth(1).unwrap_or_else(|| {
        "/Users/dylan/Downloads/dump/0084ad41-77b8-4fcc-81af-bca9056ff28e/CODEOWNERS".into()
    });
    let paths_path = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "/Users/dylan/Downloads/dump/test_paths.txt".into());

    let raw: Vec<String> = fs::read(PathBuf::from(&codeowners_path))
        .expect("read codeowners")
        .lines()
        .filter_map(Result::ok)
        .filter_map(|l| {
            let trimmed = l.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            trimmed.split_whitespace().next().map(str::to_owned)
        })
        .collect();
    let patterns: Vec<String> = raw.iter().map(|r| normalize_for_glob(r)).collect();

    let paths: Vec<String> = fs::read_to_string(PathBuf::from(&paths_path))
        .expect("read paths")
        .lines()
        .map(str::to_owned)
        .collect();

    println!("Patterns: {}", patterns.len());
    println!("Paths:    {}", paths.len());
    println!();

    // Skip the catch-all `*` (matches everything; would short-circuit the loop).
    // Also keep only patterns with wildcards — the cases each backend actually
    // has to *think* about.  Literal patterns hit string-shape fast paths in
    // production and shouldn't be in this comparison.
    let benched_patterns: Vec<String> = patterns
        .iter()
        .filter(|p| p.contains('*') || p.contains('?') || p.contains('['))
        .filter(|p| p.as_str() != "**" && p.as_str() != "**/*" && p.as_str() != "*")
        .cloned()
        .collect();
    println!("Wildcard patterns benched: {}", benched_patterns.len());
    println!("(measuring total work — every path checked against every pattern)\n");

    // ── fast_glob (no build step) ─────────────────────────────────────────────
    let t = Instant::now();
    let fg_patterns: Vec<&str> = benched_patterns.iter().map(String::as_str).collect();
    let fg_build = t.elapsed();
    let t = Instant::now();
    let mut hits = 0usize;
    for path in &paths {
        for pat in &fg_patterns {
            if fast_glob::glob_match(pat, path) {
                hits += 1;
            }
        }
    }
    let fg_match = t.elapsed();

    // ── glob crate (compiles a Pattern) ───────────────────────────────────────
    let t = Instant::now();
    let glob_patterns: Vec<glob::Pattern> = benched_patterns
        .iter()
        .filter_map(|p| glob::Pattern::new(p).ok())
        .collect();
    let glob_build = t.elapsed();
    let t = Instant::now();
    let mut hits_g = 0usize;
    for path in &paths {
        for pat in &glob_patterns {
            if pat.matches(path) {
                hits_g += 1;
            }
        }
    }
    let glob_match = t.elapsed();

    // ── regex (compiles a DFA per pattern) ────────────────────────────────────
    let t = Instant::now();
    let regex_patterns: Vec<regex::Regex> = benched_patterns
        .iter()
        .filter_map(|p| regex::Regex::new(&to_regex(p)).ok())
        .collect();
    let regex_build = t.elapsed();
    let t = Instant::now();
    let mut hits_r = 0usize;
    for path in &paths {
        for pat in &regex_patterns {
            if pat.is_match(path) {
                hits_r += 1;
            }
        }
    }
    let regex_match = t.elapsed();

    let n = paths.len() as f64;
    let total_checks = (paths.len() * benched_patterns.len()) as f64;
    println!(
        "{:<14} {:>12} {:>14} {:>14} {:>14} {:>10}",
        "Backend", "build", "match (total)", "µs/path", "ns/check", "hits"
    );
    println!("{}", "─".repeat(84));
    for (name, build, m, hits) in [
        ("fast_glob", fg_build, fg_match, hits),
        ("glob", glob_build, glob_match, hits_g),
        ("regex", regex_build, regex_match, hits_r),
    ] {
        println!(
            "{:<14} {:>12} {:>14} {:>14.2} {:>14.2} {:>10}",
            name,
            format!("{:.2?}", build),
            format!("{:.2?}", m),
            m.as_secs_f64() / n * 1e6,
            m.as_secs_f64() / total_checks * 1e9,
            hits,
        );
    }
}
