use std::collections::HashMap;
use std::str;
use std::{fs, path::Path, time::Duration};

use chrono::{DateTime, Utc};
use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestRerun, TestSuite};

use crate::file_attribution::ReportedPath;
use crate::test_locations::{Limits, TestKey, TestLocationIndex};
use crate::types::{
    SWIFT_DEFAULT_TEST_SUITE_NAME,
    schema::{TestNode, TestNodeType, TestResult, Tests},
};
use crate::xcresult_legacy::XCResultTestLegacy;
use crate::xcrun::{
    xcresulttool_get_object, xcresulttool_get_test_results_summary,
    xcresulttool_get_test_results_tests,
};

/// Where a test's file comes from — where a failure surfaced, or where the test is
/// declared — which also decides which `xcresulttool` calls the bundle is read with.
#[derive(Debug)]
pub enum FileAttribution {
    FailureSummaries(HashMap<String, XCResultTestLegacy>),
    Declarations(TestLocationIndex),
}

#[derive(Debug)]
pub struct XCResult {
    tests: Tests,
    org_url_slug: String,
    repo_full_name: String,
    attribution: FileAttribution,
    test_run_started_at: Option<DateTime<Utc>>,
}

impl XCResult {
    pub fn new<T: AsRef<Path>>(
        path: T,
        org_url_slug: String,
        repo_full_name: String,
        use_experimental_failure_summary: bool,
    ) -> anyhow::Result<XCResult> {
        let absolute_path = fs::canonicalize(path.as_ref()).map_err(|e| {
            anyhow::anyhow!(
                "failed to get absolute path for {}: {}",
                path.as_ref().display(),
                e
            )
        })?;

        // Call xcresulttool_get_object once and use it for both timestamp extraction and legacy tests
        let actions_invocation_record = xcresulttool_get_object(&absolute_path);

        // Extract test run start time from the actions invocation record
        let test_run_started_at = match &actions_invocation_record {
            Ok(record) => {
                record
                    .actions
                    .as_ref()
                    .and_then(|arr| arr.values.first())
                    .and_then(|action_record| {
                        action_record.started_time.as_ref().and_then(|date| {
                            // xcresult uses format like "2024-09-30T12:12:51.159-0700" without colon in timezone
                            DateTime::parse_from_rfc3339(&date.value)
                                .or_else(|_| {
                                    DateTime::parse_from_str(&date.value, "%Y-%m-%dT%H:%M:%S%.3f%z")
                                })
                                .ok()
                                .map(|dt| dt.with_timezone(&Utc))
                        })
                    })
            }
            Err(e) => {
                tracing::warn!("Failed to get test run start time from xcresult: {}", e);
                None
            }
        };

        // Generate legacy test info from the same actions invocation record
        let legacy_xcresult_tests = match actions_invocation_record {
            Ok(record) => {
                match XCResultTestLegacy::generate_from_record(
                    &absolute_path,
                    record,
                    use_experimental_failure_summary,
                ) {
                    Ok(tests) => tests,
                    Err(e) => {
                        tracing::warn!(
                            "Failed to generate legacy XCResultTestLegacy objects: {}",
                            e
                        );
                        tracing::warn!(
                            "Attempting to continue without legacy XCResultTestLegacy objects"
                        );
                        HashMap::new()
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to get actions invocation record: {}, continuing without legacy tests",
                    e
                );
                HashMap::new()
            }
        };
        Ok(XCResult {
            tests: xcresulttool_get_test_results_tests(&absolute_path)?,
            attribution: FileAttribution::FailureSummaries(legacy_xcresult_tests),
            org_url_slug,
            repo_full_name,
            test_run_started_at,
        })
    }

    /// Read the bundle without a single `get object --legacy` call, taking each test's file
    /// from where it is declared in `repo_root`. Beyond attribution, that is what the flag
    /// buys: the legacy per-test summary fetch is unbounded (48 GB peak on one timed-out
    /// test) and nothing here can reach that object.
    pub fn new_with_declaration_locations<T: AsRef<Path>, U: AsRef<Path>>(
        path: T,
        org_url_slug: String,
        repo_full_name: String,
        repo_root: U,
        limits: Limits,
    ) -> anyhow::Result<XCResult> {
        let absolute_path = fs::canonicalize(path.as_ref()).map_err(|e| {
            anyhow::anyhow!(
                "failed to get absolute path for {}: {}",
                path.as_ref().display(),
                e
            )
        })?;
        let tests = xcresulttool_get_test_results_tests(&absolute_path)?;

        let test_run_started_at = match xcresulttool_get_test_results_summary(&absolute_path) {
            Ok(summary) => summary.start_time.and_then(|start_time| {
                // This float's ULP exceeds a microsecond at epoch magnitudes, so anything
                // below the millisecond the legacy date string also carries is noise.
                DateTime::from_timestamp_millis((start_time * 1e3).round() as i64)
            }),
            Err(e) => {
                tracing::warn!("Failed to get test run start time from xcresult: {}", e);
                None
            }
        };

        let mut keys = Vec::new();
        collect_test_keys(&tests.test_nodes, &mut keys);
        let index = TestLocationIndex::resolve(repo_root.as_ref(), &keys, limits);
        if index.is_empty() {
            tracing::warn!(
                "no test declarations found under {}; falling back to failure locations",
                repo_root.as_ref().display()
            );
        }

        Ok(XCResult {
            tests,
            attribution: FileAttribution::Declarations(index),
            org_url_slug,
            repo_full_name,
            test_run_started_at,
        })
    }

    pub fn generate_junits(&self) -> Vec<Report> {
        self.tests
            .test_nodes
            .iter()
            .filter(|tn| matches!(tn.node_type, TestNodeType::TestPlan))
            .map(|test_plan| {
                let mut report = Report::new(format!("xcresult: {}", test_plan.name));
                report.add_test_suites(self.xcresult_test_bundles_and_suites_to_junit_test_suites(
                    test_plan.children.as_slice(),
                ));
                report
            })
            .collect()
    }

    fn xcresult_test_bundles_and_suites_to_junit_test_suites(
        &self,
        test_nodes: &[TestNode],
    ) -> Vec<TestSuite> {
        test_nodes
            .iter()
            .flat_map(|test_bundle_or_test_suite| {
                if matches!(
                    test_bundle_or_test_suite.node_type,
                    TestNodeType::UnitTestBundle | TestNodeType::UiTestBundle
                ) {
                    let test_bundle = test_bundle_or_test_suite;
                    self.xcresult_test_suites_to_junit_test_suites(
                        test_bundle.children.as_slice(),
                        Some(&test_bundle.name),
                    )
                } else if matches!(test_bundle_or_test_suite.node_type, TestNodeType::TestSuite) {
                    let test_suite = test_bundle_or_test_suite;
                    vec![
                        self.xcresult_test_suite_to_junit_test_suite(
                            test_suite,
                            Option::<&str>::None,
                        ),
                    ]
                } else {
                    vec![]
                }
            })
            .collect()
    }

    fn xcresult_test_suites_to_junit_test_suites<T: AsRef<str>>(
        &self,
        test_nodes: &[TestNode],
        bundle_name: Option<T>,
    ) -> Vec<TestSuite> {
        let mut test_suites = test_nodes
            .iter()
            .filter(|tn| matches!(tn.node_type, TestNodeType::TestSuite))
            .map(|test_suite| {
                self.xcresult_test_suite_to_junit_test_suite(test_suite, bundle_name.as_ref())
            })
            .collect::<Vec<_>>();
        // test cases can be at the top level
        let dangling_test_cases = test_nodes
            .iter()
            .filter(|tn| matches!(tn.node_type, TestNodeType::TestCase))
            .collect::<Vec<_>>();
        if !dangling_test_cases.is_empty() {
            let mut test_suite = TestSuite::new(
                bundle_name
                    .as_ref()
                    .map(|bn| bn.as_ref())
                    .unwrap_or(SWIFT_DEFAULT_TEST_SUITE_NAME),
            );
            test_suite.add_test_cases(self.xcresult_test_cases_to_junit_test_cases(test_nodes));
            test_suites.push(test_suite);
        }
        test_suites
    }

    fn xcresult_test_suite_to_junit_test_suite<T: AsRef<str>>(
        &self,
        xcresult_test_suite: &TestNode,
        bundle_name: Option<T>,
    ) -> TestSuite {
        let name = bundle_name
            .as_ref()
            .map(|bn| format!("{}.{}", bn.as_ref(), xcresult_test_suite.name))
            .unwrap_or_else(|| String::from(&xcresult_test_suite.name));
        let mut test_suite = TestSuite::new(name);
        test_suite.add_test_cases(
            self.xcresult_test_cases_to_junit_test_cases(xcresult_test_suite.children.as_slice()),
        );
        test_suite
    }

    fn xcresult_test_cases_to_junit_test_cases(&self, test_nodes: &[TestNode]) -> Vec<TestCase> {
        test_nodes
            .iter()
            .filter(|tn| matches!(tn.node_type, TestNodeType::TestCase))
            .filter_map(|tn| tn.result.as_ref().map(|result| (tn, *result)))
            .filter_map(|(xcresult_test_case, test_result)| {
                let status = match test_result {
                    TestResult::Passed | TestResult::ExpectedFailure => TestCaseStatus::success(),
                    TestResult::Failed => TestCaseStatus::non_success(NonSuccessKind::Failure),
                    TestResult::Skipped => TestCaseStatus::skipped(),
                    TestResult::Unknown => {
                        tracing::debug!(
                            "unknown test result for test case: {}",
                            xcresult_test_case.name
                        );
                        return None;
                    }
                };
                let mut test_case = TestCase::new(String::from(&xcresult_test_case.name), status);
                let classname = xcresult_test_case
                    .node_identifier
                    .as_ref()
                    .and_then(|node_identifier| node_identifier.rsplit('/').next_back());
                if let Some(classname) = classname {
                    test_case.set_classname(classname);
                }

                let failure_messages = Self::xcresult_failure_messages_to_strings(
                    xcresult_test_case.children.as_slice(),
                );
                if !failure_messages.is_empty() {
                    if let TestCaseStatus::NonSuccess {
                        ref mut message, ..
                    } = test_case.status
                    {
                        *message = Some(failure_messages.join("\n").into())
                    }
                }

                let test_reruns = Self::xcresult_repetitions_to_junit_test_reruns(
                    xcresult_test_case.children.as_slice(),
                );
                if !test_reruns.is_empty() {
                    match test_case.status {
                        TestCaseStatus::Success {
                            ref mut flaky_runs, ..
                        } => {
                            *flaky_runs = test_reruns;
                        }
                        TestCaseStatus::NonSuccess { ref mut reruns, .. } => {
                            *reruns = test_reruns;
                        }
                        _ => {}
                    }
                }

                if let Some(duration) = Self::xcresult_test_node_to_duration(xcresult_test_case) {
                    test_case.set_time(duration);
                }

                // Set timestamp to test run start time (applies to all tests in the run)
                if let Some(started_at) = self.test_run_started_at {
                    test_case.set_timestamp(started_at);
                }

                if let Some(id) = self.generate_id(xcresult_test_case) {
                    test_case.extra.insert("id".into(), id.into());
                }
                if let Some(file) = self.find_test_case_file(xcresult_test_case) {
                    test_case.extra.insert("file".into(), file.into());
                }

                Some(test_case)
            })
            .collect()
    }

    fn xcresult_repetitions_to_junit_test_reruns(test_nodes: &[TestNode]) -> Vec<TestRerun> {
        test_nodes
            .iter()
            .filter(|tn| matches!(tn.node_type, TestNodeType::Repetition))
            .filter_map(|tn| tn.result.as_ref().map(|result| (tn, *result)))
            .filter_map(|(repetition, test_result)| {
                let status = match test_result {
                    TestResult::Passed | TestResult::ExpectedFailure => {
                        // A successful repetition isn't relevant to JUnit test reruns
                        return None;
                    }
                    TestResult::Failed => NonSuccessKind::Failure,
                    TestResult::Skipped | TestResult::Unknown => {
                        tracing::debug!(
                            "unexpected test result for repetition: {}",
                            repetition.name
                        );
                        return None;
                    }
                };
                let mut test_rerun = TestRerun::new(status);

                let failure_messages =
                    Self::xcresult_failure_messages_to_strings(repetition.children.as_slice());
                if !failure_messages.is_empty() {
                    test_rerun.set_message(failure_messages.join("\n"));
                }

                if let Some(duration) = Self::xcresult_test_node_to_duration(repetition) {
                    test_rerun.set_time(duration);
                }

                Some(test_rerun)
            })
            .collect()
    }

    fn xcresult_failure_messages_to_strings(test_nodes: &[TestNode]) -> Vec<String> {
        test_nodes
            .iter()
            .filter(|tn| matches!(tn.node_type, TestNodeType::FailureMessage))
            .map(|failure_message| String::from(&failure_message.name))
            .collect()
    }

    fn xcresult_test_node_to_duration(test_node: &TestNode) -> Option<Duration> {
        test_node
            .duration
            .as_ref()
            .and_then(|secs| secs.replace('s', "").parse::<f64>().ok())
            .and_then(|secs| Duration::try_from_secs_f64(secs).ok())
    }

    fn generate_id(&self, test_case: &TestNode) -> Option<String> {
        let node_identifier = test_case.node_identifier.as_deref()?;
        let identifier_url = match &self.attribution {
            FileAttribution::FailureSummaries(tests) => tests
                .get(node_identifier)
                .map(|test| test.identifier_url.as_str()),
            // The legacy `identifierURL` under another name, so ids match across paths.
            FileAttribution::Declarations(_) => test_case.node_identifier_url.as_deref(),
        };
        Some(
            uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_URL,
                format!(
                    "{}#{}#{}",
                    &self.org_url_slug,
                    &self.repo_full_name,
                    identifier_url.unwrap_or(node_identifier)
                )
                .as_bytes(),
            )
            .to_string(),
        )
    }

    fn find_test_case_file(&self, test_case: &TestNode) -> Option<String> {
        let node_identifier = test_case.node_identifier.as_deref()?;
        match &self.attribution {
            FileAttribution::FailureSummaries(tests) => tests
                .get(node_identifier)
                .and_then(|test| test.file.clone()),
            FileAttribution::Declarations(index) => {
                if let Some(site) = index.lookup(&TestKey::from_node_identifier(node_identifier)) {
                    tracing::debug!(
                        "{} is declared at {}:{}",
                        node_identifier,
                        site.file.as_str(),
                        site.line.unwrap_or_default()
                    );
                    return Some(site.file.as_str().to_owned());
                }
                // A runtime-registered test (Quick, `+testInvocations`) has no declaration
                // to find, so fall back to where the failure surfaced.
                first_source_location(test_case)
                    .map(|path| ReportedPath::new(&path))
                    .filter(|path| !path.is_vendored_dependency())
                    .map(ReportedPath::into_string)
            }
        }
    }
}

fn collect_test_keys(test_nodes: &[TestNode], keys: &mut Vec<TestKey>) {
    for test_node in test_nodes {
        if matches!(test_node.node_type, TestNodeType::TestCase)
            && let Some(node_identifier) = &test_node.node_identifier
        {
            keys.push(TestKey::from_node_identifier(node_identifier));
        }
        collect_test_keys(&test_node.children, keys);
    }
}

fn first_source_location(test_node: &TestNode) -> Option<String> {
    if let Some(source_location) = &test_node.source_location {
        return Some(source_location.file_path.clone());
    }
    test_node.children.iter().find_map(first_source_location)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::{Value, json};

    use super::*;

    const TEST_FILE: &str = "/repo/Tests/SnapshotReproTests.swift";
    const HELPER_FILE: &str = "/repo/Tests/FailureHelper.swift";
    const DEPENDENCY_FILE: &str = "/repo/DerivedData/SourcePackages/checkouts/Dep/Assert.swift";

    fn suite(name: &str, children: Vec<Value>) -> Value {
        json!({ "nodeType": "Test Suite", "name": name, "children": children })
    }

    fn case(name: &str, node_identifier: &str, result: &str, children: Vec<Value>) -> Value {
        json!({
            "nodeType": "Test Case",
            "name": name,
            "nodeIdentifier": node_identifier,
            "result": result,
            "children": children
        })
    }

    /// The node a failure hangs its location off, which is where the failure was *raised*.
    fn raised_at(file: &str) -> Value {
        json!({
            "nodeType": "Source Code Reference",
            "name": file,
            "sourceLocation": { "filePath": file, "lineNumber": 9 }
        })
    }

    fn bundle(name: &str, children: Vec<Value>) -> Tests {
        serde_json::from_value(json!({
            "testPlanConfigurations": [],
            "devices": [],
            "testNodes": [{
                "nodeType": "Test Plan",
                "name": "ExamplePlan",
                "children": [{ "nodeType": "Unit test bundle", "name": name, "children": children }]
            }]
        }))
        .unwrap()
    }

    fn report(tests: Tests, attribution: FileAttribution) -> Report {
        let xcresult = XCResult {
            tests,
            org_url_slug: String::from("trunk"),
            repo_full_name: String::from("github.com/trunk-io/analytics-cli"),
            attribution,
            test_run_started_at: None,
        };
        let mut reports = xcresult.generate_junits();
        assert_eq!(reports.len(), 1);
        reports.pop().unwrap()
    }

    fn extra(test_case: &TestCase, key: &str) -> Option<String> {
        test_case
            .extra
            .iter()
            .find(|(name, _)| name.as_str() == key)
            .map(|(_, value)| value.as_str().to_owned())
    }

    fn suites_and_cases(report: &Report) -> Vec<(String, Vec<String>)> {
        report
            .test_suites
            .iter()
            .map(|test_suite| {
                (
                    test_suite.name.as_str().to_owned(),
                    test_suite
                        .test_cases
                        .iter()
                        .map(|test_case| test_case.name.as_str().to_owned())
                        .collect(),
                )
            })
            .collect()
    }

    fn file_of(report: &Report, name: &str) -> Option<String> {
        report
            .test_suites
            .iter()
            .flat_map(|test_suite| test_suite.test_cases.iter())
            .find(|test_case| test_case.name.as_str() == name)
            .and_then(|test_case| extra(test_case, "file"))
    }

    fn declarations() -> FileAttribution {
        FileAttribution::Declarations(
            TestLocationIndex::default().declaring("SnapshotReproTests/testExample()", TEST_FILE),
        )
    }

    // The capability the failure-summary paths cannot have at all: a test that never failed
    // has no summary, so there is nothing for them to read a path out of.
    #[rstest]
    #[case::passed("Passed")]
    #[case::skipped("Skipped")]
    #[case::expected_failure("Expected Failure")]
    fn a_test_case_that_did_not_fail_still_gets_its_declaration_file(#[case] result: &str) {
        let tests = bundle(
            "ExampleTests",
            vec![suite(
                "SnapshotReproTests",
                vec![case(
                    "testExample()",
                    "SnapshotReproTests/testExample()",
                    result,
                    vec![],
                )],
            )],
        );
        assert_eq!(
            file_of(&report(tests, declarations()), "testExample()").as_deref(),
            Some(TEST_FILE)
        );
    }

    #[test]
    fn a_passing_test_case_has_no_file_from_failure_summaries() {
        let tests = bundle(
            "ExampleTests",
            vec![suite(
                "SnapshotReproTests",
                vec![case(
                    "testExample()",
                    "SnapshotReproTests/testExample()",
                    "Passed",
                    vec![],
                )],
            )],
        );
        assert_eq!(
            file_of(
                &report(tests, FileAttribution::FailureSummaries(HashMap::new())),
                "testExample()"
            ),
            None
        );
    }

    // Whether the helper is in the repo or vendored, the raised-at location is not the test.
    #[rstest]
    #[case::in_repo_helper(HELPER_FILE)]
    #[case::vendored_dependency(DEPENDENCY_FILE)]
    fn a_failure_raised_elsewhere_is_still_attributed_to_the_test_file(#[case] raised_in: &str) {
        let tests = bundle(
            "ExampleTests",
            vec![suite(
                "SnapshotReproTests",
                vec![case(
                    "testExample()",
                    "SnapshotReproTests/testExample()",
                    "Failed",
                    vec![raised_at(raised_in)],
                )],
            )],
        );
        assert_eq!(
            file_of(&report(tests, declarations()), "testExample()").as_deref(),
            Some(TEST_FILE)
        );
    }

    // With no declaration to find — a runtime-registered test — the raised-at location is
    // all there is, and a vendored one must still be refused rather than reported.
    #[rstest]
    #[case::in_repo_helper_is_better_than_nothing(HELPER_FILE, Some(HELPER_FILE))]
    #[case::vendored_dependency_is_refused(DEPENDENCY_FILE, None)]
    fn an_unresolved_test_falls_back_to_the_raised_at_location(
        #[case] raised_in: &str,
        #[case] expected: Option<&str>,
    ) {
        let tests = bundle(
            "ExampleTests",
            vec![suite(
                "QuickSpec",
                vec![case(
                    "a calculator, fails on purpose()",
                    "QuickSpec/a calculator, fails on purpose()",
                    "Failed",
                    vec![raised_at(raised_in)],
                )],
            )],
        );
        assert_eq!(
            file_of(
                &report(
                    tests,
                    FileAttribution::Declarations(TestLocationIndex::default())
                ),
                "a calculator, fails on purpose()"
            )
            .as_deref(),
            expected
        );
    }
}
