use std::{
    collections::HashMap,
    io::{BufReader, Read},
};

use api::{client::ApiClient, urls::url_for_test_case};
use bundle::{
    FileSet, FileSetBuilder, FileSetTestRunnerReport, FileSetType, QuarantineBulkTestStatus, Test,
};
use chrono::TimeDelta;
use constants::{EXIT_FAILURE, EXIT_SUCCESS};
use context::{
    junit::{
        bindings::{
            BindingsReport, BindingsTestCase, BindingsTestCaseStatusStatus, BindingsTestSuite,
        },
        junit_path::TestRunnerReportStatus,
        parser::{bin_parse, JunitParser},
    },
    repo::RepoUrlParts,
};
use pluralizer::pluralize;
use prost::Message;

#[derive(Debug)]
pub enum QuarantineFetchStatus {
    FetchSucceeded,
    FetchSkipped,
    FetchFailed(anyhow::Error),
}
impl QuarantineFetchStatus {
    pub fn is_failure(&self) -> bool {
        match self {
            Self::FetchSucceeded => false,
            Self::FetchSkipped => false,
            Self::FetchFailed(_) => true,
        }
    }
}

#[derive(Debug)]
pub struct QuarantineContext {
    pub exit_code: i32,
    pub quarantine_status: QuarantineBulkTestStatus,
    pub failures: Vec<Test>,
    pub repo: RepoUrlParts,
    pub org_url_slug: String,
    pub fetch_status: QuarantineFetchStatus,
}
impl QuarantineContext {
    pub fn skip_fetch(failures: Vec<Test>) -> Self {
        Self {
            exit_code: i32::default(),
            quarantine_status: QuarantineBulkTestStatus::default(),
            failures,
            repo: RepoUrlParts::default(),
            org_url_slug: String::default(),
            fetch_status: QuarantineFetchStatus::FetchSkipped,
        }
    }

    pub fn fail_fetch(error: anyhow::Error) -> Self {
        Self {
            exit_code: i32::default(),
            quarantine_status: QuarantineBulkTestStatus::default(),
            failures: Vec::default(),
            repo: RepoUrlParts::default(),
            org_url_slug: String::default(),
            fetch_status: QuarantineFetchStatus::FetchFailed(error),
        }
    }
}

fn convert_case_to_test<T: AsRef<str>>(
    repo: &RepoUrlParts,
    org_slug: T,
    parent_name: String,
    case: &BindingsTestCase,
    suite: &BindingsTestSuite,
) -> Test {
    let name = String::from(case.name.as_str());
    let class_name = case.classname.clone();
    let file = case.extra().get("file").cloned();
    // convert timestamp_micros to millis using chrono
    let timestamp_millis = Some(TimeDelta::num_milliseconds(&TimeDelta::microseconds(
        case.timestamp_micros
            .or(suite.timestamp_micros)
            .unwrap_or(0),
    )));
    let failure_message = match &case.status.non_success {
        Some(non_success) => non_success
            .description
            .clone()
            .or_else(|| non_success.message.clone()),
        _ => None,
    };
    let mut test = Test {
        name,
        parent_name,
        class_name,
        file,
        id: String::with_capacity(0),
        timestamp_millis,
        is_quarantined: case.is_quarantined(),
        failure_message,
    };
    if let Some(id) = case.extra().get("id") {
        if id.is_empty() {
            test.set_id(org_slug, repo);
        } else {
            // trunk-ignore(clippy/assigning_clones)
            test.id = id.clone();
        }
    } else {
        test.set_id(org_slug, repo);
    }
    test
}

#[derive(Debug, Default, Clone)]
pub struct FailedTestsExtractor {
    failed_tests: Vec<Test>,
}

impl FailedTestsExtractor {
    pub fn new<T: AsRef<str>>(repo: &RepoUrlParts, org_slug: T, file_sets: &[FileSet]) -> Self {
        let mut failures: HashMap<String, Test> = HashMap::new();
        let mut successes: HashMap<String, i64> = HashMap::new();

        for file_set in file_sets {
            if let Some(FileSetTestRunnerReport {
                resolved_status, ..
            }) = file_set.test_runner_report
            {
                // TODO(TRUNK-13911): We should populate the status for all junits, regardless of the presence of a test runner status.
                if resolved_status != TestRunnerReportStatus::Failed {
                    continue;
                }
            }
            for base_file in &file_set.files {
                let file = match std::fs::File::open(&base_file.original_path) {
                    Ok(file) => file,
                    Err(err) => {
                        tracing::warn!(
                            "Failed to open file {:?} for reading: {}",
                            base_file.original_path,
                            err
                        );
                        continue;
                    }
                };
                let mut reader = BufReader::new(file);
                let bindings_reports = match file_set.file_set_type {
                    FileSetType::Junit => {
                        let mut junitxml = JunitParser::new();
                        match junitxml.parse(reader) {
                            Ok(junitxml) => junitxml,
                            Err(err) => {
                                tracing::warn!(
                                    "Failed to parse junit xml file {:?}: {}",
                                    base_file.original_path,
                                    err
                                );
                                continue;
                            }
                        };
                        junitxml
                            .into_reports()
                            .iter()
                            .map(|report| BindingsReport::from(report.clone()))
                            .collect::<Vec<BindingsReport>>()
                    }
                    FileSetType::Internal => {
                        let mut buffer = Vec::new();
                        let result = reader.read_to_end(&mut buffer);
                        if let Err(err) = result {
                            tracing::warn!(
                                "Failed to read file {:?} for reading: {}",
                                base_file.original_path,
                                err
                            );
                            continue;
                        }
                        let test_result = bin_parse(buffer.as_slice());
                        if let Ok(test_result) = test_result {
                            test_result
                        } else {
                            tracing::warn!(
                                "Failed to decode file {:?} for reading: {}",
                                base_file.original_path,
                                test_result.unwrap_err()
                            );
                            continue;
                        }
                    }
                };
                for report in bindings_reports {
                    for suite in report.test_suites {
                        let parent_name = String::from(suite.name.as_str());
                        for case in &suite.test_cases {
                            let test = convert_case_to_test(
                                repo,
                                org_slug.as_ref(),
                                parent_name.clone(),
                                case,
                                &suite,
                            );
                            match &case.status.status {
                                BindingsTestCaseStatusStatus::Unspecified
                                | BindingsTestCaseStatusStatus::Skipped { .. } => {
                                    continue;
                                }
                                BindingsTestCaseStatusStatus::Success { .. } => {
                                    if let Some(existing_timestamp) = successes.get(&test.id) {
                                        if *existing_timestamp > test.timestamp_millis.unwrap_or(0)
                                        {
                                            continue;
                                        }
                                    }
                                    successes.insert(
                                        test.id.clone(),
                                        test.timestamp_millis.unwrap_or(0),
                                    );
                                }
                                BindingsTestCaseStatusStatus::NonSuccess { .. } => {
                                    // Only store the most recent failure of a given test run ID
                                    if let Some(existing_test) = failures.get(&test.id) {
                                        if existing_test.timestamp_millis > test.timestamp_millis {
                                            continue;
                                        }
                                    }
                                    failures.insert(test.id.clone(), test);
                                }
                            }
                        }
                    }
                }
            }
        }

        let failed_tests = failures
            .into_iter()
            .filter_map(|(id, test)| {
                // Tests with the same id and a later timestamp should override their previous status.
                if let Some(existing_timestamp) = successes.get(&id) {
                    if *existing_timestamp > test.timestamp_millis.unwrap_or(0) {
                        return None;
                    }
                }
                Some(test)
            })
            .collect();

        Self { failed_tests }
    }

    pub fn failed_tests(&self) -> &[Test] {
        &self.failed_tests
    }

    pub fn exit_code(&self) -> i32 {
        if self.failed_tests.is_empty() {
            EXIT_SUCCESS
        } else {
            EXIT_FAILURE
        }
    }
}

pub async fn gather_quarantine_context(
    api_client: &ApiClient,
    request: &api::message::GetQuarantineConfigRequest,
    file_set_builder: &FileSetBuilder,
    failed_tests_extractor: Option<FailedTestsExtractor>,
    test_run_exit_code: Option<i32>,
) -> anyhow::Result<QuarantineContext> {
    let failed_tests_extractor = failed_tests_extractor.unwrap_or_else(|| {
        FailedTestsExtractor::new(
            &request.repo,
            &request.org_url_slug,
            file_set_builder.file_sets(),
        )
    });

    let mut exit_code = test_run_exit_code.unwrap_or(EXIT_SUCCESS);

    if file_set_builder.no_files_found() {
        tracing::info!("No test output files found, not quarantining any tests.");
        return Ok(QuarantineContext {
            exit_code,
            repo: request.repo.clone(),
            org_url_slug: request.org_url_slug.clone(),
            quarantine_status: QuarantineBulkTestStatus::default(),
            failures: Vec::default(),
            fetch_status: QuarantineFetchStatus::FetchSkipped,
        });
    }

    let (quarantine_config, quarantine_fetch_status) = if !failed_tests_extractor
        .failed_tests()
        .is_empty()
    {
        tracing::info!("Checking if failed tests can be quarantined");
        match api_client.get_quarantining_config(request).await {
            anyhow::Result::Ok(response) => (Some(response), QuarantineFetchStatus::FetchSucceeded),
            anyhow::Result::Err(error) => (None, QuarantineFetchStatus::FetchFailed(error)),
        }
    } else {
        tracing::debug!("Skipping quarantine check.");
        (None, QuarantineFetchStatus::FetchSkipped)
    };

    // if quarantining is not enabled, return exit code and empty quarantine status
    if quarantine_config
        .as_ref()
        .map(|q| q.is_disabled)
        .unwrap_or_default()
    {
        tracing::info!("Quarantining is not enabled, not quarantining any tests");
        return Ok(QuarantineContext {
            exit_code,
            quarantine_status: QuarantineBulkTestStatus::default(),
            failures: failed_tests_extractor.failed_tests().to_vec(),
            repo: request.repo.clone(),
            org_url_slug: request.org_url_slug.clone(),
            fetch_status: quarantine_fetch_status,
        });
    } else {
        // quarantining is enabled, continue with quarantine process and update exit code
        exit_code = test_run_exit_code.unwrap_or_else(|| failed_tests_extractor.exit_code());
    }

    // quarantine the failed tests
    let mut quarantine_results = QuarantineBulkTestStatus::default();
    let quarantined = &(quarantine_config
        .map(|q| q.quarantined_tests)
        .unwrap_or_default());

    let total_failures = failed_tests_extractor.failed_tests().len();
    let mut failures: Vec<Test> = vec![];
    let mut quarantined_failures: Vec<Test> = vec![];
    failed_tests_extractor
        .failed_tests()
        .iter()
        .cloned()
        .for_each(|failure| {
            let quarantine_failure = quarantined.contains(&failure.id) || failure.is_quarantined;
            if quarantine_failure {
                quarantined_failures.push(failure);
            } else {
                failures.push(failure);
            }
        });

    if !quarantined_failures.is_empty() {
        tracing::info!(
            "{} test {} quarantined:",
            quarantined_failures.len(),
            pluralize("failure", quarantined_failures.len() as isize, false),
        );
        quarantined_failures
            .iter()
            .for_each(|quarantined_failure| log_failure(quarantined_failure, request, api_client));
    }

    if !failures.is_empty() {
        tracing::info!(
            "️❌ {} test {} not quarantined:",
            failures.len(),
            pluralize("failure", quarantined_failures.len() as isize, false),
        );
        failures
            .iter()
            .for_each(|failure| log_failure(failure, request, api_client));
    }
    let quarantined_failure_count = quarantined_failures.len();
    quarantine_results.quarantine_results = quarantined_failures;
    quarantine_results.group_is_quarantined =
        quarantine_results.quarantine_results.len() == total_failures;

    // use the exit code from the command if the group is not quarantined
    // override exit code to be exit_success if the group is quarantined
    let exit_code = if total_failures == 0 {
        tracing::info!("No failed tests to quarantine, returning exit code from command.");
        exit_code
    } else if !quarantine_results.group_is_quarantined {
        tracing::info!(
            "Quarantined {} out of {} test failures",
            quarantined_failure_count,
            total_failures
        );
        tracing::info!(
            "Not all test failures were quarantined, using exit code {} from command",
            exit_code
        );
        exit_code
    } else if exit_code != EXIT_SUCCESS {
        tracing::info!(
            "All test failures were quarantined, overriding exit code to be exit_success"
        );
        EXIT_SUCCESS
    } else {
        exit_code
    };

    Ok(QuarantineContext {
        exit_code,
        quarantine_status: quarantine_results,
        failures,
        repo: request.repo.clone(),
        org_url_slug: request.org_url_slug.clone(),
        fetch_status: quarantine_fetch_status,
    })
}

fn log_failure(
    failure: &Test,
    request: &api::message::GetQuarantineConfigRequest,
    api_client: &ApiClient,
) {
    let url = match url_for_test_case(
        &api_client.api_host,
        &request.org_url_slug,
        &request.repo,
        failure,
    ) {
        Ok(url) => format!("Learn more > {}", url),
        Err(_) => String::from(""),
    };
    tracing::info!("\t{} -> {} {}", failure.parent_name, failure.name, url,);
}

#[cfg(test)]
mod tests {
    use bundle::{BundledFile, FileSetType};
    use test_utils::inputs::get_test_file_path;

    use super::*;

    /// Contains 1 failure at 1:00
    const JUNIT0_FAIL: &str = "test_fixtures/junit0_fail.xml";
    const JUNIT0_FAIL_SUITE: &str = "test_fixtures/junit0_fail_suite_timestamp.xml";
    // Contains 1 pass at 2:00
    const JUNIT0_PASS: &str = "test_fixtures/junit0_pass.xml";
    const JUNIT0_PASS_SUITE: &str = "test_fixtures/junit0_pass_suite_timestamp.xml";
    // Contains 1 failure at 3:00 and 1 failure at 5:00
    const JUNIT1_FAIL: &str = "test_fixtures/junit1_fail.xml";
    // Contains 2 passes at 4:00
    const JUNIT1_PASS: &str = "test_fixtures/junit1_pass.xml";

    const ORG_SLUG: &str = "test-org";

    #[tokio::test(start_paused = true)]
    async fn test_extract_retry_failed_tests() {
        let file_sets = vec![FileSet {
            file_set_type: FileSetType::Junit,
            files: vec![
                BundledFile {
                    original_path: get_test_file_path(JUNIT0_FAIL),
                    ..BundledFile::default()
                },
                BundledFile {
                    original_path: get_test_file_path(JUNIT0_PASS),
                    ..BundledFile::default()
                },
            ],
            glob: String::from("**/*.xml"),
            test_runner_report: None,
        }];

        let retried_failures =
            FailedTestsExtractor::new(&RepoUrlParts::default(), ORG_SLUG, &file_sets)
                .failed_tests()
                .to_vec();
        assert!(retried_failures.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn test_extract_retry_suite_failed_tests() {
        let file_sets = vec![FileSet {
            file_set_type: FileSetType::Junit,
            files: vec![
                BundledFile {
                    original_path: get_test_file_path(JUNIT0_FAIL_SUITE),
                    ..BundledFile::default()
                },
                BundledFile {
                    original_path: get_test_file_path(JUNIT0_PASS_SUITE),
                    ..BundledFile::default()
                },
            ],
            glob: String::from("**/*.xml"),
            test_runner_report: None,
        }];

        let retried_failures =
            FailedTestsExtractor::new(&RepoUrlParts::default(), ORG_SLUG, &file_sets)
                .failed_tests()
                .to_vec();
        assert!(retried_failures.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn test_extract_multi_failed_tests() {
        let file_sets = vec![FileSet {
            file_set_type: FileSetType::Junit,
            files: vec![
                BundledFile {
                    original_path: get_test_file_path(JUNIT0_FAIL),
                    ..BundledFile::default()
                },
                BundledFile {
                    original_path: get_test_file_path(JUNIT1_FAIL),
                    ..BundledFile::default()
                },
            ],
            glob: String::from("**/*.xml"),
            test_runner_report: None,
        }];

        let mut multi_failures =
            FailedTestsExtractor::new(&RepoUrlParts::default(), ORG_SLUG, &file_sets)
                .failed_tests()
                .to_vec();
        multi_failures.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(multi_failures.len(), 2);
        assert_eq!(multi_failures[0].name, "Goodbye");
        assert_eq!(multi_failures[1].name, "Hello");
    }

    #[tokio::test(start_paused = true)]
    async fn test_extract_some_retried_failed_tests() {
        let file_sets = vec![FileSet {
            file_set_type: FileSetType::Junit,
            files: vec![
                BundledFile {
                    original_path: get_test_file_path(JUNIT0_FAIL),
                    ..BundledFile::default()
                },
                BundledFile {
                    original_path: get_test_file_path(JUNIT1_FAIL),
                    ..BundledFile::default()
                },
                BundledFile {
                    original_path: get_test_file_path(JUNIT0_PASS),
                    ..BundledFile::default()
                },
                BundledFile {
                    original_path: get_test_file_path(JUNIT1_PASS),
                    ..BundledFile::default()
                },
            ],
            glob: String::from("**/*.xml"),
            test_runner_report: None,
        }];

        let some_failures =
            FailedTestsExtractor::new(&RepoUrlParts::default(), ORG_SLUG, &file_sets)
                .failed_tests()
                .to_vec();
        assert_eq!(some_failures.len(), 1);
        assert_eq!(some_failures[0].name, "Goodbye");
    }

    #[tokio::test(start_paused = true)]
    async fn test_extract_multi_failed_tests_with_runner_status() {
        let file_sets = vec![
            FileSet {
                file_set_type: FileSetType::Junit,
                files: vec![BundledFile {
                    original_path: get_test_file_path(JUNIT1_FAIL),
                    ..BundledFile::default()
                }],
                glob: String::from("1/*.xml"),
                test_runner_report: Some(FileSetTestRunnerReport {
                    resolved_status: TestRunnerReportStatus::Passed,
                    ..Default::default()
                }),
            },
            FileSet {
                file_set_type: FileSetType::Junit,
                files: vec![BundledFile {
                    original_path: get_test_file_path(JUNIT1_FAIL),
                    ..BundledFile::default()
                }],
                glob: String::from("2/*.xml"),
                test_runner_report: Some(FileSetTestRunnerReport {
                    resolved_status: TestRunnerReportStatus::Flaky,
                    ..Default::default()
                }),
            },
            FileSet {
                file_set_type: FileSetType::Junit,
                files: vec![BundledFile {
                    original_path: get_test_file_path(JUNIT0_FAIL),
                    ..BundledFile::default()
                }],
                glob: String::from("3/*.xml"),
                test_runner_report: Some(FileSetTestRunnerReport {
                    resolved_status: TestRunnerReportStatus::Failed,
                    ..Default::default()
                }),
            },
        ];

        let mut multi_failures =
            FailedTestsExtractor::new(&RepoUrlParts::default(), ORG_SLUG, &file_sets)
                .failed_tests()
                .to_vec();
        multi_failures.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(multi_failures.len(), 1);
        assert_eq!(multi_failures[0].name, "Hello");
    }

    #[test]
    fn test_convert_case_to_test_populates_failure_message() {
        use quick_junit::{NonSuccessKind, TestCase, TestCaseStatus, TestSuite};

        let repo = RepoUrlParts {
            host: "github.com".to_string(),
            owner: "test-owner".to_string(),
            name: "test-repo".to_string(),
        };
        let org_slug = "test-org";
        let parent_name = "TestSuite".to_string();

        // Create the underlying types first
        let mut test_case = TestCase::new(
            String::from("test_case"),
            TestCaseStatus::NonSuccess {
                kind: NonSuccessKind::Failure,
                message: Some("Failure message".into()),
                ty: None,
                description: Some("This is a failure".into()),
                reruns: vec![],
            },
        );
        test_case.classname = Some("TestClass".into());

        let test_suite = TestSuite::new(parent_name.clone());

        // Convert to bindings
        let case = BindingsTestCase::from(test_case);
        let suite = BindingsTestSuite::from(test_suite);

        let test = super::convert_case_to_test(&repo, org_slug, parent_name, &case, &suite);

        assert_eq!(test.name, "test_case");
        assert_eq!(test.failure_message, Some("This is a failure".to_string()));
    }
}
