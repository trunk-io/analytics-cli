use std::{
    collections::HashMap,
    ffi::OsStr,
    ops::{Deref, DerefMut},
};

use petgraph::{
    Direction::Incoming,
    graph::{DiGraph, NodeIndex},
};

use crate::file_attribution::{FileCandidate, TestIdentity};
use crate::types::{SWIFT_DEFAULT_TEST_SUITE_NAME, legacy_schema};
use crate::xcrun::{xcresulttool_get_object, xcresulttool_get_object_id};

#[derive(Debug, Clone, Default)]
pub struct XCResultTestLegacy {
    pub test_plan_name: String,
    pub test_bundle_name: String,
    pub test_suite_name: String,
    pub test_case_name: String,
    pub identifier_url: String,
    pub identifier: String,
    pub file: Option<String>,
}

impl XCResultTestLegacy {
    fn find_file_in_test_summary(
        failure_summary_id: &str,
        path: &OsStr,
        identity: &TestIdentity,
    ) -> Option<String> {
        let summary = xcresulttool_get_object_id(path, failure_summary_id);
        summary.ok().and_then(|summary| {
            summary
                .failure_summaries
                .as_ref()
                .and_then(|failure_summaries| {
                    // grab the first failure summary if there are multiple
                    failure_summaries.values.first()
                })
                .and_then(|failure_summary| {
                    Self::find_file_in_failure_summary(failure_summary, identity)
                })
        })
    }

    /// The file to report for a failure: the first candidate we are willing to
    /// stand behind.
    ///
    /// Only the test's own frame identifies the test; every other source says where
    /// the failure surfaced, which for a snapshot trait, a mocking framework or a
    /// page object is inside the dependency. Those are vetted here — in one place,
    /// so a source added later cannot quietly skip the check — and a test that
    /// crashes or fails to launch never reaches its own frame, leaving nothing
    /// reportable. We then report no file at all rather than one that would hand the
    /// test to whoever owns the vendored directory. Consumers treat a missing file
    /// as "unchanged" rather than "cleared", so it keeps the path and owners it had.
    fn find_file_in_failure_summary(
        failure_summary: &legacy_schema::ActionTestFailureSummary,
        identity: &TestIdentity,
    ) -> Option<String> {
        FileCandidate::from_failure_summary(failure_summary, identity)
            .into_iter()
            .find(Self::is_reportable)
            .map(|candidate| candidate.path.into_string())
    }

    fn is_reportable(candidate: &FileCandidate) -> bool {
        candidate.source.is_positive_identification() || !candidate.path.is_vendored_dependency()
    }

    /// The action-level issue summaries keyed by whatever names the test they
    /// belong to, which is the producing target when Xcode records one and the test
    /// case name otherwise.
    fn fallback_file_from_failure_issue_summary(
        failure_summary: &legacy_schema::TestFailureIssueSummary,
    ) -> Option<(Option<&str>, String)> {
        let candidate =
            FileCandidate::from_issue_summary(failure_summary).filter(Self::is_reportable)?;
        let producing_target = failure_summary
            .producing_target
            .as_ref()
            .map(|x| x.value.as_ref());
        let key = producing_target.or_else(|| {
            failure_summary
                .test_case_name
                .as_ref()
                .map(|x| x.value.as_ref())
        });
        Some((key, candidate.path.into_string()))
    }

    fn find_fallback_file<'a>(
        files: &HashMap<Option<&'a str>, String>,
        test_suite_name: Option<&str>,
        formatted_test_case_name: &str,
    ) -> Option<String> {
        files
            .get(&test_suite_name)
            .or_else(|| files.get(&Some(formatted_test_case_name)))
            .cloned()
    }

    pub fn generate_from_object<T: AsRef<OsStr>>(
        path: T,
        use_experimental_failure_summary: bool,
    ) -> anyhow::Result<HashMap<String, Self>> {
        let actions_invocation_record = xcresulttool_get_object(path.as_ref())?;
        Self::generate_from_record(
            path,
            actions_invocation_record,
            use_experimental_failure_summary,
        )
    }

    pub fn generate_from_record<T: AsRef<OsStr>>(
        path: T,
        actions_invocation_record: legacy_schema::ActionsInvocationRecord,
        use_experimental_failure_summary: bool,
    ) -> anyhow::Result<HashMap<String, Self>> {
        let test_plans = actions_invocation_record
            .actions
            .as_ref()
            .map(|arr| arr.values.iter())
            .unwrap_or_default()
            .filter_map(|action_record| {
                if let legacy_schema::ActionRecord {
                    action_result:
                        legacy_schema::ActionResult {
                            tests_ref:
                                Some(legacy_schema::Reference {
                                    id: Some(legacy_schema::String { value: id, .. }),
                                    ..
                                }),
                            issues,
                            ..
                        },
                    ..
                } = action_record
                {
                    let failure_summaries = issues.test_failure_summaries.as_ref();
                    let test_plan_name = action_record
                        .test_plan_name
                        .as_ref()
                        .map(|name| name.value.as_ref())
                        .unwrap_or("unspecified");
                    Some((test_plan_name, id, failure_summaries))
                } else {
                    None
                }
            })
            .flat_map(|(test_plan_name, id, failure_summaries)| {
                xcresulttool_get_object_id(&path, id)
                    .ok()
                    .map(|action_test_plan_run_summaries| {
                        (
                            test_plan_name,
                            action_test_plan_run_summaries,
                            failure_summaries,
                        )
                    })
            })
            .collect::<Vec<_>>();

        Ok(test_plans
            .iter()
            .filter_map(
                |(test_plan_name, action_test_plan_run_summaries, failure_summaries)| {
                    action_test_plan_run_summaries.summaries.as_ref().map(
                        |action_test_plan_run_summaries_summaries| {
                            (
                                test_plan_name,
                                action_test_plan_run_summaries_summaries,
                                failure_summaries,
                            )
                        },
                    )
                },
            )
            .flat_map(
                |(test_plan_name, action_test_plan_run_summaries_summaries, failure_summaries)| {
                    action_test_plan_run_summaries_summaries.values.iter().map(
                        move |action_test_plan_run_summary| {
                            (
                                test_plan_name,
                                action_test_plan_run_summary,
                                failure_summaries,
                            )
                        },
                    )
                },
            )
            .filter_map(
                |(test_plan_name, action_test_plan_run_summary, failure_summaries)| {
                    action_test_plan_run_summary
                        .testable_summaries
                        .as_ref()
                        .map(|action_test_plan_run_summary_testable_summaries| {
                            (
                                test_plan_name,
                                action_test_plan_run_summary_testable_summaries,
                                failure_summaries,
                            )
                        })
                },
            )
            .flat_map(
                |(
                    test_plan_name,
                    action_test_plan_run_summary_testable_summaries,
                    failure_summaries,
                )| {
                    action_test_plan_run_summary_testable_summaries
                        .values
                        .iter()
                        .map(move |action_testable_summary| {
                            (test_plan_name, action_testable_summary, failure_summaries)
                        })
                },
            )
            .filter_map(
                |(test_plan_name, action_testable_summary, failure_summaries)| {
                    if let legacy_schema::ActionTestableSummary {
                        name:
                            Some(legacy_schema::String {
                                value: action_testable_summary_name,
                                ..
                            }),
                        tests:
                            Some(legacy_schema::ActionTestableSummaryTests {
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
                            failure_summaries,
                        ))
                    } else {
                        None
                    }
                },
            )
            .flat_map(
                |(
                    test_plan_name,
                    action_testable_summary_name,
                    action_test_summary_identifiable_objects,
                    failure_summaries,
                )| {
                    let mut xc_result_test_node_tree = XCResultTestLegacyNodeTree::default();
                    xc_result_test_node_tree
                        .traverse(action_test_summary_identifiable_objects, None);

                    let leafs = xc_result_test_node_tree.externals(petgraph::Direction::Outgoing);
                    let raw_nodes = xc_result_test_node_tree.raw_nodes();
                    let raw_edges = xc_result_test_node_tree.raw_edges();
                    let files = failure_summaries.as_ref().map(|failure_summaries| {
                        failure_summaries
                            .values
                            .iter()
                            .flat_map(Self::fallback_file_from_failure_issue_summary)
                            .collect::<HashMap<_, _>>()
                    });
                    leafs
                        .filter_map(|leaf| {
                            // filter out any dangling leafs
                            if leaf.index() >= raw_nodes.len() {
                                return None;
                            }
                            let node = &raw_nodes[leaf.index()];
                            let next_idx = node.next_edge(Incoming).index();
                            let edge = if next_idx < raw_edges.len() {
                                Some(&raw_edges[next_idx])
                            } else {
                                None
                            };
                            let parent_node = if let Some(edge) = edge {
                                if edge.source().index() < raw_nodes.len() {
                                    Some(&raw_nodes[edge.source().index()])
                                } else {
                                    None
                                }
                            } else {
                                None
                            };

                            let test_suite_name = parent_node.map(|node| node.weight.name);
                            let test_case_name = node.weight.name;
                            let identity = TestIdentity {
                                suite: test_suite_name,
                                case: test_case_name,
                            };
                            let formatted_test_case_name = identity.fallback_key();
                            let failure_summary_id = node.weight.failure_summary_id;
                            let mut file = if use_experimental_failure_summary
                                && failure_summary_id.is_some()
                            {
                                Self::find_file_in_test_summary(
                                    failure_summary_id.unwrap_or_default(),
                                    path.as_ref(),
                                    &identity,
                                )
                            } else {
                                None
                            };
                            if file.is_none() {
                                file = files.as_ref().and_then(|files| {
                                    Self::find_fallback_file(
                                        files,
                                        test_suite_name,
                                        &formatted_test_case_name,
                                    )
                                })
                            }

                            Some(Self {
                                test_plan_name: String::from(*test_plan_name),
                                test_bundle_name: String::from(action_testable_summary_name),
                                test_suite_name: String::from(
                                    test_suite_name.unwrap_or(SWIFT_DEFAULT_TEST_SUITE_NAME),
                                ),
                                test_case_name: String::from(test_case_name),
                                identifier_url: String::from(node.weight.identifier_url),
                                identifier: String::from(node.weight.identifier),
                                file,
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
struct XCResultTestLegacyNodeRef<'a> {
    name: &'a str,
    identifier: &'a str,
    identifier_url: &'a str,
    failure_summary_id: Option<&'a str>,
}

#[derive(Debug, Clone, Default)]
struct XCResultTestLegacyNodeTree<'a>(DiGraph<XCResultTestLegacyNodeRef<'a>, ()>);

impl<'a> Deref for XCResultTestLegacyNodeTree<'a> {
    type Target = DiGraph<XCResultTestLegacyNodeRef<'a>, ()>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> DerefMut for XCResultTestLegacyNodeTree<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'a> XCResultTestLegacyNodeTree<'a> {
    fn traverse(
        &mut self,
        action_test_summary_identifiable_objects: &'a [legacy_schema::ActionTestSummaryIdentifiableObject],
        parent_node: Option<NodeIndex>,
    ) {
        for action_test_summary_identifiable_object in
            action_test_summary_identifiable_objects.as_ref().iter()
        {
            match action_test_summary_identifiable_object {
                legacy_schema::ActionTestSummaryIdentifiableObject::Variant0(
                    legacy_schema::ActionTestMetadata {
                        name: Some(legacy_schema::String { value: name, .. }),
                        identifier:
                            Some(legacy_schema::String {
                                value: identifier, ..
                            }),
                        identifier_url:
                            Some(legacy_schema::String {
                                value: identifier_url,
                                ..
                            }),
                        test_status:
                            Some(legacy_schema::String {
                                value: test_status, ..
                            }),
                        summary_ref:
                            Some(legacy_schema::Reference {
                                id:
                                    Some(legacy_schema::String {
                                        value: summary_id, ..
                                    }),
                                ..
                            }),
                        ..
                    },
                ) => {
                    let test_node = XCResultTestLegacyNodeRef {
                        name,
                        identifier,
                        identifier_url,
                        failure_summary_id: if test_status != "Success" {
                            Some(summary_id)
                        } else {
                            None
                        },
                    };
                    let node_index = self.add_node(test_node);
                    if let Some(parent_node) = parent_node {
                        self.add_edge(parent_node, node_index, ());
                    }
                }
                legacy_schema::ActionTestSummaryIdentifiableObject::Variant1(
                    legacy_schema::ActionTestSummaryGroup {
                        name: Some(legacy_schema::String { value: name, .. }),
                        identifier:
                            Some(legacy_schema::String {
                                value: identifier, ..
                            }),
                        identifier_url:
                            Some(legacy_schema::String {
                                value: identifier_url,
                                ..
                            }),
                        subtests,
                        ..
                    },
                ) => {
                    let test_node = XCResultTestLegacyNodeRef {
                        name,
                        identifier,
                        identifier_url,
                        failure_summary_id: None,
                    };
                    let node_index = self.add_node(test_node);
                    if let Some(subtests) = &subtests {
                        self.traverse(&subtests.values, Some(node_index));
                    }
                    if let Some(parent_node) = parent_node {
                        self.add_edge(parent_node, node_index, ());
                    }
                }
                legacy_schema::ActionTestSummaryIdentifiableObject::Variant2(
                    legacy_schema::ActionTestSummary {
                        name: Some(legacy_schema::String { value: name, .. }),
                        identifier:
                            Some(legacy_schema::String {
                                value: identifier, ..
                            }),
                        identifier_url:
                            Some(legacy_schema::String {
                                value: identifier_url,
                                ..
                            }),
                        ..
                    },
                ) => {
                    let test_node = XCResultTestLegacyNodeRef {
                        name,
                        identifier,
                        identifier_url,
                        failure_summary_id: None,
                    };
                    let node_index = self.add_node(test_node);
                    if let Some(parent_node) = parent_node {
                        self.add_edge(parent_node, node_index, ());
                    }
                }
                legacy_schema::ActionTestSummaryIdentifiableObject::Variant3 {
                    identifier_url:
                        Some(legacy_schema::String {
                            value: identifier_url,
                            ..
                        }),
                    identifier:
                        Some(legacy_schema::String {
                            value: identifier, ..
                        }),
                    name: Some(legacy_schema::String { value: name, .. }),
                    ..
                } => {
                    let test_node = XCResultTestLegacyNodeRef {
                        name,
                        identifier,
                        identifier_url,
                        failure_summary_id: None,
                    };
                    let node_index = self.add_node(test_node);
                    if let Some(parent_node) = parent_node {
                        self.add_edge(parent_node, node_index, ());
                    }
                }
                legacy_schema::ActionTestSummaryIdentifiableObject::Variant0(..)
                | legacy_schema::ActionTestSummaryIdentifiableObject::Variant1(..)
                | legacy_schema::ActionTestSummaryIdentifiableObject::Variant2(..)
                | legacy_schema::ActionTestSummaryIdentifiableObject::Variant3 { .. } => {
                    tracing::debug!("Skipping {:?}", action_test_summary_identifiable_object);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::{Value, json};

    use super::*;

    fn xc_string(value: &str) -> Value {
        json!({ "_value": value })
    }

    const TEST_SUITE: &str = "SnapshotReproTests";
    const TEST_CASE: &str = "failingSnapshot()";

    #[rstest]
    // The test's own frame beats the file the failure was raised from, which here is
    // the assertion helper inside the dependency.
    #[case::test_frame_wins_over_raised_from_file(
        Some("/repo/Tests/Assertion.swift"),
        Some("/repo/Tests/Assertion.swift"),
        &[
            ("assertSnapshot<A, B>(of:as:)", "/repo/Tuist/.build/checkouts/swift-snapshot-testing/Assert.swift"),
            ("SnapshotReproTests.failingSnapshot()", "/repo/Tests/SnapshotReproTests.swift"),
            ("closure #1 in _SnapshotsTestTrait.provideScope(for:)", "/repo/Tuist/.build/checkouts/swift-snapshot-testing/Trait.swift"),
        ],
        Some("/repo/Tests/SnapshotReproTests.swift")
    )]
    #[case::objc_symbol_and_closure_frames_name_the_test(
        None,
        None,
        &[("closure #1 in -[SnapshotReproTests failingSnapshot]", "/repo/Tests/SnapshotReproTests.m")],
        Some("/repo/Tests/SnapshotReproTests.m")
    )]
    #[case::file_name_wins_when_no_frame_names_the_test(
        Some("/repo/Tests/My Test.swift"),
        Some("/repo/Tests/Other.swift"),
        &[],
        Some("/repo/Tests/My%20Test.swift")
    )]
    #[case::location_before_call_stack(
        None,
        Some("/repo/Tests/Assertion.swift"),
        &[("provideScope(for:)", "/repo/Packages/SnapshotTesting/SnapshotsTestTrait.swift")],
        Some("/repo/Tests/Assertion.swift")
    )]
    #[case::dependency_file_name_falls_through_to_location(
        Some("/repo/Tuist/.build/checkouts/UITestSupport/PageObject.swift"),
        Some("/repo/Tests/Assertion.swift"),
        &[],
        Some("/repo/Tests/Assertion.swift")
    )]
    #[case::last_swift_or_objc_stack_frame(
        None,
        None,
        &[
            ("first", "/repo/Tests/Generated.cc"),
            ("second", "/repo/Tests/First.swift"),
            ("third", "/repo/Tests/Second.m"),
            ("fourth", "/repo/Tests/Readme.md"),
        ],
        Some("/repo/Tests/Second.m")
    )]
    // The outermost frame is a dependency, so the next one out is taken instead.
    #[case::dependency_frames_skipped_in_stack_fallback(
        None,
        None,
        &[
            ("first", "/repo/Tests/First.swift"),
            ("second", "/repo/Tuist/.build/checkouts/UITestSupport/Launching.swift"),
        ],
        Some("/repo/Tests/First.swift")
    )]
    // A launch failure or crash never reaches the test's own frame, so every
    // remaining source points into the dependency: report no file rather than one
    // that would re-own the test.
    #[case::only_dependency_sources_yields_nothing(
        None,
        Some("/repo/DerivedData/SourcePackages/checkouts/UITestSupport/Launching.swift"),
        &[("launch", "/repo/Tuist/.build/checkouts/UITestSupport/Launching.swift")],
        None
    )]
    #[case::no_usable_file(
        None,
        None,
        &[("first", "/repo/Tests/Generated.cc"), ("second", "/repo/Tests/Readme.md")],
        None
    )]
    fn failure_summary_file_sources(
        #[case] file_name: Option<&str>,
        #[case] location: Option<&str>,
        #[case] stack: &[(&str, &str)],
        #[case] expected: Option<&str>,
    ) {
        let summary = serde_json::from_value(json!({
            "fileName": file_name.map(xc_string),
            "sourceCodeContext": {
                "location": { "filePath": location.map(xc_string) },
                "callStack": { "_values": stack.iter().map(|(symbol, path)| json!({
                    "symbolInfo": {
                        "symbolName": xc_string(symbol),
                        "location": { "filePath": xc_string(path) }
                    }
                })).collect::<Vec<_>>() }
            }
        }))
        .unwrap();
        let identity = TestIdentity {
            suite: Some(TEST_SUITE),
            case: TEST_CASE,
        };
        assert_eq!(
            XCResultTestLegacy::find_file_in_failure_summary(&summary, &identity),
            expected.map(String::from)
        );
    }

    #[rstest]
    #[case::producing_target_key(
        Some("file:///repo/Tests/Test.swift#EndingLineNumber=8"),
        Some("SnapshotReproTests"),
        Some("SnapshotReproTests.failingSnapshot()"),
        Some((Some("SnapshotReproTests"), "/repo/Tests/Test.swift"))
    )]
    #[case::test_case_key(
        Some("file:///repo/Tests/Test.swift"),
        None,
        Some("SnapshotReproTests.failingSnapshot()"),
        Some((Some("SnapshotReproTests.failingSnapshot()"), "/repo/Tests/Test.swift"))
    )]
    #[case::dependency_document_location(
        Some(
            "file:///repo/Tuist/.build/checkouts/UITestSupport/PageObject.swift#EndingLineNumber=377"
        ),
        Some("SnapshotReproTests"),
        Some("SnapshotReproTests.failingSnapshot()"),
        None
    )]
    #[case::missing_document_location(
        None,
        None,
        Some("SnapshotReproTests.failingSnapshot()"),
        None
    )]
    fn fallback_issue_summary_cleans_url_and_selects_key(
        #[case] url: Option<&str>,
        #[case] producing_target: Option<&str>,
        #[case] test_case_name: Option<&str>,
        #[case] expected: Option<(Option<&str>, &str)>,
    ) {
        let summary = serde_json::from_value(json!({
            "documentLocationInCreatingWorkspace": { "url": url.map(xc_string) },
            "producingTarget": producing_target.map(xc_string),
            "testCaseName": test_case_name.map(xc_string)
        }))
        .unwrap();
        let file = XCResultTestLegacy::fallback_file_from_failure_issue_summary(&summary)
            .map(|(key, file)| (key.map(String::from), file));
        assert_eq!(
            file,
            expected.map(|(key, file)| (key.map(String::from), file.to_string()))
        );
    }

    #[rstest]
    #[case::suite_key_wins(true, Some("/repo/Tests/Suite.swift"))]
    #[case::formatted_case_fallback(false, Some("/repo/Tests/TestCase.swift"))]
    fn fallback_file_lookup_prefers_suite_then_formatted_case(
        #[case] include_suite_file: bool,
        #[case] expected: Option<&str>,
    ) {
        let mut files = HashMap::new();
        if include_suite_file {
            files.insert(
                Some("SnapshotReproTests"),
                "/repo/Tests/Suite.swift".to_string(),
            );
        }
        files.insert(
            Some("SnapshotReproTests.failingSnapshot()"),
            "/repo/Tests/TestCase.swift".to_string(),
        );
        assert_eq!(
            XCResultTestLegacy::find_fallback_file(
                &files,
                Some("SnapshotReproTests"),
                "SnapshotReproTests.failingSnapshot()",
            ),
            expected.map(String::from)
        );
    }
}
