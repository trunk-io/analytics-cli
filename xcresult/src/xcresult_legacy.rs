use std::{
    collections::HashMap,
    ffi::OsStr,
    ops::{Deref, DerefMut},
};

use petgraph::{
    graph::{DiGraph, NodeIndex},
    Direction::Incoming,
};

use crate::types::{legacy_schema, SWIFT_DEFAULT_TEST_SUITE_NAME};
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
    pub fn generate_from_object<T: AsRef<OsStr>>(path: T) -> anyhow::Result<HashMap<String, Self>> {
        let actions_invocation_record = xcresulttool_get_object(path.as_ref())?;
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
                            .flat_map(|failure_summary| {
                                failure_summary
                                    .document_location_in_creating_workspace
                                    .as_ref()
                                    .and_then(|document_location_in_creating_workspace| {
                                        document_location_in_creating_workspace.url.as_ref()
                                    })
                                    .map(|file| {
                                        let mut file = file.value.clone();
                                        file = file
                                            .replace("file://", "")
                                            .split('#')
                                            .collect::<Vec<&str>>()[0]
                                            .into();
                                        let producing_target = failure_summary
                                            .producing_target
                                            .as_ref()
                                            .map(|x| x.value.as_ref());
                                        if producing_target.is_some() {
                                            return (producing_target, file);
                                        }
                                        let test_case_name = failure_summary
                                            .test_case_name
                                            .as_ref()
                                            .map(|x| x.value.as_ref());
                                        (test_case_name, file)
                                    })
                            })
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
                            let formatted_test_case_name =
                                if let Some(test_suite_name) = test_suite_name {
                                    format!("{}.{}", test_suite_name, test_case_name)
                                } else {
                                    test_case_name.to_string()
                                };
                            let failure_summary_id = node.weight.failure_summary_id;
                            let mut file = if let Some(failure_summary_id) = failure_summary_id {
                                let summary =
                                    xcresulttool_get_object_id(path.as_ref(), failure_summary_id);
                                summary.ok().and_then(|summary| {
                                    summary
                                        .failure_summaries
                                        .as_ref()
                                        .and_then(|failure_summaries| {
                                            // grab the first failure summary if there are multiple
                                            failure_summaries.values.first()
                                        })
                                        .and_then(|failure_summary| {
                                            failure_summary.source_code_context.as_ref().and_then(
                                                |source_code_context| {
                                                    source_code_context
                                                        .call_stack
                                                        .as_ref()
                                                        .and_then(|call_stack| {
                                                            call_stack
                                                                .values
                                                                .iter()
                                                                .filter_map(|call_stack| {
                                                                    call_stack
                                                                        .symbol_info
                                                                        .as_ref()
                                                                        .and_then(|symbol_info| {
                                                                            symbol_info
                                                                                .location
                                                                                .as_ref()
                                                                                .and_then(
                                                                                    |location| {
                                                                                        location
                                                                                .file_path
                                                                                .as_ref()
                                                                                    },
                                                                                )
                                                                        })
                                                                        .map(|file_path| {
                                                                            file_path.value.clone()
                                                                        })
                                                                })
                                                                .filter(|file_path| {
                                                                    std::path::Path::new(&file_path)
                                                                        .extension()
                                                                        .map(|ext| {
                                                                            ext == "swift"
                                                                                || ext == "m"
                                                                        })
                                                                        .unwrap_or(false)
                                                                })
                                                                // use the last valid swift / obj-c file-path in the stack
                                                                .last()
                                                        })
                                                },
                                            )
                                        })
                                })
                            } else {
                                None
                            };
                            if file.is_none() {
                                file = files.as_ref().and_then(|files| {
                                    files
                                        .get(&test_suite_name)
                                        .or_else(|| files.get(&Some(&formatted_test_case_name)))
                                        .cloned()
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
                    if let Some(ref subtests) = subtests {
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
