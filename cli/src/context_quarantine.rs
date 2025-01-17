use std::collections::HashMap;

use api::client::ApiClient;
use bundle::{FileSet, FileSetBuilder, QuarantineBulkTestStatus, Test};
use constants::{EXIT_FAILURE, EXIT_SUCCESS};
use context::{
    junit::{junit_path::JunitReportStatus, parser::JunitParser},
    repo::RepoUrlParts,
};
use quick_junit::TestCaseStatus;

#[derive(Debug, Default, Clone)]
pub struct QuarantineContext {
    pub exit_code: i32,
    pub quarantine_status: QuarantineBulkTestStatus,
}

fn convert_case_to_test<T: AsRef<str>>(
    repo: &RepoUrlParts,
    org_slug: T,
    parent_name: String,
    case: &quick_junit::TestCase,
    suite: &quick_junit::TestSuite,
) -> Test {
    let name = String::from(case.name.as_str());
    let xml_string_to_string = |s: &quick_junit::XmlString| String::from(s.as_str());
    let class_name = case.classname.as_ref().map(xml_string_to_string);
    let file = case.extra.get("file").map(xml_string_to_string);
    let timestamp_millis = case
        .timestamp
        .or(suite.timestamp)
        .map(|t| t.timestamp_millis());
    let mut test = Test {
        name,
        parent_name,
        class_name,
        file,
        id: String::with_capacity(0),
        timestamp_millis,
    };
    if let Some(id) = case.extra.get("id").map(xml_string_to_string) {
        test.id = id;
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
            if let Some(resolved_status) = &file_set.resolved_status {
                // TODO(TRUNK-13911): We should populate the status for all junits, regardless of the presence of a test runner status.
                if resolved_status != &JunitReportStatus::Failed {
                    continue;
                }
            }
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
                            let test = convert_case_to_test(
                                repo,
                                org_slug.as_ref(),
                                parent_name.clone(),
                                case,
                                suite,
                            );
                            match &case.status {
                                TestCaseStatus::Skipped { .. } => {
                                    continue;
                                }
                                TestCaseStatus::Success { .. } => {
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
                                TestCaseStatus::NonSuccess { .. } => {
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
) -> QuarantineContext {
    let failed_tests_extractor = failed_tests_extractor.unwrap_or_else(|| {
        FailedTestsExtractor::new(
            &request.repo,
            &request.org_url_slug,
            file_set_builder.file_sets(),
        )
    });

    let exit_code = test_run_exit_code.unwrap_or_else(|| failed_tests_extractor.exit_code());

    if file_set_builder.no_files_found() {
        log::info!("No JUnit files found, not quarantining any tests");
        return QuarantineContext {
            exit_code,
            ..Default::default()
        };
    }

    let quarantine_config = if !failed_tests_extractor.failed_tests().is_empty() {
        log::info!("Checking if failed tests can be quarantined");
        let result = api_client.get_quarantining_config(request).await;

        if let Err(ref err) = result {
            log::error!("{}", err);
        }

        result.unwrap_or_default()
    } else {
        log::debug!("No failed tests to quarantine");
        api::message::GetQuarantineConfigResponse::default()
    };

    // if quarantining is not enabled, return exit code and empty quarantine status
    if quarantine_config.is_disabled {
        log::info!("Quarantining is not enabled, not quarantining any tests");
        return QuarantineContext {
            exit_code,
            quarantine_status: QuarantineBulkTestStatus::default(),
        };
    }

    // quarantine the failed tests
    let mut quarantine_results = QuarantineBulkTestStatus::default();
    let quarantined = &quarantine_config.quarantined_tests;

    let total_failures = failed_tests_extractor.failed_tests().len();
    quarantine_results.quarantine_results = failed_tests_extractor
        .failed_tests()
        .iter()
        .cloned()
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

    QuarantineContext {
        exit_code,
        quarantine_status: quarantine_results,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bundle::{BundledFile, FileSetType};
    use test_utils::inputs::get_test_file_path;

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
            resolved_status: None,
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
            resolved_status: None,
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
            resolved_status: None,
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
            resolved_status: None,
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
                resolved_status: Some(JunitReportStatus::Passed),
            },
            FileSet {
                file_set_type: FileSetType::Junit,
                files: vec![BundledFile {
                    original_path: get_test_file_path(JUNIT1_FAIL),
                    ..BundledFile::default()
                }],
                glob: String::from("2/*.xml"),
                resolved_status: Some(JunitReportStatus::Flaky),
            },
            FileSet {
                file_set_type: FileSetType::Junit,
                files: vec![BundledFile {
                    original_path: get_test_file_path(JUNIT0_FAIL),
                    ..BundledFile::default()
                }],
                glob: String::from("3/*.xml"),
                resolved_status: Some(JunitReportStatus::Failed),
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
}
