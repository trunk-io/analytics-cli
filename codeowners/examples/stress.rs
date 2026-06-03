//! Stress test for CODEOWNERS matching against realistic and edge-case workloads.
//!
//! Each workload is run through `GitHubOwners::from_reader` (the production
//! `AcMatcher`-backed matcher) and reports parse + per-path match times.
//! Workloads are modeled on the pattern shapes we've observed in real
//! CODEOWNERS files plus a handful of targeted stress cases.

use std::io::Cursor;
use std::time::{Duration, Instant};

use codeowners::{FromReader, GitHubOwners, OwnersOfPath};

const PATHS: usize = 1_000_000;

struct Workload {
    name: &'static str,
    description: &'static str,
    codeowners: String,
    paths: Vec<String>,
}

struct Result {
    workload: &'static str,
    description: &'static str,
    patterns: usize,
    parse: Duration,
    paths: usize,
    matching: Duration,
    hits: usize,
}

fn run(w: Workload) -> Result {
    let pattern_count = w
        .codeowners
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .count();

    let t = Instant::now();
    let owners = GitHubOwners::from_reader(Cursor::new(&w.codeowners)).expect("parse failed");
    let parse = t.elapsed();

    let t = Instant::now();
    let mut hits = 0usize;
    for p in &w.paths {
        if owners.of(p.as_str()).is_some_and(|o| !o.is_empty()) {
            hits += 1;
        }
    }
    let matching = t.elapsed();

    Result {
        workload: w.name,
        description: w.description,
        patterns: pattern_count,
        parse,
        paths: w.paths.len(),
        matching,
        hits,
    }
}

// ── Pattern/path helpers ──────────────────────────────────────────────────────

const TOP_DIRS: &[&str] = &[
    "expense_management",
    "spend",
    "risk",
    "deposits",
    "underwriting",
    "libraries",
    "card_infra",
    "global_financial_services",
    "external_api",
    "accounts",
    "security",
    "rewards",
    "pricing",
    "travel",
    "communications",
    "growth",
    "tools",
    "support_experiences",
    "automation",
    "extensibility",
];

const SUB_DIRS: &[&str] = &[
    "api",
    "core",
    "utils",
    "models",
    "handlers",
    "middleware",
    "auth",
    "db",
    "cache",
    "queue",
    "events",
    "metrics",
    "logging",
    "client",
    "server",
    "service",
    "domain",
    "controllers",
    "repositories",
    "validators",
];

const LEAF_NAMES: &[&str] = &[
    "main",
    "lib",
    "index",
    "mod",
    "handler",
    "service",
    "controller",
    "repository",
    "client",
    "server",
    "config",
    "schema",
    "types",
    "utils",
    "helpers",
    "validators",
    "errors",
    "models",
    "constants",
    "factory",
];

const TEAMS: &[&str] = &[
    "@platform",
    "@frontend",
    "@backend",
    "@ml",
    "@data",
    "@security",
    "@mobile-ios",
    "@mobile-android",
    "@infra",
    "@devex",
];

fn realistic_test_paths(count: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let top = TOP_DIRS[i % TOP_DIRS.len()];
        let s1 = SUB_DIRS[(i / 7) % SUB_DIRS.len()];
        let s2 = SUB_DIRS[(i / 13) % SUB_DIRS.len()];
        let s3 = SUB_DIRS[(i / 17) % SUB_DIRS.len()];
        let leaf = LEAF_NAMES[i % LEAF_NAMES.len()];
        out.push(format!(
            "{top}/src/test/kotlin/brex/{top}/{s1}/{s2}/{s3}/{leaf}Test.kt"
        ));
    }
    out
}

// ── Workload generators ───────────────────────────────────────────────────────

/// Brex-like monorepo: ~3,000 patterns, ~85% anchored literals with implied
/// children, ~13% anchored wildcards with deep prefixes, a sprinkle of
/// unanchored patterns and pure wildcards.
fn brex_like() -> Workload {
    let mut out = String::new();
    out.push_str("* @default\n");
    out.push_str("**/detekt-baseline.xml\n");
    out.push_str("**/*.graphqls @brexhq/graphql-bar-raisers\n");

    // ~4,200 anchored literals (no wildcards), various depths.
    for i in 0..4200 {
        let top = TOP_DIRS[i % TOP_DIRS.len()];
        let s1 = SUB_DIRS[(i / 7) % SUB_DIRS.len()];
        let s2 = SUB_DIRS[(i / 13) % SUB_DIRS.len()];
        let s3 = SUB_DIRS[(i / 23) % SUB_DIRS.len()];
        let team = TEAMS[i % TEAMS.len()];
        let pat = match i % 4 {
            0 => format!("/{top}/{s1} {team}\n"),
            1 => format!("/{top}/{s1}/{s2} {team}\n"),
            2 => format!("/{top}/{s1}/{s2}/{s3}/ {team}\n"),
            _ => format!("/{top}/src/main/kotlin/brex/{top}/{s1}/{s2} {team}\n"),
        };
        out.push_str(&pat);
    }
    // ~700 anchored wildcards with deep prefixes
    for i in 0..700 {
        let top = TOP_DIRS[i % TOP_DIRS.len()];
        let s1 = SUB_DIRS[(i / 7) % SUB_DIRS.len()];
        let team = TEAMS[(i / 5) % TEAMS.len()];
        let pat = match i % 3 {
            0 => format!("/{top}/{s1}/**/*.kt {team}\n"),
            1 => format!("/{top}/{s1}/**/*.{}\n", ["proto", "yaml", "json"][i % 3]),
            _ => format!("/{top}/**/{s1}_*.kt {team}\n"),
        };
        out.push_str(&pat);
    }
    // ~100 unanchored literals
    for i in 0..100 {
        let s1 = SUB_DIRS[i % SUB_DIRS.len()];
        out.push_str(&format!(
            "{s1}/{}/ @brexhq/floating\n",
            LEAF_NAMES[i % LEAF_NAMES.len()]
        ));
    }

    Workload {
        name: "brex_like",
        description: "~5K patterns, mostly anchored literals (Brex monorepo shape)",
        codeowners: out,
        paths: realistic_test_paths(PATHS),
    }
}

/// oscerai-like JS/TS repo: ~100 unanchored directory patterns, no wildcards.
fn oscerai_like() -> Workload {
    let mut out = String::new();
    out.push_str("# integrations\n");
    let comps = [
        "chat",
        "ui",
        "modal",
        "settings",
        "navigation",
        "audio",
        "video",
        "auth",
        "billing",
        "feedback",
        "history",
        "search",
        "notifications",
        "onboarding",
        "design-system",
        "buttons",
        "forms",
        "icons",
        "tooltips",
        "modals",
    ];
    let sub_features = [
        "hooks",
        "components",
        "lib",
        "utils",
        "models",
        "queries",
        "mutations",
        "providers",
        "context",
        "stores",
    ];
    for i in 0..100 {
        let c = comps[i % comps.len()];
        let s = sub_features[(i / 3) % sub_features.len()];
        out.push_str(&format!(
            "src/components/{c}/{s}/ @oscerai/team-{}\n",
            i % 8
        ));
    }

    // Paths look like nested React test files.
    let mut paths = Vec::with_capacity(3000);
    for i in 0..3000 {
        let c = comps[i % comps.len()];
        let s = sub_features[(i / 3) % sub_features.len()];
        let leaf = LEAF_NAMES[i % LEAF_NAMES.len()];
        paths.push(format!("src/components/{c}/{s}/{leaf}.test.tsx"));
    }

    Workload {
        name: "oscerai_like",
        description: "100 unanchored directory patterns (TS/React monorepo shape)",
        codeowners: out,
        paths,
    }
}

/// Pathological case: many patterns with SHORT literals that appear all over
/// in paths.  AC produces tons of false positives → most regex checks fail.
fn pathological_short_literals() -> Workload {
    let mut out = String::new();
    let shorts = [
        "src", "test", "lib", "util", "api", "core", "db", "ops", "cfg", "fmt",
    ];
    // Each combined with a different team to force distinct candidates.
    for (i, &s) in shorts.iter().enumerate() {
        for j in 0..500 {
            out.push_str(&format!("{s}/sub{j}/ @team{i}\n"));
        }
    }
    // A single anchored literal at the end so most paths can hit it.
    out.push_str("/expense_management @default\n");

    Workload {
        name: "pathological_literals",
        description: "Short-literal patterns (`src/`, `test/`, ...) → many AC false positives",
        codeowners: out,
        paths: realistic_test_paths(PATHS),
    }
}

/// Deep paths (12-15 segments) stress the prefix-map walk.
fn deep_paths() -> Workload {
    let inner = brex_like();
    let mut paths = Vec::with_capacity(PATHS);
    for i in 0..PATHS {
        let top = TOP_DIRS[i % TOP_DIRS.len()];
        let mut parts = vec![top.to_string()];
        for d in 0..12 {
            parts.push(SUB_DIRS[(i / (d + 1)) % SUB_DIRS.len()].to_string());
        }
        parts.push(format!("{}Test.kt", LEAF_NAMES[i % LEAF_NAMES.len()]));
        paths.push(parts.join("/"));
    }
    Workload {
        name: "deep_paths",
        description: "Brex patterns × 14-segment paths (stresses prefix walk)",
        codeowners: inner.codeowners,
        paths,
    }
}

/// All paths miss everything — worst case for short-circuiting.
fn many_misses() -> Workload {
    let inner = brex_like();
    let mut paths = Vec::with_capacity(PATHS);
    for i in 0..PATHS {
        // First component is unique gibberish that no pattern matches.
        paths.push(format!(
            "unknown_top_{i}/src/test/kotlin/brex/random/very/deep/leaf.kt"
        ));
    }
    Workload {
        name: "many_misses",
        description: "Brex patterns × paths that match nothing (worst case for short-circuit)",
        codeowners: inner.codeowners,
        paths,
    }
}

/// Wildcard-heavy CODEOWNERS — every pattern needs the regex path.
fn wildcard_heavy() -> Workload {
    let mut out = String::new();
    out.push_str("* @default\n");
    for i in 0..5000 {
        let top = TOP_DIRS[i % TOP_DIRS.len()];
        let s1 = SUB_DIRS[(i / 7) % SUB_DIRS.len()];
        let s2 = SUB_DIRS[(i / 13) % SUB_DIRS.len()];
        let team = TEAMS[i % TEAMS.len()];
        let pat = match i % 4 {
            0 => format!("/{top}/{s1}/**/*.kt {team}\n"),
            1 => format!("/{top}/**/{s1}/**/*.proto {team}\n"),
            2 => format!("/{top}/{s1}/**/{s2}/**/* {team}\n"),
            _ => format!("**/{s1}/**/*.{}\n", ["yaml", "json", "kt"][i % 3]),
        };
        out.push_str(&pat);
    }
    Workload {
        name: "wildcard_heavy",
        description: "5K patterns, every one uses `**` (forces glob fallback)",
        codeowners: out,
        paths: realistic_test_paths(PATHS),
    }
}

/// Mixed casing in patterns and paths — GitHub matches case-insensitively, so
/// the matcher lowercases both sides.  This exercises that path.
fn mixed_case_chaos() -> Workload {
    let mut out = String::new();
    for i in 0..500 {
        let top = TOP_DIRS[i % TOP_DIRS.len()];
        // Random uppercase mangling.
        let mangled: String = top
            .chars()
            .enumerate()
            .map(|(j, c)| {
                if (i + j) % 3 == 0 {
                    c.to_ascii_uppercase()
                } else {
                    c
                }
            })
            .collect();
        out.push_str(&format!(
            "/{mangled}/src/MAIN/Kotlin/Brex {}\n",
            TEAMS[i % TEAMS.len()]
        ));
    }
    let mut paths = Vec::with_capacity(PATHS);
    for i in 0..PATHS {
        let top = TOP_DIRS[i % TOP_DIRS.len()];
        let s1 = SUB_DIRS[(i / 7) % SUB_DIRS.len()];
        let mangled: String = top
            .chars()
            .enumerate()
            .map(|(j, c)| {
                if (i + j) % 5 == 0 {
                    c.to_ascii_uppercase()
                } else {
                    c
                }
            })
            .collect();
        paths.push(format!("{mangled}/src/main/kotlin/brex/{s1}/Foo.kt"));
    }
    Workload {
        name: "case_insensitive",
        description: "Mangled-case patterns + paths (exercises lowercasing path)",
        codeowners: out,
        paths,
    }
}

/// Overlapping anchors: many patterns at every depth under the same prefix.
/// Stress for the prefix-map (longest-prefix walk gathers many candidates).
fn overlapping_anchors() -> Workload {
    let mut out = String::new();
    // Lots of overlapping ownership at different depths.  Each (top, s1) pair
    // owns at six different depths → the prefix-walk gathers many candidates
    // per path.  Patterns are unique because the depth-suffix varies.
    for i in 0..5000 {
        let top = TOP_DIRS[i % TOP_DIRS.len()];
        let s1 = SUB_DIRS[(i / 7) % SUB_DIRS.len()];
        let depth = i % 6;
        let base = match depth {
            0 => format!("/{top}/{s1}/"),
            1 => format!("/{top}/{s1}/api/"),
            2 => format!("/{top}/{s1}/api/v1/"),
            3 => format!("/{top}/{s1}/api/v1/handlers/"),
            4 => format!("/{top}/{s1}/api/v1/handlers/internal/"),
            _ => format!("/{top}/{s1}/api/v1/handlers/internal/admin/"),
        };
        out.push_str(&format!("{base} @owner-depth-{depth}\n"));
    }
    // Paths that drive into the overlap zones so something actually matches.
    let mut paths = Vec::with_capacity(PATHS);
    for i in 0..PATHS {
        let top = TOP_DIRS[i % TOP_DIRS.len()];
        let s1 = SUB_DIRS[(i / 7) % SUB_DIRS.len()];
        let leaf = LEAF_NAMES[i % LEAF_NAMES.len()];
        paths.push(format!(
            "{top}/{s1}/api/v1/handlers/internal/admin/{leaf}.kt"
        ));
    }
    Workload {
        name: "overlapping_anchors",
        description: "Many anchored patterns at every depth of the same prefix",
        codeowners: out,
        paths,
    }
}

// ── Reporting ─────────────────────────────────────────────────────────────────

fn format_duration(d: Duration) -> String {
    if d.as_secs_f64() >= 1.0 {
        format!("{:.2}s", d.as_secs_f64())
    } else if d.as_secs_f64() >= 0.001 {
        format!("{:.1}ms", d.as_secs_f64() * 1000.0)
    } else {
        format!("{:.1}µs", d.as_secs_f64() * 1e6)
    }
}

fn print_table(rows: &[Result]) {
    println!(
        "\n{:<22} {:>8} {:>9} {:>9} {:>9} {:>10} {:>9}",
        "Workload", "patterns", "parse", "paths", "match", "µs/path", "hit %"
    );
    println!("{}", "─".repeat(86));
    for r in rows {
        let n = r.paths.max(1) as f64;
        let us = r.matching.as_secs_f64() / n * 1e6;
        let hit_pct = r.hits as f64 / n * 100.0;
        println!(
            "{:<22} {:>8} {:>9} {:>9} {:>9} {:>10.2} {:>8.1}%",
            r.workload,
            r.patterns,
            format_duration(r.parse),
            r.paths,
            format_duration(r.matching),
            us,
            hit_pct,
        );
    }
    println!();
    // Highlight outliers
    if let (Some(slowest_parse), Some(slowest_match)) = (
        rows.iter().max_by_key(|r| r.parse),
        rows.iter()
            .max_by_key(|r| (r.matching.as_secs_f64() / r.paths.max(1) as f64 * 1e6) as u64),
    ) {
        println!(
            "Slowest parse:  {}  ({})",
            slowest_parse.workload,
            format_duration(slowest_parse.parse)
        );
        let us = slowest_match.matching.as_secs_f64() / slowest_match.paths.max(1) as f64 * 1e6;
        println!(
            "Slowest match:  {}  ({:.2} µs/path)",
            slowest_match.workload, us
        );
    }
}

fn main() {
    let workloads: Vec<fn() -> Workload> = vec![
        brex_like,
        oscerai_like,
        pathological_short_literals,
        deep_paths,
        many_misses,
        wildcard_heavy,
        mixed_case_chaos,
        overlapping_anchors,
    ];

    println!("CODEOWNERS stress test (AcMatcher via GitHubOwners)");
    println!("─────────────────────────────────────────────────────────────");

    let mut rows = Vec::new();
    for build in workloads {
        let w = build();
        let name = w.name;
        let desc = w.description;
        print!("Running {name:<22} ... ");
        let r = run(w);
        let us = r.matching.as_secs_f64() / r.paths.max(1) as f64 * 1e6;
        println!(
            "parse {}  match {:.2} µs/path  ({} hits / {} paths)",
            format_duration(r.parse),
            us,
            r.hits,
            r.paths,
        );
        rows.push(Result {
            description: desc,
            workload: name,
            ..r
        });
    }

    print_table(&rows);

    println!("\nWorkload descriptions:");
    for r in &rows {
        println!("  {:<22} — {}", r.workload, r.description);
    }
}
