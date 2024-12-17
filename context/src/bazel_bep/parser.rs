use anyhow::Ok;
use bazel_bep::types::build_event_stream::{
    build_event::Payload, build_event_id::Id, file::File::Uri, BuildEvent, TestStatus,
};
use serde_json::Deserializer;
use std::{collections::HashSet, path::PathBuf};

#[derive(Debug, Clone, Default)]
pub struct TestResult {
    pub label: String,
    pub cached: bool,
    pub xml_files: Vec<String>,
    pub override_pass: bool,
}

const FILE_URI_PREFIX: &str = "file://";

#[derive(Debug, Clone, Default)]
pub struct BepParseResult {
    pub bep_test_events: Vec<BuildEvent>,
    pub errors: Vec<String>,
    pub test_results: Vec<TestResult>,
}

impl BepParseResult {
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

    pub fn uncached_xml_files(&self) -> Vec<String> {
        self.test_results
            .iter()
            .filter_map(|r| {
                if r.cached {
                    return None;
                }
                Some(r.xml_files.clone())
            })
            .flatten()
            .collect()
    }
}

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

        let (errors, test_results, pass_labels, bep_test_events) =
            Deserializer::from_reader(reader)
                .into_iter::<BuildEvent>()
                .fold(
                    (
                        Vec::<String>::new(),
                        Vec::<TestResult>::new(),
                        HashSet::<String>::new(),
                        Vec::<BuildEvent>::new(),
                    ),
                    |(mut errors, mut test_results, mut pass_labels, mut bep_test_events),
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
                                        // These are the cases in which bazel marks a test as passing
                                        if test_summary.overall_status == TestStatus::Passed as i32
                                            || test_summary.overall_status
                                                == TestStatus::Flaky as i32
                                        {
                                            pass_labels.insert(id.label.clone());
                                        }
                                        bep_test_events.push(build_event);
                                    }
                                    (
                                        Some(Payload::TestResult(test_result)),
                                        Some(Id::TestResult(id)),
                                    ) => {
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

                                        let cached = if let Some(execution_info) =
                                            &test_result.execution_info
                                        {
                                            execution_info.cached_remotely
                                                || test_result.cached_locally
                                        } else {
                                            test_result.cached_locally
                                        };

                                        test_results.push(TestResult {
                                            label: id.label.clone(),
                                            cached,
                                            xml_files,
                                            override_pass: false,
                                        });
                                        bep_test_events.push(build_event);
                                    }
                                    _ => {}
                                }
                            }
                        }

                        (errors, test_results, pass_labels, bep_test_events)
                    },
                );

        Ok(BepParseResult {
            bep_test_events,
            errors,
            test_results: test_results
                .iter()
                .map(|test_result| {
                    let mut override_pass = false;
                    if pass_labels.contains(&test_result.label.to_string()) {
                        override_pass = true;
                    }
                    TestResult {
                        override_pass,
                        ..test_result.clone()
                    }
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_utils::inputs::get_test_file_path;

    const SIMPLE_EXAMPLE: &str = "test_fixtures/bep_example";
    const EMPTY_EXAMPLE: &str = "test_fixtures/bep_empty";
    const PARTIAL_EXAMPLE: &str = "test_fixtures/bep_partially_valid";
    // DONOTLAND TODO: TYLER NEED TO ADD A TEST CASE FOR SUMMARY STATUS

    #[test]
    fn test_parse_simple_bep() {
        let input_file = get_test_file_path(SIMPLE_EXAMPLE);
        let mut parser = BazelBepParser::new(input_file);
        let parse_result = parser.parse().unwrap();

        let empty_vec: Vec<String> = Vec::new();
        assert_eq!(
            parse_result.uncached_xml_files(),
            vec!["/tmp/hello_test/test.xml"]
        );
        assert_eq!(parse_result.xml_file_counts(), (1, 0));
        assert_eq!(*parse_result.errors, empty_vec);
    }

    #[test]
    fn test_parse_empty_bep() {
        let input_file = get_test_file_path(EMPTY_EXAMPLE);
        let mut parser = BazelBepParser::new(input_file);
        let parse_result = parser.parse().unwrap();

        let empty_vec: Vec<String> = Vec::new();
        assert_eq!(parse_result.uncached_xml_files(), empty_vec);
        assert_eq!(parse_result.xml_file_counts(), (0, 0));
        assert_eq!(*parse_result.errors, empty_vec);
    }

    #[test]
    fn test_parse_partial_bep() {
        let input_file = get_test_file_path(PARTIAL_EXAMPLE);
        let mut parser = BazelBepParser::new(input_file);
        let parse_result = parser.parse().unwrap();

        assert_eq!(
            parse_result.uncached_xml_files(),
            vec!["/tmp/hello_test/test.xml", "/tmp/client_test/test.xml"]
        );
        assert_eq!(parse_result.xml_file_counts(), (3, 1));
        assert_eq!(
            *parse_result.errors,
            vec!["Error parsing build event: EOF while parsing a value at line 108 column 0"]
        );
    }
}
