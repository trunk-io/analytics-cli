use std::collections::HashMap;
use std::str;
use std::{fs, path::Path, time::Duration};

use chrono::{DateTime, Utc};
use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestRerun, TestSuite};

use crate::types::{
    schema::{TestNode, TestNodeType, TestResult, Tests},
    SWIFT_DEFAULT_TEST_SUITE_NAME,
};
use crate::xcresult_legacy::XCResultTestLegacy;
use crate::xcrun::{xcresulttool_get_object, xcresulttool_get_test_results_tests};

#[derive(Debug, Clone)]
pub struct XCResult {
    tests: Tests,
    org_url_slug: String,
    repo_full_name: String,
    legacy_xcresult_tests: HashMap<String, XCResultTestLegacy>,
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
            legacy_xcresult_tests,
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
                    vec![self
                        .xcresult_test_suite_to_junit_test_suite(test_suite, Option::<&str>::None)]
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

                if let Some(node_identifier) = &xcresult_test_case.node_identifier {
                    let id = self.generate_id(node_identifier);
                    test_case.extra.insert("id".into(), id.into());
                    let file = self.find_test_case_file(node_identifier);
                    if let Some(file) = file {
                        test_case.extra.insert("file".into(), file.into());
                    }
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

    fn generate_id<T: AsRef<str>>(&self, raw_id: T) -> String {
        let identifier_url = self
            .legacy_xcresult_tests
            .get(raw_id.as_ref())
            .map(|test| &test.identifier_url)
            .map(|identifier_url| identifier_url.as_str());
        // join the org and repo name to the raw id and generate uuid v5 from it
        uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_URL,
            format!(
                "{}#{}#{}",
                &self.org_url_slug,
                &self.repo_full_name,
                identifier_url.unwrap_or(raw_id.as_ref())
            )
            .as_bytes(),
        )
        .to_string()
    }

    fn find_test_case_file<T: AsRef<str>>(&self, raw_id: T) -> Option<String> {
        if let Some(file) = self
            .legacy_xcresult_tests
            .get(raw_id.as_ref())
            .map(|test| &test.file)
            .and_then(|file| file.as_ref())
        {
            return Some(file.to_owned());
        }
        None
    }
}
