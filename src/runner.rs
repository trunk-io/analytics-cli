use crate::constants::EXIT_FAILURE;
use crate::scanner::{BundleRepo, FileSet, FileSetCounter};
use crate::types::{RunResult, Test};
use junit_parser;
use std::fs::metadata;
use std::process::Command;
use std::process::Stdio;
use std::time::SystemTime;

pub async fn run_test_command(
    repo: &BundleRepo,
    command: &String,
    args: Vec<&String>,
    output_paths: Vec<&String>,
) -> anyhow::Result<RunResult> {
    let start = SystemTime::now();
    let mut child = Command::new(command)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let result = match child.wait() {
        Ok(result) => result,
        Err(e) => {
            log::error!("Error waiting for execution: {}", e);
            return Ok(RunResult {
                exit_code: EXIT_FAILURE,
                failures: Vec::new(),
            });
        }
    };
    let mut failures = Vec::<Test>::new();
    if !result.success() {
        let mut file_counter = FileSetCounter::default();
        let file_sets = match output_paths
            .iter()
            .map(|path| {
                FileSet::scan_from_glob(&repo.repo_root, path.to_string(), &mut file_counter)
            })
            .collect::<anyhow::Result<Vec<FileSet>>>()
        {
            Ok(file_sets) => file_sets,
            Err(e) => {
                log::error!("Error scanning file sets: {}", e);
                return Ok(RunResult {
                    exit_code: result.code().unwrap_or(EXIT_FAILURE),
                    failures,
                });
            }
        };
        for file_set in &file_sets {
            for file in &file_set.files {
                log::info!("Checking file: {}", file.original_path);
                let metadata = match metadata(&file.original_path) {
                    Ok(metadata) => metadata,
                    Err(e) => {
                        log::warn!("Error getting metadata: {}", e);
                        continue;
                    }
                };
                let time = match metadata.modified() {
                    Ok(time) => time,
                    Err(e) => {
                        log::warn!("Error getting modified time: {}", e);
                        continue;
                    }
                };
                // skip files that were last modified before the test started
                if time <= start {
                    log::info!(
                        "Skipping file because of lack of modification: {}",
                        file.original_path
                    );
                    continue;
                }
                let file = match std::fs::File::open(&file.original_path) {
                    Ok(file) => file,
                    Err(e) => {
                        log::warn!("Error opening file: {}", e);
                        continue;
                    }
                };
                let reader = std::io::BufReader::new(file);
                let junitxml = match junit_parser::from_reader(reader) {
                    Ok(junitxml) => junitxml,
                    Err(e) => {
                        log::warn!("Error parsing junitxml: {}", e);
                        continue;
                    }
                };
                for suite in junitxml.suites {
                    let parent_name = if junitxml.name.is_empty() {
                        suite.name
                    } else {
                        format!("{}/{}", junitxml.name, suite.name)
                    };
                    for case in suite.cases {
                        let failure = case.status.is_failure();
                        if failure {
                            let name = case.original_name;
                            log::debug!("Test failed: {} -> {}", parent_name, name);
                            failures.push(Test {
                                parent_name: parent_name.clone(),
                                name: name.clone(),
                            });
                        }
                    }
                }
            }
        }
    }
    let exit_code = result.code().unwrap_or(EXIT_FAILURE);
    Ok(RunResult {
        exit_code,
        failures,
    })
}
