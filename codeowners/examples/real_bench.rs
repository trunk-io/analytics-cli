/// Benchmark GitHubOwners matching against real CODEOWNERS files and real
/// test-case paths extracted from trunk upload bundles.
///
/// Usage:
///   cargo run --example real_bench --release -- <codeowners_file> <paths_file>
///
/// Or to run against every CODEOWNERS in a dump directory:
///   cargo run --example real_bench --release -- --dump-dir <dir> <paths_file>
use std::{fs, io::BufReader, path::PathBuf, time::Instant};

use codeowners::{FromReader, GitHubOwners, OwnersOfPath};

fn bench_one(label: &str, codeowners_path: &str, paths: &[String]) {
    // Parse
    let t = Instant::now();
    let f = fs::File::open(codeowners_path).expect("open CODEOWNERS");
    let owners = GitHubOwners::from_reader(BufReader::new(f)).expect("parse CODEOWNERS");
    let parse_ms = t.elapsed().as_secs_f64() * 1000.0;

    // Match
    let t = Instant::now();
    let mut hits = 0usize;
    let mut misses = 0usize;
    for path in paths {
        match owners.of(path.as_str()) {
            Some(o) if !o.is_empty() => hits += 1,
            _ => misses += 1,
        }
    }
    let elapsed = t.elapsed();
    let us_per_path = elapsed.as_secs_f64() / paths.len() as f64 * 1e6;

    println!(
        "{label:50}  parse {parse_ms:6.1}ms  match {:.2?}  ({:.2} µs/path)  hits={hits} misses={misses}",
        elapsed, us_per_path,
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Simple arg parsing: --dump-dir <dir> <paths_file>  OR  <codeowners> <paths_file>
    let (codeowners_files, paths_file): (Vec<PathBuf>, PathBuf) =
        if args.len() >= 4 && args[1] == "--dump-dir" {
            let dump_dir = PathBuf::from(&args[2]);
            let pf = PathBuf::from(&args[3]);
            let mut files: Vec<PathBuf> = fs::read_dir(&dump_dir)
                .expect("read dump dir")
                .filter_map(|e| e.ok())
                .map(|e| e.path().join("CODEOWNERS"))
                .filter(|p| p.exists())
                .collect();
            files.sort();
            (files, pf)
        } else if args.len() >= 3 {
            (vec![PathBuf::from(&args[1])], PathBuf::from(&args[2]))
        } else {
            eprintln!("Usage:");
            eprintln!("  real_bench <codeowners_file> <paths_file>");
            eprintln!("  real_bench --dump-dir <dir> <paths_file>");
            std::process::exit(1);
        };

    // Load paths once
    let paths_raw = fs::read_to_string(&paths_file).expect("read paths file");
    let paths: Vec<String> = paths_raw.lines().map(str::to_owned).collect();
    println!("Loaded {} paths from {}", paths.len(), paths_file.display());
    println!("Running {} CODEOWNERS file(s)\n", codeowners_files.len());

    if codeowners_files.len() == 1 {
        bench_one(
            codeowners_files[0].to_str().unwrap(),
            codeowners_files[0].to_str().unwrap(),
            &paths,
        );
    } else {
        // Run each file individually, then aggregate
        let mut total_parse_ms = 0f64;
        let mut total_match_ns = 0u128;
        let mut total_hits = 0usize;
        let mut total_paths = 0usize;

        for co in &codeowners_files {
            let label = co.parent().unwrap().file_name().unwrap().to_string_lossy();

            let t = Instant::now();
            let f = fs::File::open(co).expect("open CODEOWNERS");
            let owners = GitHubOwners::from_reader(BufReader::new(f)).expect("parse");
            let parse_ms = t.elapsed().as_secs_f64() * 1000.0;
            total_parse_ms += parse_ms;

            let t = Instant::now();
            let mut hits = 0usize;
            let mut misses = 0usize;
            for path in &paths {
                match owners.of(path.as_str()) {
                    Some(o) if !o.is_empty() => hits += 1,
                    _ => misses += 1,
                }
            }
            let elapsed = t.elapsed();
            total_match_ns += elapsed.as_nanos();
            total_hits += hits;
            total_paths += paths.len();

            let us_per_path = elapsed.as_secs_f64() / paths.len() as f64 * 1e6;
            println!(
                "  {label:40}  parse {parse_ms:5.1}ms  match {:.2?}  ({:.2} µs/path)  hits={hits} misses={misses}",
                elapsed, us_per_path,
            );
        }

        let avg_parse = total_parse_ms / codeowners_files.len() as f64;
        let avg_match_us = total_match_ns as f64 / 1000.0 / total_paths as f64;
        let avg_match_total = std::time::Duration::from_nanos(
            (total_match_ns / codeowners_files.len() as u128) as u64,
        );
        println!(
            "\n  {:40}  parse {:5.1}ms  match {:.2?}  ({:.2} µs/path)  hits={} misses={}",
            "AVERAGE",
            avg_parse,
            avg_match_total,
            avg_match_us,
            total_hits / codeowners_files.len(),
            (total_paths - total_hits) / codeowners_files.len(),
        );
    }
}
