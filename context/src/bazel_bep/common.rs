use std::collections::HashMap;

use anyhow::{Ok, Result};
use bazel_bep::types::build_event_stream::{
    build_event::Payload, build_event_id::Id, file::File::Uri, BuildEvent, TestSummary,
};
use chrono::DateTime;

use crate::junit::junit_path::{
    JunitReportFileWithTestRunnerReport, TestRunnerReport, TestRunnerReportStatus,
};

const FILE_URI_PREFIX: &str = "file://";

#[derive(Debug, Clone, Default)]
pub struct BepTestResult {
    pub label: String,
    pub cached: bool,
    pub xml_files: Vec<String>,
    pub test_runner_report: Option<TestRunnerReport>,
}

#[derive(Debug, Clone, Default)]
pub struct BepParseResult {
    pub bep_test_events: Vec<BuildEvent>,
    pub errors: Vec<String>,
    pub test_results: Vec<BepTestResult>,
}

impl BepParseResult {
    pub fn from_build_events<I: IntoIterator<Item = Result<BuildEvent>>>(
        events: I,
    ) -> Result<Self> {
        #[derive(Debug, Default)]
        struct Acc {
            errors: Vec<String>,
            test_results: Vec<BepTestResult>,
            test_runner_reports: HashMap<String, TestRunnerReport>,
            bep_test_events: Vec<BuildEvent>,
        }
        let Acc {
            errors,
            test_results,
            test_runner_reports,
            bep_test_events,
        } = events
            .into_iter()
            .fold(Acc::default(), |mut acc, parse_event| {
                match parse_event {
                    Result::Err(ref err) => {
                        acc.errors
                            .push(format!("Error parsing build event: {}", err));
                    }
                    Result::Ok(build_event) => {
                        let payload = &build_event.payload;
                        let id = build_event.id.clone().and_then(|id| id.id);
                        match (payload, id) {
                            (
                                Some(Payload::TestSummary(test_summary)),
                                Some(Id::TestSummary(id)),
                            ) => {
                                if let Result::Ok(test_runner_report) =
                                    TestRunnerReport::try_from(&LabelledTestSummary {
                                        test_summary,
                                        label: Some(id.label.clone()),
                                    })
                                {
                                    acc.test_runner_reports
                                        .insert(id.label.clone(), test_runner_report);
                                }
                                acc.bep_test_events.push(build_event);
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

                                acc.test_results.push(BepTestResult {
                                    label: id.label.clone(),
                                    cached,
                                    xml_files,
                                    test_runner_report: None,
                                });
                                acc.bep_test_events.push(build_event);
                            }
                            _ => {}
                        }
                    }
                }

                acc
            });

        Ok(BepParseResult {
            bep_test_events,
            errors,
            test_results: test_results
                .into_iter()
                .map(|test_result| BepTestResult {
                    test_runner_report: test_runner_reports.get(&test_result.label).cloned(),
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

    pub fn uncached_xml_files(&self) -> Vec<JunitReportFileWithTestRunnerReport> {
        self.test_results
            .iter()
            .filter_map(|r| {
                if r.cached {
                    return None;
                }
                Some(
                    r.xml_files
                        .iter()
                        .map(|f| JunitReportFileWithTestRunnerReport {
                            junit_path: f.clone(),
                            test_runner_report: r.test_runner_report.clone(),
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten()
            .collect()
    }
}

struct LabelledTestSummary<'a> {
    test_summary: &'a TestSummary,
    label: Option<String>,
}

impl<'a> TryFrom<&LabelledTestSummary<'a>> for TestRunnerReport {
    type Error = anyhow::Error;

    fn try_from(wrapped_test_summary: &LabelledTestSummary) -> Result<Self> {
        Ok(Self {
            status: TestRunnerReportStatus::try_from(
                wrapped_test_summary.test_summary.overall_status(),
            )?,
            start_time: wrapped_test_summary
                .test_summary
                .first_start_time
                .clone()
                .ok_or(anyhow::anyhow!("No start time"))
                .and_then(|ts| {
                    DateTime::try_from(ts)
                        .map_err(|e| anyhow::anyhow!("Failed to convert start time: {}", e))
                })?,
            end_time: wrapped_test_summary
                .test_summary
                .last_stop_time
                .clone()
                .ok_or(anyhow::anyhow!("No end time"))
                .and_then(|ts| {
                    DateTime::try_from(ts)
                        .map_err(|e| anyhow::anyhow!("Failed to convert end time: {}", e))
                })?,
            label: wrapped_test_summary.label.clone(),
        })
    }
}
