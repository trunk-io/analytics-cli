use std::collections::HashMap;

use anyhow::{Ok, Result};
use bazel_bep::types::build_event_stream::{
    build_event::Payload, build_event_id::Id, file::File::Uri, BuildEvent,
};

use crate::junit::junit_path::{JunitReportFileWithStatus, JunitReportStatus};

const FILE_URI_PREFIX: &str = "file://";

#[derive(Debug, Clone, Default)]
pub struct TestResult {
    pub label: String,
    pub cached: bool,
    pub xml_files: Vec<String>,
    pub summary_status: Option<JunitReportStatus>,
}

#[derive(Debug, Clone, Default)]
pub struct BepParseResult {
    pub bep_test_events: Vec<BuildEvent>,
    pub errors: Vec<String>,
    pub test_results: Vec<TestResult>,
}

impl BepParseResult {
    pub fn from_build_events<I: IntoIterator<Item = Result<BuildEvent>>>(
        events: I,
    ) -> Result<Self> {
        let (errors, test_results, summary_statuses, bep_test_events) = events.into_iter().fold(
            (
                Vec::<String>::new(),
                Vec::<TestResult>::new(),
                HashMap::<String, JunitReportStatus>::new(),
                Vec::<BuildEvent>::new(),
            ),
            |(mut errors, mut test_results, mut summary_statuses, mut bep_test_events),
             parse_event| {
                match parse_event {
                    Result::Err(ref err) => {
                        errors.push(format!("Error parsing build event: {}", err));
                    }
                    Result::Ok(build_event) => {
                        let payload = &build_event.payload;
                        let id = build_event.id.clone().and_then(|id| id.id);
                        match (payload, id) {
                            (
                                Some(Payload::TestSummary(test_summary)),
                                Some(Id::TestSummary(id)),
                            ) => {
                                if let Result::Ok(status) =
                                    JunitReportStatus::try_from(test_summary.overall_status())
                                {
                                    summary_statuses.insert(id.label.clone(), status);
                                }
                                bep_test_events.push(build_event);
                            }
                            (Some(Payload::TestResult(test_result)), Some(Id::TestResult(id))) => {
                                let xml_files = test_result
                                    .test_action_output
                                    .iter()
                                    .filter_map(|action_output| {
                                        if action_output.name.ends_with(".xml") {
                                            action_output.file.clone().and_then(|f| {
                                                if let Uri(uri) = f {
                                                    Some(
                                                        uri.strip_prefix(FILE_URI_PREFIX)
                                                            .unwrap_or(&uri)
                                                            .to_string(),
                                                    )
                                                } else {
                                                    None
                                                }
                                            })
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();

                                let cached =
                                    if let Some(execution_info) = &test_result.execution_info {
                                        execution_info.cached_remotely || test_result.cached_locally
                                    } else {
                                        test_result.cached_locally
                                    };

                                test_results.push(TestResult {
                                    label: id.label.clone(),
                                    cached,
                                    xml_files,
                                    summary_status: None,
                                });
                                bep_test_events.push(build_event);
                            }
                            _ => {}
                        }
                    }
                }

                (errors, test_results, summary_statuses, bep_test_events)
            },
        );

        Ok(BepParseResult {
            bep_test_events,
            errors,
            test_results: test_results
                .into_iter()
                .map(|test_result| TestResult {
                    summary_status: summary_statuses.get(&test_result.label).cloned(),
                    ..test_result
                })
                .collect(),
        })
    }

    pub fn xml_file_counts(&self) -> (usize, usize) {
        let (xml_count, cached_xml_count) = self.test_results.iter().fold(
            (0, 0),
            |(mut test_count, mut cached_count), test_result| {
                test_count += test_result.xml_files.len();
                if test_result.cached {
                    cached_count += test_result.xml_files.len();
                }
                (test_count, cached_count)
            },
        );
        (xml_count, cached_xml_count)
    }

    pub fn uncached_xml_files(&self) -> Vec<JunitReportFileWithStatus> {
        self.test_results
            .iter()
            .filter_map(|r| {
                if r.cached {
                    return None;
                }
                Some(
                    r.xml_files
                        .iter()
                        .map(|f| JunitReportFileWithStatus {
                            junit_path: f.clone(),
                            status: r.summary_status.clone(),
                        })
                        .collect::<Vec<JunitReportFileWithStatus>>(),
                )
            })
            .flatten()
            .collect()
    }
}
