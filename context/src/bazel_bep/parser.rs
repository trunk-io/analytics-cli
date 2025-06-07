use std::path::PathBuf;

use bazel_bep::types::build_event_stream::BuildEvent;
use serde_json::Deserializer;

use crate::bazel_bep::common::BepParseResult;

/// Uses proto spec
/// https://github.com/TylerJang27/bazel-bep/blob/master/proto/build_event_stream.proto based on
/// https://github.com/bazelbuild/bazel/blob/master/src/main/java/com/google/devtools/build/lib/buildeventstream/proto/build_event_stream.proto
#[derive(Debug, Clone, Default)]
pub struct BazelBepParser {
    bazel_bep_path: PathBuf,
}

impl BazelBepParser {
    pub fn new<T: Into<PathBuf>>(bazel_bep_path: T) -> Self {
        Self {
            bazel_bep_path: bazel_bep_path.into(),
            ..Default::default()
        }
    }

    pub fn parse(&mut self) -> anyhow::Result<BepParseResult> {
        let file = std::fs::File::open(&self.bazel_bep_path)?;
        let reader = std::io::BufReader::new(file);

        let events = Deserializer::from_reader(reader)
            .into_iter::<BuildEvent>()
            .map(|r| r.map_err(anyhow::Error::from));

        BepParseResult::from_build_events(events)
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use test_utils::inputs::get_test_file_path;

    use super::*;
    use crate::junit::junit_path::{
        JunitReportFileWithTestRunnerReport, TestRunnerReport, TestRunnerReportStatus,
    };

    const SIMPLE_EXAMPLE: &str = "test_fixtures/bep_example";
    const EMPTY_EXAMPLE: &str = "test_fixtures/bep_empty";
    const PARTIAL_EXAMPLE: &str = "test_fixtures/bep_partially_valid";
    const FLAKY_SUMMARY_EXAMPLE: &str = "test_fixtures/bep_flaky_summary";

    #[test]
    fn test_parse_simple_bep() {
        let input_file = get_test_file_path(SIMPLE_EXAMPLE);
        let mut parser = BazelBepParser::new(input_file);
        let parse_result = parser.parse().unwrap();

        let empty_vec: Vec<String> = Vec::new();
        assert_eq!(
            parse_result.uncached_xml_files(),
            vec![JunitReportFileWithTestRunnerReport {
                junit_path: "/tmp/hello_test/test.xml".to_string(),
                test_runner_report: None
            }]
        );
        assert_eq!(parse_result.xml_file_counts(), (1, 0));
        assert_eq!(*parse_result.errors, empty_vec);
    }

    #[test]
    fn test_parse_empty_bep() {
        let input_file = get_test_file_path(EMPTY_EXAMPLE);
        let mut parser = BazelBepParser::new(input_file);
        let parse_result = parser.parse().unwrap();

        let empty_xml_vec: Vec<JunitReportFileWithTestRunnerReport> = Vec::new();
        let empty_errors_vec: Vec<String> = Vec::new();
        assert_eq!(parse_result.uncached_xml_files(), empty_xml_vec);
        assert_eq!(parse_result.xml_file_counts(), (0, 0));
        assert_eq!(*parse_result.errors, empty_errors_vec);
    }

    #[test]
    fn test_parse_partial_bep() {
        let input_file = get_test_file_path(PARTIAL_EXAMPLE);
        let mut parser = BazelBepParser::new(input_file);
        let parse_result = parser.parse().unwrap();

        pretty_assertions::assert_eq!(
            parse_result.uncached_xml_files(),
            vec![
                JunitReportFileWithTestRunnerReport {
                    junit_path: "/tmp/hello_test/test.xml".to_string(),
                    test_runner_report: Some(TestRunnerReport {
                        status: TestRunnerReportStatus::Passed,
                        start_time: DateTime::parse_from_rfc3339("2024-12-02T20:27:17.474Z")
                            .unwrap()
                            .into(),
                        end_time: DateTime::parse_from_rfc3339("2024-12-02T20:27:17.627Z")
                            .unwrap()
                            .into(),
                    })
                },
                JunitReportFileWithTestRunnerReport {
                    junit_path: "/tmp/client_test/test.xml".to_string(),
                    test_runner_report: Some(TestRunnerReport {
                        status: TestRunnerReportStatus::Passed,
                        start_time: DateTime::parse_from_rfc3339("2024-12-02T20:50:00.347Z")
                            .unwrap()
                            .into(),
                        end_time: DateTime::parse_from_rfc3339("2024-12-02T20:50:02.100Z")
                            .unwrap()
                            .into(),
                    })
                }
            ]
        );
        assert_eq!(parse_result.xml_file_counts(), (3, 1));
        pretty_assertions::assert_eq!(
            parse_result.errors,
            vec!["Error parsing build event: EOF while parsing a value at line 108 column 0"]
        );
    }

    #[test]
    fn test_parse_flaky_summary_bep() {
        let input_file = get_test_file_path(FLAKY_SUMMARY_EXAMPLE);
        let mut parser = BazelBepParser::new(input_file);
        let parse_result = parser.parse().unwrap();

        pretty_assertions::assert_eq!(
            parse_result.uncached_xml_files(),
            vec![
                JunitReportFileWithTestRunnerReport {
                    junit_path: "/tmp/hello_test/test_attempts/attempt_1.xml".to_string(),
                    test_runner_report: Some(TestRunnerReport {
                        status: TestRunnerReportStatus::Flaky,
                        start_time: DateTime::parse_from_rfc3339("2024-12-17T04:10:55.425Z")
                            .unwrap()
                            .into(),
                        end_time: DateTime::parse_from_rfc3339("2024-12-17T04:10:55.466Z")
                            .unwrap()
                            .into(),
                    })
                },
                JunitReportFileWithTestRunnerReport {
                    junit_path: "/tmp/hello_test/test_attempts/attempt_2.xml".to_string(),
                    test_runner_report: Some(TestRunnerReport {
                        status: TestRunnerReportStatus::Flaky,
                        start_time: DateTime::parse_from_rfc3339("2024-12-17T04:10:55.425Z")
                            .unwrap()
                            .into(),
                        end_time: DateTime::parse_from_rfc3339("2024-12-17T04:10:55.466Z")
                            .unwrap()
                            .into(),
                    })
                },
                JunitReportFileWithTestRunnerReport {
                    junit_path: "/tmp/hello_test/test.xml".to_string(),
                    test_runner_report: Some(TestRunnerReport {
                        status: TestRunnerReportStatus::Flaky,
                        start_time: DateTime::parse_from_rfc3339("2024-12-17T04:10:55.425Z")
                            .unwrap()
                            .into(),
                        end_time: DateTime::parse_from_rfc3339("2024-12-17T04:10:55.466Z")
                            .unwrap()
                            .into(),
                    })
                },
                JunitReportFileWithTestRunnerReport {
                    junit_path: "/tmp/client_test/test.xml".to_string(),
                    test_runner_report: Some(TestRunnerReport {
                        status: TestRunnerReportStatus::Failed,
                        start_time: DateTime::parse_from_rfc3339("2024-12-17T04:10:55.322Z")
                            .unwrap()
                            .into(),
                        end_time: DateTime::parse_from_rfc3339("2024-12-17T04:10:56.383Z")
                            .unwrap()
                            .into(),
                    })
                }
            ]
        );
        assert_eq!(parse_result.xml_file_counts(), (4, 0));
    }
}
