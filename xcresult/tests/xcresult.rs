use std::{fs::File, path::Path};

use context::repo::RepoUrlParts;
use flate2::read::GzDecoder;
use lazy_static::lazy_static;
use tar::Archive;
use temp_testdir::TempDir;
use xcresult::XCResult;

fn unpack_archive_to_temp_dir<T: AsRef<Path>>(archive_file_path: T) -> TempDir {
    let file = File::open(archive_file_path).unwrap();
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    let temp_dir = TempDir::default();
    if let Err(e) = archive.unpack(temp_dir.as_ref()) {
        panic!("failed to unpack data.tar.gz: {}", e);
    }
    temp_dir
}

lazy_static! {
    static ref TEMP_DIR_TEST_1: TempDir =
        unpack_archive_to_temp_dir("tests/data/test1.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_3: TempDir =
        unpack_archive_to_temp_dir("tests/data/test3.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_4: TempDir =
        unpack_archive_to_temp_dir("tests/data/test4.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_EXPECTED_FAILURES: TempDir =
        unpack_archive_to_temp_dir("tests/data/test-ExpectedFailures.xcresult.tar.gz");
    static ref ORG_URL_SLUG: String = String::from("trunk");
    static ref REPO_FULL_NAME: String = RepoUrlParts {
        host: "github.com".to_string(),
        owner: "trunk-io".to_string(),
        name: "analytics-cli".to_string()
    }
    .repo_full_name();
}

#[cfg(target_os = "macos")]
#[test]
fn test_xcresult_with_valid_path() {
    let path = TEMP_DIR_TEST_1.as_ref().join("test1.xcresult");
    let path_str = path.to_str().unwrap();
    let xcresult = XCResult::new(path_str, ORG_URL_SLUG.clone(), REPO_FULL_NAME.clone());
    assert!(xcresult.is_ok());

    let mut junits = xcresult.unwrap().generate_junits();
    assert_eq!(junits.len(), 1);
    let junit = junits.pop().unwrap();
    let mut junit_writer: Vec<u8> = Vec::new();
    junit.serialize(&mut junit_writer).unwrap();
    pretty_assertions::assert_eq!(
        String::from_utf8(junit_writer).unwrap(),
        include_str!("data/test1.junit.xml")
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_xcresult_with_invalid_path() {
    let path = TempDir::default().join("does-not-exist.xcresult");
    let path_str = path.to_str().unwrap();
    let xcresult = XCResult::new(path_str, ORG_URL_SLUG.clone(), REPO_FULL_NAME.clone());
    assert!(xcresult.is_err());
    pretty_assertions::assert_eq!(
        xcresult.err().unwrap().to_string(),
        format!(
            "failed to get absolute path for {}: No such file or directory (os error 2)",
            path.to_string_lossy()
        )
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_xcresult_with_invalid_xcresult() {
    let path = TEMP_DIR_TEST_3.as_ref().join("test3.xcresult");
    let path_str = path.to_str().unwrap();
    let xcresult = XCResult::new(path_str, ORG_URL_SLUG.clone(), REPO_FULL_NAME.clone());
    assert!(xcresult.is_err());
    pretty_assertions::assert_eq!(
        xcresult.err().unwrap().to_string(),
        "failed to parse json from xcrun output: EOF while parsing a value at line 1 column 0"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_complex_xcresult_with_valid_path() {
    let path = TEMP_DIR_TEST_4.as_ref().join("test4.xcresult");
    let path_str = path.to_str().unwrap();
    let xcresult = XCResult::new(path_str, ORG_URL_SLUG.clone(), REPO_FULL_NAME.clone());
    assert!(xcresult.is_ok());

    let mut junits = xcresult.unwrap().generate_junits();
    assert_eq!(junits.len(), 1);
    let junit = junits.pop().unwrap();
    let mut junit_writer: Vec<u8> = Vec::new();
    junit.serialize(&mut junit_writer).unwrap();
    pretty_assertions::assert_eq!(
        String::from_utf8(junit_writer).unwrap(),
        include_str!("data/test4.junit.xml")
    );
}

#[cfg(target_os = "linux")]
#[test]
fn test_xcresult_with_valid_path_invalid_os() {
    let path = TEMP_DIR_TEST_1.as_ref().join("test1.xcresult");
    let path_str = path.to_str().unwrap();
    let xcresult = XCResult::new(path_str, ORG_URL_SLUG.clone(), REPO_FULL_NAME.clone());
    pretty_assertions::assert_eq!(
        xcresult.err().unwrap().to_string(),
        "xcrun is only available on macOS"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_expected_failures_xcresult_with_valid_path() {
    let path = TEMP_DIR_TEST_EXPECTED_FAILURES
        .as_ref()
        .join("test-ExpectedFailures.xcresult");
    let path_str = path.to_str().unwrap();
    let xcresult = XCResult::new(path_str, ORG_URL_SLUG.clone(), REPO_FULL_NAME.clone());
    assert!(xcresult.is_ok());

    let mut junits = xcresult.unwrap().generate_junits();
    assert_eq!(junits.len(), 1);
    let junit = junits.pop().unwrap();
    let mut junit_writer: Vec<u8> = Vec::new();
    junit.serialize(&mut junit_writer).unwrap();
    pretty_assertions::assert_eq!(
        String::from_utf8(junit_writer).unwrap(),
        include_str!("data/test-ExpectedFailures.junit.xml")
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_xcresulttool_get_object() {
    use std::{
        ffi::OsStr,
        ops::{Deref, DerefMut},
    };

    use petgraph::{
        graph::{DiGraph, NodeIndex},
        Direction::Incoming,
    };
    use xcresult::xcrun_cmd;

    #[derive(Debug, Clone, Default)]
    struct XCResultTest {
        pub test_plan_name: String,
        pub test_bundle_name: String,
        pub test_suite_name: String,
        pub test_case_name: String,
        pub identifier_url: String,
    }

    impl XCResultTest {
        fn generate_from_object<T: AsRef<OsStr>>(path: T) -> Vec<Self> {
            let actions_invocation_record =
                xcrun_cmd::xcresulttool_get_object(path.as_ref()).unwrap();
            let test_plans = actions_invocation_record
                .actions
                .as_ref()
                .map(|arr| arr.values.iter())
                .unwrap_or_default()
                .filter_map(|action_record| {
                    if let xcresult::fd_schema::ActionRecord {
                        test_plan_name:
                            Some(xcresult::fd_schema::String {
                                value: test_plan_name,
                                ..
                            }),
                        action_result:
                            xcresult::fd_schema::ActionResult {
                                tests_ref:
                                    Some(xcresult::fd_schema::Reference {
                                        id: Some(xcresult::fd_schema::String { value: id, .. }),
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

            test_plans
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
                    if let xcresult::fd_schema::ActionTestableSummary {
                        name:
                            Some(xcresult::fd_schema::String {
                                value: action_testable_summary_name,
                                ..
                            }),
                        tests:
                            Some(xcresult::fd_schema::ActionTestableSummaryTests {
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

                        let leafs =
                            xc_result_test_node_tree.externals(petgraph::Direction::Outgoing);
                        let raw_nodes = xc_result_test_node_tree.raw_nodes();
                        let raw_edges = xc_result_test_node_tree.raw_edges();
                        leafs
                            .map(|leaf| {
                                let node = &raw_nodes[leaf.index()];
                                let edge = &raw_edges[node.next_edge(Incoming).index()];
                                let parent_node = &raw_nodes[edge.source().index()];

                                let test_suite_name = parent_node.weight.name;
                                let test_case_name = node.weight.name;

                                Self {
                                    test_plan_name: String::from(*test_plan_name),
                                    test_bundle_name: String::from(action_testable_summary_name),
                                    test_suite_name: String::from(test_suite_name),
                                    test_case_name: String::from(test_case_name),
                                    identifier_url: String::from(node.weight.identifier_url),
                                }
                            })
                            .collect::<Vec<_>>()
                    },
                )
                .collect()
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
            action_test_summary_identifiable_objects: &'a [xcresult::fd_schema::ActionTestSummaryIdentifiableObject],
            parent_node: Option<NodeIndex>,
        ) {
            for action_test_summary_identifiable_object in
                action_test_summary_identifiable_objects.as_ref().iter()
            {
                match action_test_summary_identifiable_object {
                    xcresult::fd_schema::ActionTestSummaryIdentifiableObject::Variant0(
                        xcresult::fd_schema::ActionTestSummary {
                            name: Some(xcresult::fd_schema::String { value: name, .. }),
                            identifier:
                                Some(xcresult::fd_schema::String {
                                    value: identifier, ..
                                }),
                            identifier_url:
                                Some(xcresult::fd_schema::String {
                                    value: identifier_url,
                                    ..
                                }),
                            expected_failures,
                            failure_summaries,
                            ..
                        },
                    ) => {
                        dbg!(&name);
                        let blah1 = expected_failures
                            .as_ref()
                            .map(|x| x.values.as_slice())
                            .unwrap_or_default();
                        let blah2 = failure_summaries
                            .as_ref()
                            .map(|x| x.values.as_slice())
                            .unwrap_or_default();
                        let blah3 = blah1
                            .iter()
                            .filter_map(|expected_failure| {
                                expected_failure.failure_summary.as_ref()
                            })
                            .chain(blah2.iter());
                        let blah4 = blah3.collect::<Vec<_>>();
                        dbg!(blah4);
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
                    xcresult::fd_schema::ActionTestSummaryIdentifiableObject::Variant2(
                        xcresult::fd_schema::ActionTestSummaryGroup {
                            name: Some(xcresult::fd_schema::String { value: name, .. }),
                            identifier:
                                Some(xcresult::fd_schema::String {
                                    value: identifier, ..
                                }),
                            identifier_url:
                                Some(xcresult::fd_schema::String {
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
                    xcresult::fd_schema::ActionTestSummaryIdentifiableObject::Variant0(..)
                    | xcresult::fd_schema::ActionTestSummaryIdentifiableObject::Variant1(..)
                    | xcresult::fd_schema::ActionTestSummaryIdentifiableObject::Variant2(..)
                    | xcresult::fd_schema::ActionTestSummaryIdentifiableObject::Variant3 {
                        ..
                    } => {
                        dbg!(
                            serde_json::to_value(action_test_summary_identifiable_object)
                                .map(|v| match v {
                                    serde_json::Value::Object(map) => map.get("_type").cloned(),
                                    _ => unreachable!(),
                                })
                                .unwrap()
                        );
                    }
                }
            }
        }
    }

    // let path = TEMP_DIR_TEST_4.as_ref().join("test4.xcresult");
    let path = "/Users/dylan/Downloads/gradle-ui-tests-dump/gradle-ui-tests.xcresult";
    // let path = "/Users/dylan/Downloads/gradle-AllUnitTests.xcresult";

    let xcresult_tests = XCResultTest::generate_from_object(path);

    dbg!(xcresult_tests);
}
