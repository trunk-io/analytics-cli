use std::str;
use std::{
    collections::HashMap,
    ffi::OsStr,
    ops::{Deref, DerefMut},
};
use std::{fs, path::Path, time::Duration};

use petgraph::{
    graph::{DiGraph, NodeIndex},
    Direction::Incoming,
};
use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestRerun, TestSuite};
use schema::TestNode;

#[allow(dead_code, clippy::all)]
pub mod schema {
    include!(concat!(
        env!("OUT_DIR"),
        "/xcrun-xcresulttool-get-test-results-tests-json-schema.rs"
    ));
}

#[allow(dead_code, clippy::all)]
pub mod fd_schema {
    include!(concat!(
        env!("OUT_DIR"),
        "/xcrun-xcresulttool-formatDescription-get---format-json---legacy-json-schema.rs"
    ));
}

#[derive(Debug, Clone)]
pub struct XCResult {
    tests: schema::Tests,
    org_url_slug: String,
    repo_full_name: String,
    xcresult_tests: HashMap<String, XCResultTest>,
}

impl XCResult {
    pub fn new<T: AsRef<Path>>(
        path: T,
        org_url_slug: String,
        repo_full_name: String,
    ) -> anyhow::Result<XCResult> {
        let absolute_path = fs::canonicalize(path.as_ref()).map_err(|e| {
            anyhow::anyhow!(
                "failed to get absolute path for {}: {}",
                path.as_ref().display(),
                e
            )
        })?;
        // TODO
        let xcresult_tests = XCResultTest::generate_from_object(&absolute_path).unwrap_or_default();
        Ok(XCResult {
            tests: xcrun_cmd::xcresulttool_get_test_results_tests(&absolute_path)?,
            xcresult_tests,
            org_url_slug,
            repo_full_name,
        })
    }

    pub fn generate_junits(&self) -> Vec<Report> {
        self.xcresult_test_plans_to_junit_reports(self.tests.test_nodes.as_slice())
    }

    fn xcresult_test_plans_to_junit_reports(&self, test_nodes: &[schema::TestNode]) -> Vec<Report> {
        test_nodes
            .iter()
            .filter(|tn| matches!(tn.node_type, schema::TestNodeType::TestPlan))
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
        test_nodes: &[schema::TestNode],
    ) -> Vec<TestSuite> {
        test_nodes
            .iter()
            .filter(|tn| {
                matches!(
                    tn.node_type,
                    schema::TestNodeType::UnitTestBundle
                        | schema::TestNodeType::UiTestBundle
                        | schema::TestNodeType::TestSuite
                )
            })
            .flat_map(|test_bundle_or_test_suite| {
                if matches!(
                    test_bundle_or_test_suite.node_type,
                    schema::TestNodeType::UnitTestBundle | schema::TestNodeType::UiTestBundle
                ) {
                    let test_bundle = test_bundle_or_test_suite;
                    self.xcresult_test_suites_to_junit_test_suites(
                        test_bundle.children.as_slice(),
                        Some(&test_bundle.name),
                    )
                } else {
                    let test_suite = test_bundle_or_test_suite;
                    vec![self
                        .xcresult_test_suite_to_junit_test_suite(test_suite, Option::<&str>::None)]
                }
            })
            .collect()
    }

    fn xcresult_test_suites_to_junit_test_suites<T: AsRef<str>>(
        &self,
        test_nodes: &[schema::TestNode],
        bundle_name: Option<T>,
    ) -> Vec<TestSuite> {
        test_nodes
            .iter()
            .filter(|tn| matches!(tn.node_type, schema::TestNodeType::TestSuite))
            .map(|test_suite| {
                self.xcresult_test_suite_to_junit_test_suite(test_suite, bundle_name.as_ref())
            })
            .collect()
    }

    fn xcresult_test_suite_to_junit_test_suite<T: AsRef<str>>(
        &self,
        xcresult_test_suite: &schema::TestNode,
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

    fn xcresult_test_cases_to_junit_test_cases(
        &self,
        test_nodes: &[schema::TestNode],
    ) -> Vec<TestCase> {
        test_nodes
            .iter()
            .filter(|tn| matches!(tn.node_type, schema::TestNodeType::TestCase))
            .filter_map(|tn| tn.result.as_ref().map(|result| (tn, *result)))
            .filter_map(|(xcresult_test_case, test_result)| {
                let status = match test_result {
                    schema::TestResult::Passed | schema::TestResult::ExpectedFailure => {
                        TestCaseStatus::success()
                    }
                    schema::TestResult::Failed => {
                        TestCaseStatus::non_success(NonSuccessKind::Failure)
                    }
                    schema::TestResult::Skipped => TestCaseStatus::skipped(),
                    schema::TestResult::Unknown => {
                        // TODO: Add a warning
                        return None;
                    }
                };
                let mut test_case = TestCase::new(String::from(&xcresult_test_case.name), status);

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

                if let Some(node_identifier) = &xcresult_test_case.node_identifier {
                    let id = self.generate_id(node_identifier);
                    test_case.extra.insert("id".into(), id.into());
                }

                Some(test_case)
            })
            .collect()
    }

    fn xcresult_repetitions_to_junit_test_reruns(
        test_nodes: &[schema::TestNode],
    ) -> Vec<TestRerun> {
        test_nodes
            .iter()
            .filter(|tn| matches!(tn.node_type, schema::TestNodeType::Repetition))
            .filter_map(|tn| tn.result.as_ref().map(|result| (tn, *result)))
            .filter_map(|(repetition, test_result)| {
                let status = match test_result {
                    schema::TestResult::Passed | schema::TestResult::ExpectedFailure => {
                        // A successful repetition isn't relevant to JUnit test reruns
                        return None;
                    }
                    schema::TestResult::Failed => NonSuccessKind::Failure,
                    schema::TestResult::Skipped | schema::TestResult::Unknown => {
                        // TODO: Add a warning
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

    fn xcresult_failure_messages_to_strings(test_nodes: &[schema::TestNode]) -> Vec<String> {
        test_nodes
            .iter()
            .filter(|tn| matches!(tn.node_type, schema::TestNodeType::FailureMessage))
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
            .xcresult_tests
            .get(raw_id.as_ref())
            .map(|test| &test.identifier_url);
        // join the org and repo name to the raw id and generate uuid v5 from it
        uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_URL,
            format!(
                "{}#{}#{}",
                &self.org_url_slug,
                &self.repo_full_name,
                identifier_url
                    .map(|s| s.as_str())
                    .unwrap_or(raw_id.as_ref())
            )
            .as_bytes(),
        )
        .to_string()
    }
}

pub mod xcrun_cmd {
    use std::{ffi::OsStr, process::Command};

    use lazy_static::lazy_static;

    use crate::{fd_schema, schema};

    pub fn xcresulttool_get_test_results_tests<T: AsRef<OsStr>>(
        path: T,
    ) -> anyhow::Result<schema::Tests> {
        xcresulttool_min_version_check()?;

        let output = xcrun(&[
            "xcresulttool".as_ref(),
            "get".as_ref(),
            "test-results".as_ref(),
            "tests".as_ref(),
            "--path".as_ref(),
            path.as_ref(),
        ])?;

        serde_json::from_str::<schema::Tests>(&output)
            .map_err(|e| anyhow::anyhow!("failed to parse json from xcresulttool output: {}", e))
    }

    pub fn xcresulttool_get_object<T: AsRef<OsStr>>(
        path: T,
    ) -> anyhow::Result<fd_schema::ActionsInvocationRecord> {
        let mut args: Vec<&OsStr> = vec![
            "xcresulttool".as_ref(),
            "get".as_ref(),
            "object".as_ref(),
            "--format".as_ref(),
            "json".as_ref(),
            "--path".as_ref(),
            path.as_ref(),
        ];

        if xcresulttool_min_version_check().is_ok() {
            args.push("--legacy".as_ref());
        }

        let output = xcrun(&args)?;

        serde_json::from_str::<fd_schema::ActionsInvocationRecord>(&output)
            .map_err(|e| anyhow::anyhow!("failed to parse json from xcresulttool output: {}", e))
    }

    pub fn xcresulttool_get_object_id<T: AsRef<OsStr>, U: AsRef<OsStr>>(
        path: T,
        id: U,
    ) -> anyhow::Result<fd_schema::ActionTestPlanRunSummaries> {
        let mut args: Vec<&OsStr> = vec![
            "xcresulttool".as_ref(),
            "get".as_ref(),
            "object".as_ref(),
            "--format".as_ref(),
            "json".as_ref(),
            "--id".as_ref(),
            id.as_ref(),
            "--path".as_ref(),
            path.as_ref(),
        ];

        if xcresulttool_min_version_check().is_ok() {
            args.push("--legacy".as_ref());
        }

        let output = xcrun(&args)?;

        serde_json::from_str::<fd_schema::ActionTestPlanRunSummaries>(&output)
            .map_err(|e| anyhow::anyhow!("failed to parse json from xcresulttool output: {}", e))
    }

    const LEGACY_FLAG_MIN_VERSION: usize = 22608;
    fn xcresulttool_min_version_check() -> anyhow::Result<()> {
        let version = xcresulttool_version()?;
        if version <= LEGACY_FLAG_MIN_VERSION {
            return Err(anyhow::anyhow!(
                "xcresulttool version {} is not supported, please upgrade to version {} or higher",
                version,
                LEGACY_FLAG_MIN_VERSION
            ));
        }
        Ok(())
    }

    fn xcresulttool_version() -> anyhow::Result<usize> {
        let version_raw = xcrun(&["xcresulttool", "version"])?;

        lazy_static! {
            // regex to match version where the output looks like "xcresulttool version 22608, format version 3.49 (current)"
            static ref RE: regex::Regex = regex::Regex::new(r"xcresulttool version (\d+)").unwrap();
        }
        let version_parsed = RE
            .captures(&version_raw)
            .and_then(|capture_group| capture_group.get(1))
            .and_then(|version| version.as_str().parse::<usize>().ok());

        if let Some(version) = version_parsed {
            Ok(version)
        } else {
            Err(anyhow::anyhow!("failed to parse xcresulttool version"))
        }
    }

    fn xcrun<T: AsRef<OsStr>>(args: &[T]) -> anyhow::Result<String> {
        if !cfg!(target_os = "macos") {
            return Err(anyhow::anyhow!("xcrun is only available on macOS"));
        }
        let output = Command::new("xcrun").args(args).output()?;
        let data = if output.status.code() == Some(0) {
            output.stdout
        } else {
            output.stderr
        };
        let result = String::from_utf8(data)?;
        Ok(result)
    }
}

#[derive(Debug, Clone, Default)]
pub struct XCResultTest {
    pub test_plan_name: String,
    pub test_bundle_name: String,
    pub test_suite_name: String,
    pub test_case_name: String,
    pub identifier_url: String,
    pub identifier: String,
}

// TODO (log and cleanup)
impl XCResultTest {
    pub fn generate_from_object<T: AsRef<OsStr>>(path: T) -> anyhow::Result<HashMap<String, Self>> {
        let actions_invocation_record = xcrun_cmd::xcresulttool_get_object(path.as_ref())?;
        let test_plans = actions_invocation_record
            .actions
            .as_ref()
            .map(|arr| arr.values.iter())
            .unwrap_or_default()
            .filter_map(|action_record| {
                if let fd_schema::ActionRecord {
                    test_plan_name:
                        Some(fd_schema::String {
                            value: test_plan_name,
                            ..
                        }),
                    action_result:
                        fd_schema::ActionResult {
                            tests_ref:
                                Some(fd_schema::Reference {
                                    id: Some(fd_schema::String { value: id, .. }),
                                    ..
                                }),
                            ..
                        },
                    ..
                } = action_record
                {
                    Some((test_plan_name, id))
                } else {
                    None
                }
            })
            .flat_map(|(test_plan_name, id)| {
                xcrun_cmd::xcresulttool_get_object_id(&path, id).ok().map(
                    |action_test_plan_run_summaries| {
                        (test_plan_name, action_test_plan_run_summaries)
                    },
                )
            })
            .collect::<Vec<_>>();

        Ok(test_plans
            .iter()
            .filter_map(|(test_plan_name, action_test_plan_run_summaries)| {
                action_test_plan_run_summaries.summaries.as_ref().map(
                    |action_test_plan_run_summaries_summaries| {
                        (test_plan_name, action_test_plan_run_summaries_summaries)
                    },
                )
            })
            .flat_map(
                |(test_plan_name, action_test_plan_run_summaries_summaries)| {
                    action_test_plan_run_summaries_summaries.values.iter().map(
                        move |action_test_plan_run_summary| {
                            (test_plan_name, action_test_plan_run_summary)
                        },
                    )
                },
            )
            .filter_map(|(test_plan_name, action_test_plan_run_summary)| {
                action_test_plan_run_summary
                    .testable_summaries
                    .as_ref()
                    .map(|action_test_plan_run_summary_testable_summaries| {
                        (
                            test_plan_name,
                            action_test_plan_run_summary_testable_summaries,
                        )
                    })
            })
            .flat_map(
                |(test_plan_name, action_test_plan_run_summary_testable_summaries)| {
                    action_test_plan_run_summary_testable_summaries
                        .values
                        .iter()
                        .map(move |action_testable_summary| {
                            (test_plan_name, action_testable_summary)
                        })
                },
            )
            .filter_map(|(test_plan_name, action_testable_summary)| {
                if let fd_schema::ActionTestableSummary {
                    name:
                        Some(fd_schema::String {
                            value: action_testable_summary_name,
                            ..
                        }),
                    tests:
                        Some(fd_schema::ActionTestableSummaryTests {
                            values: action_test_summary_identifiable_objects,
                            ..
                        }),
                    ..
                } = &action_testable_summary
                {
                    Some((
                        test_plan_name,
                        action_testable_summary_name,
                        action_test_summary_identifiable_objects,
                    ))
                } else {
                    None
                }
            })
            .flat_map(
                |(
                    test_plan_name,
                    action_testable_summary_name,
                    action_test_summary_identifiable_objects,
                )| {
                    let mut xc_result_test_node_tree = XCResultTestNodeTree::default();
                    xc_result_test_node_tree
                        .traverse(action_test_summary_identifiable_objects, None);

                    let leafs = xc_result_test_node_tree.externals(petgraph::Direction::Outgoing);
                    let raw_nodes = xc_result_test_node_tree.raw_nodes();
                    let raw_edges = xc_result_test_node_tree.raw_edges();
                    // dbg!(&leafs);
                    leafs
                        .filter_map(|leaf| {
                            // filter out any dangling leafs
                            if leaf.index() >= raw_nodes.len() {
                                return None;
                            }
                            let node = &raw_nodes[leaf.index()];
                            let next_idx = node.next_edge(Incoming).index();
                            if next_idx >= raw_edges.len() {
                                return None;
                            }
                            let edge = &raw_edges[next_idx];
                            if edge.source().index() >= raw_nodes.len() {
                                return None;
                            }
                            let parent_node = &raw_nodes[edge.source().index()];

                            let test_suite_name = parent_node.weight.name;
                            let test_case_name = node.weight.name;

                            Some(Self {
                                test_plan_name: String::from(*test_plan_name),
                                test_bundle_name: String::from(action_testable_summary_name),
                                test_suite_name: String::from(test_suite_name),
                                test_case_name: String::from(test_case_name),
                                identifier_url: String::from(node.weight.identifier_url),
                                identifier: String::from(node.weight.identifier),
                            })
                        })
                        .collect::<Vec<_>>()
                },
            )
            .map(|test| (test.identifier.clone(), test))
            .collect::<HashMap<_, _>>())
    }
}

#[derive(Debug, Clone, Default, PartialEq, PartialOrd, Eq, Ord, Hash)]
struct XCResultTestNodeRef<'a> {
    name: &'a str,
    identifier: &'a str,
    identifier_url: &'a str,
}

#[derive(Debug, Clone, Default)]
struct XCResultTestNodeTree<'a>(DiGraph<XCResultTestNodeRef<'a>, ()>);

impl<'a> Deref for XCResultTestNodeTree<'a> {
    type Target = DiGraph<XCResultTestNodeRef<'a>, ()>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> DerefMut for XCResultTestNodeTree<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'a> XCResultTestNodeTree<'a> {
    fn traverse(
        &mut self,
        action_test_summary_identifiable_objects: &'a [fd_schema::ActionTestSummaryIdentifiableObject],
        parent_node: Option<NodeIndex>,
    ) {
        for action_test_summary_identifiable_object in
            action_test_summary_identifiable_objects.as_ref().iter()
        {
            match action_test_summary_identifiable_object {
                fd_schema::ActionTestSummaryIdentifiableObject::Variant0(
                    fd_schema::ActionTestSummary {
                        name: Some(fd_schema::String { value: name, .. }),
                        identifier:
                            Some(fd_schema::String {
                                value: identifier, ..
                            }),
                        identifier_url:
                            Some(fd_schema::String {
                                value: identifier_url,
                                ..
                            }),
                        ..
                    },
                ) => {
                    let test_node = XCResultTestNodeRef {
                        name,
                        identifier,
                        identifier_url,
                    };
                    let node_index = self.add_node(test_node);
                    if let Some(parent_node) = parent_node {
                        self.add_edge(parent_node, node_index, ());
                    }
                }
                fd_schema::ActionTestSummaryIdentifiableObject::Variant1(
                    fd_schema::ActionTestMetadata {
                        name: Some(fd_schema::String { value: name, .. }),
                        identifier:
                            Some(fd_schema::String {
                                value: identifier, ..
                            }),
                        identifier_url:
                            Some(fd_schema::String {
                                value: identifier_url,
                                ..
                            }),
                        ..
                    },
                ) => {
                    let test_node = XCResultTestNodeRef {
                        name,
                        identifier,
                        identifier_url,
                    };
                    let node_index = self.add_node(test_node);
                    if let Some(parent_node) = parent_node {
                        self.add_edge(parent_node, node_index, ());
                    }
                }
                fd_schema::ActionTestSummaryIdentifiableObject::Variant2(
                    fd_schema::ActionTestSummaryGroup {
                        name: Some(fd_schema::String { value: name, .. }),
                        identifier:
                            Some(fd_schema::String {
                                value: identifier, ..
                            }),
                        identifier_url:
                            Some(fd_schema::String {
                                value: identifier_url,
                                ..
                            }),
                        subtests,
                        ..
                    },
                ) => {
                    let test_node = XCResultTestNodeRef {
                        name,
                        identifier,
                        identifier_url,
                    };
                    let node_index = self.add_node(test_node);
                    if let Some(ref subtests) = subtests {
                        self.traverse(&subtests.values, Some(node_index));
                    }
                    if let Some(parent_node) = parent_node {
                        self.add_edge(parent_node, node_index, ());
                    }
                }
                fd_schema::ActionTestSummaryIdentifiableObject::Variant3 {
                    identifier_url:
                        Some(fd_schema::String {
                            value: identifier_url,
                            ..
                        }),
                    identifier:
                        Some(fd_schema::String {
                            value: identifier, ..
                        }),
                    name: Some(fd_schema::String { value: name, .. }),
                    ..
                } => {
                    let test_node = XCResultTestNodeRef {
                        name,
                        identifier,
                        identifier_url,
                    };
                    let node_index = self.add_node(test_node);
                    if let Some(parent_node) = parent_node {
                        self.add_edge(parent_node, node_index, ());
                    }
                }
                fd_schema::ActionTestSummaryIdentifiableObject::Variant0(..)
                | fd_schema::ActionTestSummaryIdentifiableObject::Variant1(..)
                | fd_schema::ActionTestSummaryIdentifiableObject::Variant2(..)
                | fd_schema::ActionTestSummaryIdentifiableObject::Variant3 { .. } => {
                    // do nothing
                }
            }
        }
    }
}
