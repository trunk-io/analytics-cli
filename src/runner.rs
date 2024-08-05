use crate::constants::{EXIT_FAILURE, EXIT_SUCCESS};
use crate::scanner::{BundleRepo, FileSet, FileSetCounter};
use crate::types::{QuarantineRunResult, RunResult, Test};
use junit_parser;
use std::fs::metadata;
use std::process::Command;
use std::process::Stdio;
use std::time::SystemTime;
use crate::types::QuarantineBulkTestStatus;
use tokio_retry::Retry;
use tokio_retry::strategy::ExponentialBackoff;
use crate::clients::get_quarantine_bulk_test_status;

pub async fn run_test_command(
    repo: &BundleRepo,
    command: &String,
    args: Vec<&String>,
    output_paths: Vec<&String>,
    team: Option<String>,
    codeowners_path: Option<String>,
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
                FileSet::scan_from_glob(
                    &repo.repo_root,
                    path.to_string(),
                    &mut file_counter,
                    team.clone(),
                    codeowners_path.clone(),
                )
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
                    let parent_name = suite.name;
                    for case in suite.cases {
                        let failure = case.status.is_failure();
                        if failure {
                            let name = case.original_name;
                            log::debug!("Test failed: {} -> {}", parent_name, name);
                            failures.push(Test {
                                parent_name: parent_name.clone(),
                                name: name.clone(),
                                class_name: case.classname.clone(),
                                file: case.file.clone(),
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

pub async fn run_quarantine(
    run_result: &RunResult,
    api_address: &str,
    token: &str,
    org_url_slug: &str,
    repo: &BundleRepo,
    delay: std::iter::Take<ExponentialBackoff>,
) -> anyhow::Result<QuarantineRunResult> {
    // check with the API if the group is quarantined
    let quarantine_results = if run_result.failures.is_empty() {
        QuarantineBulkTestStatus {
            group_is_quarantined: false,
            quarantine_results: Vec::new(),
        }
    } else {
        match Retry::spawn(delay, || {
            get_quarantine_bulk_test_status(
                api_address,
                token,
                org_url_slug,
                &repo.repo,
                &run_result.failures,
            )
        })
        .await
        {
            Ok(quarantine_results) => quarantine_results,
            Err(e) => {
                log::error!("Failed to get quarantine results: {:?}", e);
                QuarantineBulkTestStatus {
                    group_is_quarantined: false,
                    quarantine_results: Vec::new(),
                }
            }
        }
    };
    log::info!("Quarantine results: {:?}", quarantine_results);
    // use the exit code from the command if the group is not quarantined
    // override exit code to be exit_success if the group is quarantined
    let exit_code = if !quarantine_results.group_is_quarantined {
        log::info!("Not all test failures were quarantined, returning exit code from command.");
        run_result.exit_code
    } else if run_result.exit_code != EXIT_SUCCESS {
        log::info!("All test failures were quarantined, overriding exit code to be exit_success");
        EXIT_SUCCESS
    } else {
        run_result.exit_code
    };

    Ok(QuarantineRunResult {
        exit_code,
        quarantine_status: quarantine_results,
    })
}
