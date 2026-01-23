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
    use crate::bazel_bep::common::BepTestStatus;
    use crate::junit::junit_path::{
        JunitReportFileWithTestRunnerReport, TestRunnerReport, TestRunnerReportStatus,
    };

    const SIMPLE_EXAMPLE: &str = "test_fixtures/bep_example";
    const EMPTY_EXAMPLE: &str = "test_fixtures/bep_empty";
    const PARTIAL_EXAMPLE: &str = "test_fixtures/bep_partially_valid";
    const FLAKY_SUMMARY_EXAMPLE: &str = "test_fixtures/bep_flaky_summary";
    const RETRIES_EXAMPLE: &str = "test_fixtures/bep_retries";

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
    fn test_parse_retries_bep() {
        let input_file = get_test_file_path(RETRIES_EXAMPLE);
        let mut parser = BazelBepParser::new(input_file);
        let parse_result = parser.parse().unwrap();

        let uncached_xml_files = parse_result.uncached_xml_files();
        assert_eq!(
            uncached_xml_files.len(),
            2,
            "Should have 2 XML files from retries"
        );

        let xml_paths: Vec<String> = uncached_xml_files
            .iter()
            .map(|f| f.junit_path.clone())
            .collect();
        assert!(
            xml_paths.iter().any(|path| path.contains("URI_FAIL")),
            "Should have fail XML"
        );
        assert!(
            xml_paths.iter().any(|path| path.contains("URI_PASS")),
            "Should have pass XML"
        );

        // Test that we have the correct number of test results (should be 1 since we merge by label)
        assert_eq!(
            parse_result.test_results.len(),
            1,
            "Should have 1 test result (merged by label)"
        );

        // Test that the merged result has both XML files
        let test_result = &parse_result.test_results[0];
        assert_eq!(
            test_result.xml_files.len(),
            2,
            "Should have 2 XML files in merged result"
        );
        assert_eq!(test_result.label, "//trunk/hello_world/cc:hello_test");

        // Test that the build status reflects the latest status (should be Success since the last attempt was PASSED)
        assert!(test_result.build_status.is_some());
        // Verify that the status is PASSED (the latest attempt) not FAILED (the most severe)
        assert_eq!(
            test_result.build_status.as_ref().unwrap(),
            &BepTestStatus::Passed
        );

        // Test that attempt numbers are captured correctly
        let attempt_numbers: Vec<i32> = test_result.xml_files.iter().map(|f| f.attempt).collect();
        assert_eq!(attempt_numbers.len(), 2, "Should have 2 attempt numbers");
        assert!(attempt_numbers.contains(&0), "Should have attempt 0");
        assert!(attempt_numbers.contains(&1), "Should have attempt 1");

        assert_eq!(
            parse_result.errors.len(),
            0,
            "Should have no parsing errors"
        );
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
                        label: Some("//trunk/hello_world/cc:hello_test".into())
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
                        label: Some("//trunk/hello_world/cc_grpc:client_test".into())
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
                        label: Some("//trunk/hello_world/cc:hello_test".into())
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
                        label: Some("//trunk/hello_world/cc:hello_test".into())
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
                        label: Some("//trunk/hello_world/cc:hello_test".into())
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
                        label: Some("//trunk/hello_world/cc_grpc:client_test".into())
                    })
                }
            ]
        );
        assert_eq!(parse_result.xml_file_counts(), (4, 0));

        // Test that attempt numbers are captured for the flaky test
        let hello_test_result = parse_result
            .test_results
            .iter()
            .find(|r| r.label == "//trunk/hello_world/cc:hello_test")
            .expect("Should find hello_test result");
        let attempt_numbers: Vec<i32> = hello_test_result
            .xml_files
            .iter()
            .map(|f| f.attempt)
            .collect();
        assert_eq!(
            attempt_numbers.len(),
            3,
            "Should have 3 attempt numbers for flaky test"
        );
        assert!(attempt_numbers.contains(&0), "Should have attempt 0");
        assert!(attempt_numbers.contains(&1), "Should have attempt 1");
        assert!(attempt_numbers.contains(&2), "Should have attempt 2");
    }

    #[test]
    fn test_uncached_labels() {
        use itertools::Itertools;
        let input_file = get_test_file_path(FLAKY_SUMMARY_EXAMPLE);
        let mut parser = BazelBepParser::new(input_file);
        let parse_result = parser.parse().unwrap();

        let uncached_labels = parse_result.uncached_labels();

        // Should have 2 labels
        assert_eq!(uncached_labels.len(), 2);

        // Get the first label (hello_test)
        let first_label = uncached_labels.keys().sorted().next().unwrap();
        assert_eq!(first_label, "//trunk/hello_world/cc:hello_test");
        let first_label_files = uncached_labels.get(first_label).unwrap();
        assert_eq!(
            first_label_files.len(),
            3,
            "Should have 3 XML files from retries"
        );

        // Check that we have all the expected XML files from the retries
        let xml_paths: Vec<String> = first_label_files
            .iter()
            .map(|f| f.junit_path.clone())
            .collect();

        assert!(xml_paths.contains(&"/tmp/hello_test/test_attempts/attempt_1.xml".to_string()));
        assert!(xml_paths.contains(&"/tmp/hello_test/test_attempts/attempt_2.xml".to_string()));
        assert!(xml_paths.contains(&"/tmp/hello_test/test.xml".to_string()));

        // All files should have the same test runner report
        for file in first_label_files {
            assert!(file.test_runner_report.is_some());
            if let Some(report) = &file.test_runner_report {
                assert_eq!(report.status, TestRunnerReportStatus::Flaky);
            }
        }

        // Get the second label (client_test)
        let second_label = uncached_labels.keys().sorted().nth(1).unwrap();
        assert_eq!(second_label, "//trunk/hello_world/cc_grpc:client_test");
        let second_label_files = uncached_labels.get(second_label).unwrap();
        assert_eq!(second_label_files.len(), 1);

        let second_label_file = &second_label_files[0];
        assert_eq!(second_label_file.junit_path, "/tmp/client_test/test.xml");
        assert!(second_label_file.test_runner_report.is_some());
        if let Some(report) = &second_label_file.test_runner_report {
            assert_eq!(report.status, TestRunnerReportStatus::Failed);
        }
    }
}
