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
    output_paths: Vec<String>,
    team: Option<String>,
    codeowners_path: Option<String>,
) -> anyhow::Result<RunResult> {
    let start = SystemTime::now();
    let exit_code = run_test_and_get_exit_code(command, args).await?;
    log::info!("Command exit code: {}", exit_code);
    let (file_sets, _file_counter) = get_files(repo, output_paths, team.clone(), codeowners_path.clone())?;
    let failures = if exit_code != EXIT_SUCCESS {
        get_failures(file_sets, start).await?
    } else {
        Vec::new()
    };
    Ok(RunResult {
        exit_code,
        failures,
    })
}

async fn run_test_and_get_exit_code(command: &String, args: Vec<&String>) -> anyhow::Result<i32> {
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
            return Ok(EXIT_FAILURE);
        }
    };
    Ok(result.code().unwrap_or(EXIT_FAILURE))
}

pub fn get_files(
    repo: &BundleRepo,
    junit_paths: Vec<String>,
    team: Option<String>,
    codeowners_path: Option<String>,
) -> anyhow::Result<(Vec<FileSet>, FileSetCounter)>  {
    let mut file_counter = FileSetCounter::default();
    let mut file_sets = junit_paths
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
        .collect::<anyhow::Result<Vec<FileSet>>>()?;

    // Handle case when junit paths are not globs.
    if file_counter.get_count() == 0 {
        file_sets = junit_paths
            .iter()
            .map(|path| {
                let mut path = path.clone();
                if !path.ends_with("/") {
                    path.push_str("/");
                }
                path.push_str("**/*.xml");
                FileSet::scan_from_glob(
                    &repo.repo_root,
                    path.to_string(),
                    &mut file_counter,
                    team.clone(),
                    codeowners_path.clone(),
                )
            })
            .collect::<anyhow::Result<Vec<FileSet>>>()?;
    }

    Ok((file_sets, file_counter))
}

pub async fn get_failures(
    file_sets: Vec<FileSet>,
    start: SystemTime,
) -> anyhow::Result<Vec<Test>> {
    let mut failures = Vec::<Test>::new();
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
    Ok(failures)
}

pub async fn run_quarantine(
    run_result: &RunResult,
    api_address: &str,
    token: &str,
    org_url_slug: &str,
    repo: &BundleRepo,
    delay: std::iter::Take<ExponentialBackoff>,
    no_quarantining: &bool,
) -> anyhow::Result<QuarantineRunResult> {
        if *no_quarantining {
            log::info!("Skipping quarantining step.");
            return Ok(QuarantineRunResult {
                exit_code: run_result.exit_code,
                quarantine_status: QuarantineBulkTestStatus {
                    group_is_quarantined: false,
                    quarantine_results: Vec::new(),
                },
            });
        }
    // check with the API if the group is quarantined
    log::info!("Checking quarantine status for failures: {:?}", run_result.failures);
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
