use std::io::Cursor;

use codeowners::{FromReader, GitLabOwners, OwnersOfPath};
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

fn build_codeowners(rule_count: usize) -> String {
    // Real-world-ish teams and dirs drawn from typical monorepo layouts.
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

    let mut out = String::from("# Auto-generated stress CODEOWNERS\n\n");
    out.push_str("* @default\n\n");

    out.push_str("[Docs]\n");
    out.push_str("/docs/ @docs-team\n");
    out.push_str("/docs/**/*.md @docs-team @technical-writers\n");
    out.push_str("README.md @docs-team\n\n");

    out.push_str("[Security]\n");
    out.push_str("**/*secret* @security\n");
    out.push_str("**/*token* @security\n");
    out.push_str("**/*password* @security\n");
    out.push_str("**/*credential* @security\n");
    out.push_str("/infra/secrets/** @security @platform\n");
    out.push_str("*.pem @security\n");
    out.push_str("*.key @security\n\n");

    out.push_str("[Config]\n");
    out.push_str("/config/ @platform\n");
    out.push_str("**/*.toml @platform\n");
    out.push_str("**/*.yaml @platform\n");
    out.push_str("**/*.yml @platform\n");
    out.push_str("**/*.json @platform\n\n");

    // Generate N additional rules spread across dirs/teams/patterns.
    for rule_idx in 0..rule_count {
        let top = top_dirs[rule_idx % top_dirs.len()];
        let sub = sub_dirs[(rule_idx / top_dirs.len()) % sub_dirs.len()];
        let team = teams[(rule_idx / (top_dirs.len() * sub_dirs.len())) % teams.len()];
        let ext = extensions[rule_idx % extensions.len()];

        match rule_idx % 6 {
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

fn bench_matching(c: &mut Criterion) {
    let rule_counts = [50usize, 200, 500, 2000];
    let path_count = 500;

    let mut group = c.benchmark_group("gitlab/entries_for_path");

    for &rules in &rule_counts {
        let codeowners_text = build_codeowners(rules);
        let paths = build_paths(path_count);

        let owners = GitLabOwners::from_reader(Cursor::new(&codeowners_text))
            .expect("generated CODEOWNERS failed to parse");

        group.throughput(Throughput::Elements(path_count as u64));
        group.bench_with_input(BenchmarkId::new("rules", rules), &paths, |b, paths| {
            b.iter(|| {
                for path in paths {
                    black_box(owners.of(black_box(path.as_str())));
                }
            });
        });
    }

    group.finish();
}

fn bench_parse(c: &mut Criterion) {
    let rule_counts = [50usize, 200, 500, 2000];

    let mut group = c.benchmark_group("gitlab/parse");

    for &rules in &rule_counts {
        let codeowners_text = build_codeowners(rules);

        group.throughput(Throughput::Elements(rules as u64));
        group.bench_with_input(
            BenchmarkId::new("rules", rules),
            &codeowners_text,
            |b, text| {
                b.iter(|| {
                    black_box(
                        GitLabOwners::from_reader(Cursor::new(black_box(text.as_str())))
                            .expect("parse failed"),
                    );
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_matching, bench_parse);
criterion_main!(benches);
