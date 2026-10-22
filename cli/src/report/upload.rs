use std::collections::BTreeMap;

use api::urls::{url_for_test_case, url_for_upload};
use bundle::Test;
use constants::EXIT_SUCCESS;
use context::junit::validator::JunitValidationLevel;
use schemars::JsonSchema;
use serde::Serialize;

use super::{ErrorSummary, Notice};
use crate::{upload_command::UploadRunResult, validate_command::JunitReportValidations};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadCommand {
    Upload,
    Test,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadReport {
    /// Why uploading the results failed. The tests are still summarized.
    pub error: Option<ErrorSummary>,
    pub bundle_upload_id: Option<String>,
    /// This upload's page in Trunk. Only set for uploads to a test collection.
    pub upload_url: Option<String>,
    /// One entry per test report file.
    pub validations: Vec<FileValidation>,
    pub counts: TestCounts,
    /// Trunk couldn't say which tests are quarantined, so no failed test counts as quarantined.
    pub quarantine_lookup_failed: bool,
    /// Failed tests that were quarantined, grouped like `failures`.
    pub quarantined: Vec<FailureGroup>,
    /// Failed tests that weren't quarantined, grouped by file, else by suite.
    pub failures: Vec<FailureGroup>,
    pub outcome: Outcome,
    /// The code the CLI exits with.
    pub exit_code: i32,
    /// Warnings that didn't stop the run, such as a deprecated flag.
    pub notices: Vec<Notice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestCounts {
    pub total: usize,
    /// Tests that neither failed nor were quarantined.
    pub passed: usize,
    /// Failed tests that weren't quarantined.
    pub failed: usize,
    /// Failed tests that were quarantined.
    pub quarantined: usize,
    /// `passed / total`, from 0 to 1. Null when there are no tests.
    pub pass_ratio: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FailureGroup {
    pub key: GroupKey,
    pub tests: Vec<FailedTest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum GroupKey {
    /// Failed tests from this file.
    File(String),
    /// Failed tests from this suite that have no file.
    Suite(String),
    /// Failed tests with neither a file nor a suite.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FailedTest {
    pub name: String,
    /// Empty when the test has no suite.
    pub suite: String,
    pub class_name: Option<String>,
    pub file: Option<String>,
    /// Trunk's ID for the test.
    pub id: String,
    pub variant: Option<String>,
    /// The full message, which can be long.
    pub failure_message: Option<String>,
    pub quarantined: bool,
    /// The test's page in Trunk.
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileValidation {
    pub file: String,
    /// Why the file couldn't be parsed. `errors` and `warnings` are empty when this is set.
    pub parse_error: Option<ParseError>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// Null when the file couldn't be parsed.
    pub test_suite_count: Option<usize>,
    /// Null when the file couldn't be parsed.
    pub test_case_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ParseError {
    pub error: String,
    /// The errors that caused `error`, outermost first.
    pub causes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    /// The test reports had no tests.
    NoTestsFound,
    /// Every failed test was quarantined, so none of them fail the run.
    AllFailuresQuarantined,
    /// No tests failed.
    NoFailures,
    /// No tests failed, but the test run's exit code wasn't 0, so some tests may not have run.
    NoFailuresButNonZeroExitCode,
    /// Some failed tests weren't quarantined.
    FailuresNotQuarantined,
}

impl UploadReport {
    /// `test` exits with the test command's exit code even when the upload fails, so the
    /// command decides whether an error's exit code wins.
    pub fn new(result: &UploadRunResult, command: UploadCommand) -> Self {
        let quarantine_context = &result.quarantine_context;
        let quarantined = &quarantine_context.quarantine_status.quarantine_results;
        let failures = &quarantine_context.failures;
        let counts = TestCounts::new(
            result.meta.junit_props.num_tests,
            failures.len(),
            quarantined.len(),
        );
        let error = result.error_report.as_ref().map(ErrorSummary::new);
        let exit_code = match command {
            UploadCommand::Upload => error
                .as_ref()
                .and_then(|error| error.exit_code)
                .unwrap_or(quarantine_context.exit_code),
            UploadCommand::Test => quarantine_context.exit_code,
        };
        let bundle_upload_id = &result.meta.base_props.bundle_upload_id;

        Self {
            error,
            bundle_upload_id: Some(bundle_upload_id.clone()).filter(|id| !id.is_empty()),
            upload_url: upload_url(result),
            validations: file_validations(&result.validations),
            counts,
            quarantine_lookup_failed: quarantine_context.fetch_status.is_failure(),
            quarantined: group(quarantined, |test| failed_test(result, test, true)),
            failures: group(failures, |test| failed_test(result, test, false)),
            outcome: Outcome::new(&counts, quarantine_context.exit_code),
            exit_code,
            notices: result.notices.clone(),
        }
    }
}

impl TestCounts {
    fn new(total: usize, failed: usize, quarantined: usize) -> Self {
        let passed = total.saturating_sub(quarantined + failed);
        Self {
            total,
            passed,
            failed,
            quarantined,
            pass_ratio: (total > 0).then(|| passed as f64 / total as f64),
        }
    }
}

impl FileValidation {
    pub fn has_issues(&self) -> bool {
        self.parse_error.is_some() || !self.errors.is_empty() || !self.warnings.is_empty()
    }
}

impl ParseError {
    pub fn new(error: &anyhow::Error) -> Self {
        Self {
            error: error.to_string(),
            causes: error.chain().skip(1).map(ToString::to_string).collect(),
        }
    }
}

impl Outcome {
    fn new(counts: &TestCounts, test_run_exit_code: i32) -> Self {
        if counts.total == 0 {
            Self::NoTestsFound
        } else if counts.failed == 0 && counts.quarantined > 0 {
            Self::AllFailuresQuarantined
        } else if counts.failed == 0 && test_run_exit_code == EXIT_SUCCESS {
            Self::NoFailures
        } else if counts.failed == 0 {
            Self::NoFailuresButNonZeroExitCode
        } else {
            Self::FailuresNotQuarantined
        }
    }
}

/// Link to this upload's page, when the run is in test-collection link mode. Gated on
/// `test_collection_short_id` rather than `base_props.test_collection.short_id` so an
/// exported-but-blank collection id can't yield a malformed `/collections//uploads/`
/// link. The payload supplies both halves of the key the canonical route is keyed on.
fn upload_url(result: &UploadRunResult) -> Option<String> {
    if result.hide_test_collection_links {
        return None;
    }
    let short_id = result.test_collection_short_id.as_deref()?;
    let test_collection = result.meta.base_props.test_collection.as_ref()?;
    url_for_upload(
        &result.api_address,
        &result.meta.base_props.org,
        short_id,
        &test_collection.bundle_meta_id,
        &test_collection.bundle_meta_created_at,
    )
    .ok()
}

fn failed_test(result: &UploadRunResult, test: &Test, quarantined: bool) -> FailedTest {
    let show_collection_links = !result.hide_test_collection_links;
    let link = url_for_test_case(
        &result.api_address,
        &result.quarantine_context.org_url_slug,
        &result.quarantine_context.repo,
        test,
        result
            .test_collection_short_id
            .as_deref()
            .filter(|_| show_collection_links),
        result.guid_scope.as_ref().filter(|_| show_collection_links),
    );
    FailedTest {
        name: test.name.clone(),
        suite: test.parent_name.clone(),
        class_name: test.class_name.clone(),
        file: test.file.clone(),
        id: test.id.clone(),
        variant: test.variant.clone(),
        failure_message: test.failure_message.clone(),
        quarantined,
        link: link.ok(),
    }
}

fn group(tests: &[Test], to_failed_test: impl Fn(&Test) -> FailedTest) -> Vec<FailureGroup> {
    let mut by_file: BTreeMap<&str, Vec<FailedTest>> = BTreeMap::new();
    let mut by_suite: BTreeMap<&str, Vec<FailedTest>> = BTreeMap::new();
    let mut other = Vec::new();
    for test in tests {
        if let Some(file) = &test.file {
            by_file.entry(file).or_default().push(to_failed_test(test));
        } else if !test.parent_name.is_empty() {
            by_suite
                .entry(&test.parent_name)
                .or_default()
                .push(to_failed_test(test));
        } else {
            other.push(to_failed_test(test));
        }
    }
    let files = by_file.into_iter().map(|(file, tests)| FailureGroup {
        key: GroupKey::File(file.to_string()),
        tests,
    });
    let suites = by_suite.into_iter().map(|(suite, tests)| FailureGroup {
        key: GroupKey::Suite(suite.to_string()),
        tests,
    });
    let other = (!other.is_empty()).then_some(FailureGroup {
        key: GroupKey::Other,
        tests: other,
    });
    files.chain(suites).chain(other).collect()
}

fn file_validations(validations: &JunitReportValidations) -> Vec<FileValidation> {
    validations
        .validations
        .iter()
        .map(|(file, validation)| match validation {
            Err(error) => FileValidation {
                file: file.clone(),
                parse_error: Some(ParseError::new(error)),
                errors: Vec::new(),
                warnings: Vec::new(),
                test_suite_count: None,
                test_case_count: None,
            },
            Ok(validation) => {
                let issues = validation.all_issues_flat();
                let messages = |level: JunitValidationLevel| {
                    issues
                        .iter()
                        .filter(|issue| issue.level == level)
                        .map(|issue| issue.error_message.clone())
                        .collect()
                };
                FileValidation {
                    file: file.clone(),
                    parse_error: None,
                    errors: messages(JunitValidationLevel::Invalid),
                    warnings: messages(JunitValidationLevel::SubOptimal),
                    test_suite_count: Some(validation.test_suites().len()),
                    test_case_count: Some(validation.test_cases().len()),
                }
            }
        })
        .collect()
}
