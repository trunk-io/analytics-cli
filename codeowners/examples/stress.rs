use std::io::Cursor;
use std::time::Instant;

use codeowners::{FromReader, GitHubOwners, GitLabOwners, OwnersOfPath};

const RULES: usize = 5_000;
const PATHS: usize = 1_000_000;

fn build_gitlab_codeowners(rule_count: usize) -> String {
    let teams = [
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
    let top_dirs = [
        "src",
        "lib",
        "pkg",
        "services",
        "apps",
        "internal",
        "tools",
        "docs",
        "scripts",
        "config",
        "infra",
        "deploy",
        "test",
        "proto",
        "third_party",
    ];
    let sub_dirs = [
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
    ];
    let extensions = [
        "ts", "tsx", "rs", "go", "py", "rb", "java", "kt", "swift", "md",
    ];

    let mut out = String::from("* @default\n");
    out.push_str("/docs/**/*.md @docs-team\n");
    out.push_str("**/*secret* @security\n");
    out.push_str("**/*.toml @platform\n");
    out.push_str("**/*.yaml @platform\n");

    for i in 0..rule_count {
        let top = top_dirs[i % top_dirs.len()];
        let sub = sub_dirs[(i / top_dirs.len()) % sub_dirs.len()];
        let team = teams[(i / (top_dirs.len() * sub_dirs.len())) % teams.len()];
        let ext = extensions[i % extensions.len()];
        match i % 6 {
            0 => out.push_str(&format!("/{top}/{sub}/ {team}\n")),
            1 => out.push_str(&format!("/{top}/**/*.{ext} {team}\n")),
            2 => out.push_str(&format!("/{top}/{sub}/**/* {team}\n")),
            3 => out.push_str(&format!("{sub}/ {team}\n")),
            4 => out.push_str(&format!("/{top}/{sub}/*.{ext} {team}\n")),
            _ => out.push_str(&format!("**/{sub}/**/*.{ext} {team}\n")),
        }
    }
    out
}

fn build_github_codeowners(rule_count: usize) -> String {
    let teams = [
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
    let top_dirs = [
        "src",
        "lib",
        "pkg",
        "services",
        "apps",
        "internal",
        "tools",
        "docs",
        "scripts",
        "config",
        "infra",
        "deploy",
        "test",
        "proto",
        "third_party",
    ];
    let sub_dirs = [
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
    ];
    let extensions = [
        "ts", "tsx", "rs", "go", "py", "rb", "java", "kt", "swift", "md",
    ];

    let mut out = String::from("* @default\n");
    out.push_str("docs/ @docs-team\n");
    out.push_str("**/*secret* @security\n");
    out.push_str("*.toml @platform\n");
    out.push_str("*.yaml @platform\n");

    for i in 0..rule_count {
        let top = top_dirs[i % top_dirs.len()];
        let sub = sub_dirs[(i / top_dirs.len()) % sub_dirs.len()];
        let team = teams[(i / (top_dirs.len() * sub_dirs.len())) % teams.len()];
        let ext = extensions[i % extensions.len()];
        match i % 5 {
            0 => out.push_str(&format!("{top}/{sub}/ {team}\n")),
            1 => out.push_str(&format!("{top}/**/*.{ext} {team}\n")),
            2 => out.push_str(&format!("{top}/{sub}/ {team}\n")),
            3 => out.push_str(&format!("*.{ext} {team}\n")),
            _ => out.push_str(&format!("{sub}/**/*.{ext} {team}\n")),
        }
    }
    out
}

fn build_paths(path_count: usize) -> Vec<String> {
    let top_dirs = [
        "src",
        "lib",
        "pkg",
        "services",
        "apps",
        "internal",
        "tools",
        "docs",
        "scripts",
        "config",
        "infra",
        "deploy",
        "test",
        "proto",
        "third_party",
    ];
    let sub_dirs = [
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
    ];
    let leaf_names = [
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
    ];
    let extensions = [
        "ts", "tsx", "rs", "go", "py", "rb", "java", "kt", "swift", "md",
    ];

    (0..path_count)
        .map(|i| {
            let top = top_dirs[i % top_dirs.len()];
            let sub = sub_dirs[(i / top_dirs.len()) % sub_dirs.len()];
            let depth = (i % 4) + 1;
            let leaf = leaf_names[i % leaf_names.len()];
            let ext = extensions[i % extensions.len()];
            if depth == 1 {
                format!("/{top}/{leaf}.{ext}")
            } else if depth == 2 {
                format!("/{top}/{sub}/{leaf}.{ext}")
            } else if depth == 3 {
                let mid = sub_dirs[(i / 7) % sub_dirs.len()];
                format!("/{top}/{sub}/{mid}/{leaf}.{ext}")
            } else {
                let mid1 = sub_dirs[(i / 7) % sub_dirs.len()];
                let mid2 = sub_dirs[(i / 11) % sub_dirs.len()];
                format!("/{top}/{sub}/{mid1}/{mid2}/{leaf}.{ext}")
            }
        })
        .collect()
}

fn run<O>(label: &str, owners: &O, paths: &[String])
where
    O: OwnersOfPath,
{
    let t = Instant::now();
    let mut hits = 0usize;
    for path in paths {
        if owners.of(path.as_str()).is_some() {
            hits += 1;
        }
    }
    let elapsed = t.elapsed();
    println!(
        "{label}: {} paths in {:.2?}  ({:.1} µs/path, {} hits)",
        paths.len(),
        elapsed,
        elapsed.as_secs_f64() / paths.len() as f64 * 1e6,
        hits,
    );
}

fn main() {
    let paths = build_paths(PATHS);

    println!("Building CODEOWNERS files ({RULES} generated rules)...");
    let gitlab_text = build_gitlab_codeowners(RULES);
    let github_text = build_github_codeowners(RULES);

    println!("Parsing...");
    let t = Instant::now();
    let gitlab = GitLabOwners::from_reader(Cursor::new(&gitlab_text)).expect("gitlab parse failed");
    println!("  GitLab parsed in {:.2?}", t.elapsed());

    let t = Instant::now();
    let github = GitHubOwners::from_reader(Cursor::new(&github_text)).expect("github parse failed");
    println!("  GitHub parsed in {:.2?}", t.elapsed());

    println!("\nMatching {PATHS} paths...");
    run("  GitLab", &gitlab, &paths);
    run("  GitHub", &github, &paths);
}
