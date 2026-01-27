use std::collections::HashMap;

use anyhow::{Ok, Result};
use bazel_bep::types::build_event_stream::{
    BuildEvent, TestStatus, TestSummary, build_event::Payload, build_event_id::Id, file::File::Uri,
};
use chrono::DateTime;
use proto::test_context::test_run::TestBuildResult;

use crate::junit::junit_path::{
    JunitReportFileWithTestRunnerReport, TestRunnerReport, TestRunnerReportStatus,
};

const FILE_URI_PREFIX: &str = "file://";

pub fn map_test_status_to_build_result(test_status: BepTestStatus) -> TestBuildResult {
    match test_status {
        BepTestStatus::Passed => TestBuildResult::Success,
        BepTestStatus::Flaky => TestBuildResult::Flaky,
        BepTestStatus::Timeout => TestBuildResult::Failure,
        BepTestStatus::Failed => TestBuildResult::Failure,
        BepTestStatus::Incomplete => TestBuildResult::Failure,
        BepTestStatus::RemoteFailure => TestBuildResult::Failure,
        BepTestStatus::FailedToBuild => TestBuildResult::Failure,
        BepTestStatus::ToolHaltedBeforeTesting => TestBuildResult::Failure,
        BepTestStatus::NoStatus => TestBuildResult::Unspecified,
    }
}

pub fn map_i32_test_status_to_bep_test_status(test_status: i32) -> Result<BepTestStatus> {
    if test_status == TestStatus::Passed as i32 {
        Ok(BepTestStatus::Passed)
    } else if test_status == TestStatus::Flaky as i32 {
        Ok(BepTestStatus::Flaky)
    } else if test_status == TestStatus::Timeout as i32 {
        Ok(BepTestStatus::Timeout)
    } else if test_status == TestStatus::Failed as i32 {
        Ok(BepTestStatus::Failed)
    } else if test_status == TestStatus::Incomplete as i32 {
        Ok(BepTestStatus::Incomplete)
    } else if test_status == TestStatus::RemoteFailure as i32 {
        Ok(BepTestStatus::RemoteFailure)
    } else if test_status == TestStatus::FailedToBuild as i32 {
        Ok(BepTestStatus::FailedToBuild)
    } else if test_status == TestStatus::ToolHaltedBeforeTesting as i32 {
        Ok(BepTestStatus::ToolHaltedBeforeTesting)
    } else {
        Err(anyhow::anyhow!("Unknown test status: {}", test_status))
    }
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum BepTestStatus {
    Passed,
    Failed,
    Flaky,
    Timeout,
    Incomplete,
    RemoteFailure,
    FailedToBuild,
    ToolHaltedBeforeTesting,
    NoStatus,
}

#[derive(Debug, Clone)]
pub struct BepXMLFile {
    pub file: String,
    pub attempt: i32,
    pub label: String,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Default)]
pub struct BepTestResult {
    pub label: String,
    pub cached: bool,
    pub xml_files: Vec<BepXMLFile>,
    pub test_runner_report: Option<TestRunnerReport>,
    pub build_status: Option<BepTestStatus>, // TestStatus from Bazel BEP
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
                                let start_time =
                                    test_result.test_attempt_start.clone().and_then(|ts| {
                                        DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
                                    });
                                let end_time = start_time.and_then(|start| {
                                    test_result.test_attempt_duration.clone().map(|duration| {
                                        let duration_secs = duration.seconds;
                                        let duration_nanos = duration.nanos as i64;
                                        start
                                            + chrono::Duration::seconds(duration_secs)
                                            + chrono::Duration::nanoseconds(duration_nanos)
                                    })
                                });
                                let xml_files = test_result
                                    .test_action_output
                                    .iter()
                                    .filter_map(|action_output| {
                                        if action_output.name.ends_with(".xml") {
                                            action_output.file.clone().and_then(|f| {
                                                if let Uri(uri) = f {
                                                    Some(BepXMLFile {
                                                        file: uri
                                                            .strip_prefix(FILE_URI_PREFIX)
                                                            .unwrap_or(&uri)
                                                            .to_string(),
                                                        // bazel attempt number is 1-indexed, our representation is 0-indexed
                                                        attempt: id.attempt - 1,
                                                        label: id.label.clone(),
                                                        start_time,
                                                        end_time,
                                                    })
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

                                let new_status = match map_i32_test_status_to_bep_test_status(
                                    test_result.status,
                                ) {
                                    std::result::Result::Ok(status) => status,
                                    Err(err) => {
                                        acc.errors.push(format!(
                                            "Error mapping test status for label {}: {}",
                                            id.label, err
                                        ));
                                        BepTestStatus::NoStatus
                                    }
                                };
                                if let Some(existing_result) =
                                    acc.test_results.iter_mut().find(|r| r.label == id.label)
                                {
                                    existing_result.xml_files.extend(xml_files);
                                    existing_result.build_status = Some(new_status);
                                } else {
                                    acc.test_results.push(BepTestResult {
                                        label: id.label.clone(),
                                        cached,
                                        xml_files,
                                        test_runner_report: None,
                                        build_status: Some(new_status),
                                    });
                                }
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
                            junit_path: f.file.clone(),
                            test_runner_report: r.test_runner_report.clone(),
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten()
            .collect()
    }

    pub fn uncached_labels(&self) -> HashMap<String, Vec<JunitReportFileWithTestRunnerReport>> {
        self.test_results
            .iter()
            .filter_map(|r| {
                if r.cached {
                    return None;
                }
                Some((
                    r.label.clone(),
                    r.xml_files
                        .iter()
                        .map(|f| JunitReportFileWithTestRunnerReport {
                            junit_path: f.file.clone(),
                            test_runner_report: r.test_runner_report.clone(),
                        })
                        .collect::<Vec<_>>(),
                ))
            })
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
