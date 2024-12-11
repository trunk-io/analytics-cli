use quick_junit::TestCaseStatus;
use std::process::{Command, Stdio};
use std::time::SystemTime;

use api;
use bundle::{
    FileSet, FileSetCounter, QuarantineBulkTestStatus, QuarantineRunResult, RunResult, Test,
};
use codeowners::CodeOwners;
use constants::{EXIT_FAILURE, EXIT_SUCCESS};
use context::{bazel_bep::parser::BazelBepParser, junit::parser::JunitParser, repo::BundleRepo};

use crate::{api_client::ApiClient, print::print_bep_results};

pub enum JunitSpec {
    Paths(Vec<String>),
    BazelBep(String),
}

pub async fn run_test_command(
    repo: &BundleRepo,
    org_slug: &str,
    command: &String,
    args: Vec<&String>,
    junit_spec: JunitSpec,
    team: Option<String>,
    codeowners: &Option<CodeOwners>,
) -> anyhow::Result<RunResult> {
    let start = SystemTime::now();
    let exit_code = run_test_and_get_exit_code(command, args).await?;
    log::info!("Command exit code: {}", exit_code);

    let output_paths = match junit_spec {
        JunitSpec::Paths(paths) => paths,
        JunitSpec::BazelBep(bep_path) => {
            let mut parser = BazelBepParser::new(bep_path);
            parser.parse()?;
            print_bep_results(&parser);
            parser.uncached_xml_files()
        }
    };

    let (file_sets, ..) = build_filesets(
        &repo.repo_root,
        &output_paths,
        team,
        codeowners,
        Some(start),
    )?;
    let failures = if exit_code != EXIT_SUCCESS {
        extract_failed_tests(repo, org_slug, &file_sets).await
    } else {
        Vec::new()
    };
    if failures.is_empty() && exit_code != EXIT_SUCCESS {
        log::warn!("Command failed but no test failures were found!");
    }
    Ok(RunResult {
        exit_code,
        failures,
        exec_start: Some(start),
    })
}

async fn run_test_and_get_exit_code(command: &String, args: Vec<&String>) -> anyhow::Result<i32> {
    let mut child = Command::new(command)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    let result = child
        .wait()
        .map_or_else(
            |e| {
                log::error!("Error waiting for execution: {}", e);
                None
            },
            |exit_status| exit_status.code(),
        )
        .unwrap_or(EXIT_FAILURE);

    Ok(result)
}

pub fn build_filesets(
    repo_root: &str,
    junit_paths: &[String],
    team: Option<String>,
    codeowners: &Option<CodeOwners>,
    exec_start: Option<SystemTime>,
) -> anyhow::Result<(Vec<FileSet>, FileSetCounter)> {
    let mut file_counter = FileSetCounter::default();
    let mut file_sets = junit_paths
        .iter()
        .map(|path| {
            FileSet::scan_from_glob(
                repo_root,
                path.to_string(),
                &mut file_counter,
                team.clone(),
                codeowners,
                exec_start,
            )
        })
        .collect::<anyhow::Result<Vec<FileSet>>>()?;

    // Handle case when junit paths are not globs.
    if file_counter.get_count() == 0 {
        file_sets = junit_paths
            .iter()
            .map(|path| {
                let mut path = path.clone();
                if !path.ends_with('/') {
                    path.push('/');
                }
                path.push_str("**/*.xml");
                FileSet::scan_from_glob(
                    repo_root,
                    path.to_string(),
                    &mut file_counter,
                    team.clone(),
                    codeowners,
                    exec_start,
                )
            })
            .collect::<anyhow::Result<Vec<FileSet>>>()?;
    }

    Ok((file_sets, file_counter))
}

pub async fn extract_failed_tests(
    repo: &BundleRepo,
    org_slug: &str,
    file_sets: &[FileSet],
) -> Vec<Test> {
    let mut failures = Vec::<Test>::new();
    for file_set in file_sets {
        for file in &file_set.files {
            let file = match std::fs::File::open(&file.original_path) {
                Ok(file) => file,
                Err(e) => {
                    log::warn!("Error opening file: {}", e);
                    continue;
                }
            };
            let reader = std::io::BufReader::new(file);
            let mut junitxml = JunitParser::new();
            match junitxml.parse(reader) {
                Ok(junitxml) => junitxml,
                Err(e) => {
                    log::warn!("Error parsing junitxml: {}", e);
                    continue;
                }
            };
            for report in junitxml.reports() {
                for suite in &report.test_suites {
                    let parent_name = String::from(suite.name.as_str());
                    for case in &suite.test_cases {
                        if !matches!(case.status, TestCaseStatus::NonSuccess { .. }) {
                            continue;
                        }
                        let name = String::from(case.name.as_str());
                        let xml_string_to_string =
                            |s: &quick_junit::XmlString| String::from(s.as_str());
                        let classname = case.classname.as_ref().map(xml_string_to_string);
                        let file = case.extra.get("file").map(xml_string_to_string);
                        let id = case.extra.get("id").map(xml_string_to_string);
                        let test = Test::new(
                            name,
                            parent_name.clone(),
                            classname,
                            file,
                            id,
                            org_slug,
                            repo,
                        );
                        failures.push(test);
                    }
                }
            }
        }
    }
    failures
}

pub async fn run_quarantine(
    api_client: &ApiClient,
    request: &api::GetQuarantineBulkTestStatusRequest,
    failures: Vec<Test>,
    exit_code: i32,
) -> QuarantineRunResult {
    let quarantine_config: api::QuarantineConfig = if !failures.is_empty() {
        log::info!("Checking if failed tests can be quarantined");
        let result = api_client.get_quarantining_config(request).await;

        if let Err(ref err) = result {
            log::error!("{}", err);
        }

        result.unwrap_or_default()
    } else {
        log::debug!("No failed tests to quarantine");
        api::QuarantineConfig::default()
    };

    // if quarantining is not enabled, return exit code and empty quarantine status
    if quarantine_config.is_disabled {
        log::info!("Quarantining is not enabled, not quarantining any tests");
        return QuarantineRunResult {
            exit_code,
            quarantine_status: QuarantineBulkTestStatus::default(),
        };
    }

    // quarantine the failed tests
    let mut quarantine_results = QuarantineBulkTestStatus::default();
    let quarantined = &quarantine_config.quarantined_tests;

    let total_failures = failures.len();
    quarantine_results.quarantine_results = failures
        .clone()
        .into_iter()
        .filter_map(|failure| {
            let quarantine_failure = quarantined.contains(&failure.id);
            log::info!(
                "{} -> {}{}(id: {})",
                failure.parent_name,
                failure.name,
                if quarantine_failure {
                    " [QUARANTINED] "
                } else {
                    " "
                },
                failure.id
            );
            if quarantine_failure {
                Some(failure)
            } else {
                None
            }
        })
        .collect();
    quarantine_results.group_is_quarantined =
        quarantine_results.quarantine_results.len() == total_failures;

    // use the exit code from the command if the group is not quarantined
    // override exit code to be exit_success if the group is quarantined
    let exit_code = if total_failures == 0 {
        log::info!("No failed tests to quarantine, returning exit code from command.");
        exit_code
    } else if !quarantine_results.group_is_quarantined {
        log::info!("Not all test failures were quarantined, returning exit code from command.");
        exit_code
    } else if exit_code != EXIT_SUCCESS {
        log::info!("All test failures were quarantined, overriding exit code to be exit_success");
        EXIT_SUCCESS
    } else {
        exit_code
    };

    QuarantineRunResult {
        exit_code,
        quarantine_status: quarantine_results,
    }
}
