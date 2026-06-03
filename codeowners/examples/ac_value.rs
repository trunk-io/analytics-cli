//! Measures the contribution of the AC pre-filter to overall match speed.
//!
//! For each CODEOWNERS file we build one `AcMatcher` and run the same paths
//! through three variants:
//!
//! * `full`        — prefix-map + AC + always-candidates (production path)
//! * `no_ac`       — prefix-map + linear scan of the AC-routed patterns
//! * `linear`      — no indexes at all; linear scan of every pattern
//!
//! If `no_ac` is close to `full`, AC isn't pulling its weight.

use std::{
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    time::Instant,
};

use codeowners::{FromReader, GitHubOwners};
use owners_glob::{PatternOptions, acmatcher::AcMatcher};

fn build_ac_matcher(codeowners_path: &PathBuf) -> AcMatcher {
    // Re-parse the file and build a matcher directly so we can call the
    // benchmark-only variants on it.
    let bytes = fs::read(codeowners_path).expect("read");
    let mut entries: Vec<String> = Vec::new();
    for line in BufReader::new(&bytes[..]).lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(first) = trimmed.split_whitespace().next() {
            entries.push(first.to_owned());
        }
    }
    // Reverse: last in file wins → index 0 highest priority.
    entries.reverse();
    let raw: Vec<&str> = entries.iter().map(String::as_str).collect();
    AcMatcher::new(raw, PatternOptions::default()).expect("build matcher")
}

fn time<F: Fn() -> usize>(label: &str, paths: usize, f: F) -> std::time::Duration {
    let t = Instant::now();
    let hits = f();
    let elapsed = t.elapsed();
    let us = elapsed.as_secs_f64() / paths as f64 * 1e6;
    println!(
        "  {label:<10}  {:>10.2?}  ({us:.2} µs/path, {hits} hits)",
        elapsed
    );
    elapsed
}

fn bench_file(codeowners_path: PathBuf, paths: &[String]) {
    let label = codeowners_path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| codeowners_path.display().to_string());
    println!("\n=== {label} ===");

    // Just so we surface obvious bugs.
    let _ = GitHubOwners::from_reader(BufReader::new(fs::File::open(&codeowners_path).unwrap()))
        .expect("parse");

    let m = build_ac_matcher(&codeowners_path);
    println!("  patterns: {}", m.len());
    let n = paths.len();

    let full = time("full", n, || {
        paths.iter().filter(|p| m.first_match(p).is_some()).count()
    });
    let no_ac = time("no_ac", n, || {
        paths
            .iter()
            .filter(|p| m.first_match_no_ac(p).is_some())
            .count()
    });
    let linear = time("linear", n, || {
        paths
            .iter()
            .filter(|p| m.first_match_linear(p).is_some())
            .count()
    });

    let ratio_no_ac = no_ac.as_secs_f64() / full.as_secs_f64();
    let ratio_linear = linear.as_secs_f64() / full.as_secs_f64();
    println!(
        "  → no_ac is {:.2}× slower; linear is {:.2}× slower than full",
        ratio_no_ac, ratio_linear
    );
}

fn main() {
    // Brex dump: pick the first CODEOWNERS file (they're all near-identical)
    let brex = PathBuf::from(
        "/Users/dylan/Downloads/dump/0084ad41-77b8-4fcc-81af-bca9056ff28e/CODEOWNERS",
    );
    let brex_paths: Vec<String> = fs::read_to_string("/Users/dylan/Downloads/dump/test_paths.txt")
        .expect("read brex paths")
        .lines()
        .map(str::to_owned)
        .collect();
    bench_file(brex, &brex_paths);

    let oscerai = PathBuf::from(
        "/Users/dylan/Downloads/dump 2/9c0aec19-7fba-4cea-872c-29ce085ed23e/CODEOWNERS",
    );
    let oscerai_paths: Vec<String> = fs::read_to_string(
        "/Users/dylan/Downloads/dump 2/9c0aec19-7fba-4cea-872c-29ce085ed23e/test_paths.txt",
    )
    .expect("read oscerai paths")
    .lines()
    .map(str::to_owned)
    .collect();
    bench_file(oscerai, &oscerai_paths);
}
