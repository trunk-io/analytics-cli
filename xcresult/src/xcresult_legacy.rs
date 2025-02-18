use std::{
    collections::HashMap,
    ffi::OsStr,
    ops::{Deref, DerefMut},
};

use petgraph::{
    graph::{DiGraph, NodeIndex},
    Direction::Incoming,
};

use crate::types::legacy_schema;
use crate::xcrun::{xcresulttool_get_object, xcresulttool_get_object_id};

#[derive(Debug, Clone, Default)]
pub struct XCResultTest {
    pub test_plan_name: String,
    pub test_bundle_name: String,
    pub test_suite_name: String,
    pub test_case_name: String,
    pub identifier_url: String,
    pub identifier: String,
    pub file: Option<String>,
}

impl XCResultTest {
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
                    let mut xc_result_test_node_tree = XCResultTestNodeTree::default();
                    xc_result_test_node_tree
                        .traverse(action_test_summary_identifiable_objects, None);

                    let leafs = xc_result_test_node_tree.externals(petgraph::Direction::Outgoing);
                    let raw_nodes = xc_result_test_node_tree.raw_nodes();
                    let raw_edges = xc_result_test_node_tree.raw_edges();
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
                            let file = failure_summaries
                                .as_ref()
                                .and_then(|failure_summaries| {
                                    failure_summaries
                                        .values
                                        .iter()
                                        .find(|failure_summary| {
                                            let producing_target = failure_summary
                                                .producing_target
                                                .as_ref()
                                                .map(|x| x.value.as_ref());
                                            let inner_test_case_name = failure_summary
                                                .test_case_name
                                                .as_ref()
                                                .map(|x| x.value.as_ref());
                                            let formatted_test_case_name =
                                                format!("{}.{}", test_suite_name, test_case_name);
                                            return producing_target == Some(test_suite_name)
                                                || inner_test_case_name
                                                    == Some(formatted_test_case_name.as_str());
                                        })
                                        .and_then(|failure_summary| {
                                            failure_summary
                                                .document_location_in_creating_workspace
                                                .as_ref()
                                        })
                                })
                                .and_then(|document_location_in_creating_workspace| {
                                    document_location_in_creating_workspace.url.as_ref()
                                })
                                .map(|file| file.value.clone())
                                .map(|file| {
                                    let file = file.replace("file://", "");
                                    file.split('#').collect::<Vec<&str>>()[0].into()
                                });

                            Some(Self {
                                test_plan_name: String::from(*test_plan_name),
                                test_bundle_name: String::from(action_testable_summary_name),
                                test_suite_name: String::from(test_suite_name),
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
        action_test_summary_identifiable_objects: &'a [legacy_schema::ActionTestSummaryIdentifiableObject],
        parent_node: Option<NodeIndex>,
    ) {
        for action_test_summary_identifiable_object in
            action_test_summary_identifiable_objects.as_ref().iter()
        {
            match action_test_summary_identifiable_object {
                legacy_schema::ActionTestSummaryIdentifiableObject::Variant0(
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
                legacy_schema::ActionTestSummaryIdentifiableObject::Variant1(
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
                legacy_schema::ActionTestSummaryIdentifiableObject::Variant2(
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
