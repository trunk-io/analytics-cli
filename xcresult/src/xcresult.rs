use std::collections::HashMap;
use std::str;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{fs, path::Path, path::PathBuf, time::Duration};

use chrono::{DateTime, Utc};
use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestRerun, TestSuite};
use tempfile::TempDir;

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

/// `xcresulttool` migrates an older bundle in place on first read, writing into a directory
/// we were only asked to read and failing outright when it is not writable.
fn copy_bundle(path: &Path) -> anyhow::Result<(TempDir, PathBuf)> {
    fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            let destination = to.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                copy_dir(&entry.path(), &destination)?;
            } else {
                fs::copy(entry.path(), destination)?;
            }
        }
        Ok(())
    }

    let temp_dir = TempDir::new()?;
    let destination = temp_dir.path().join(
        path.file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("bundle.xcresult")),
    );
    copy_dir(path, &destination)
        .map_err(|e| anyhow::anyhow!("failed to copy {} for reading: {}", path.display(), e))?;
    Ok((temp_dir, destination))
}

/// Makes it visible how many tests the checkout could not account for.
#[derive(Debug, Default)]
struct AttributionCounts {
    declared: AtomicUsize,
    unresolved: AtomicUsize,
}

#[derive(Debug)]
pub struct XCResult {
    tests: Tests,
    org_url_slug: String,
    repo_full_name: String,
    attribution: FileAttribution,
    test_run_started_at: Option<DateTime<Utc>>,
    counts: AttributionCounts,
    _bundle_copy: TempDir,
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
        let (bundle_copy, absolute_path) = copy_bundle(&absolute_path)?;

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
            counts: AttributionCounts::default(),
            _bundle_copy: bundle_copy,
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
        let (bundle_copy, absolute_path) = copy_bundle(&absolute_path)?;
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
            counts: AttributionCounts::default(),
            _bundle_copy: bundle_copy,
        })
    }

    pub fn generate_junits(&self) -> Vec<Report> {
        let reports: Vec<Report> = self
            .tests
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
            .collect();
        if matches!(self.attribution, FileAttribution::Declarations(_)) {
            tracing::info!(
                "xcresult test files: {} from a declaration, {} with no declaration found",
                self.counts.declared.load(Ordering::Relaxed),
                self.counts.unresolved.load(Ordering::Relaxed),
            );
        }
        reports
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
                    self.xcresult_test_suite_to_junit_test_suites(test_suite, None)
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
        let qualifier = bundle_name.as_ref().map(|bn| bn.as_ref());
        let mut test_suites = test_nodes
            .iter()
            .filter(|tn| matches!(tn.node_type, TestNodeType::TestSuite))
            .flat_map(|test_suite| {
                self.xcresult_test_suite_to_junit_test_suites(test_suite, qualifier)
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

    /// A suite and, flattened after it, every suite nested inside it — JUnit has no nested
    /// `<testsuite>`, and emitting only the outer one drops the tests the inner ones declare.
    fn xcresult_test_suite_to_junit_test_suites(
        &self,
        xcresult_test_suite: &TestNode,
        qualifier: Option<&str>,
    ) -> Vec<TestSuite> {
        let name = qualifier
            .map(|qualifier| format!("{}.{}", qualifier, xcresult_test_suite.name))
            .unwrap_or_else(|| String::from(&xcresult_test_suite.name));
        let mut test_suite = TestSuite::new(name.clone());
        test_suite.add_test_cases(
            self.xcresult_test_cases_to_junit_test_cases(xcresult_test_suite.children.as_slice()),
        );
        let mut test_suites = vec![test_suite];
        test_suites.extend(
            xcresult_test_suite
                .children
                .iter()
                .filter(|tn| matches!(tn.node_type, TestNodeType::TestSuite))
                .flat_map(|nested| {
                    self.xcresult_test_suite_to_junit_test_suites(nested, Some(&name))
                }),
        );
        test_suites
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
                    self.counts.declared.fetch_add(1, Ordering::Relaxed);
                    return Some(site.file.as_str().to_owned());
                }
                // Where a failure surfaced is not where the test is written, and reporting
                // it hands the test to whoever owns that file. No file at all resolves no
                // codeowners, which is recoverable; the wrong file is not. A test with no
                // declaration to find is runtime-registered (Quick, `+testInvocations`).
                self.counts.unresolved.fetch_add(1, Ordering::Relaxed);
                tracing::debug!("no declaration in the checkout for {node_identifier}");
                None
            }
        }
    }
}

fn collect_test_keys(test_nodes: &[TestNode], keys: &mut Vec<(TestKey, Option<String>)>) {
    for test_node in test_nodes {
        if matches!(test_node.node_type, TestNodeType::TestCase)
            && let Some(node_identifier) = &test_node.node_identifier
        {
            let target = test_node
                .node_identifier_url
                .as_deref()
                .and_then(TestKey::target_from_identifier_url);
            keys.push((TestKey::from_node_identifier(node_identifier), target));
        }
        collect_test_keys(&test_node.children, keys);
    }
}
