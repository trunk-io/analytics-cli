use anyhow::Ok;
use bazel_bep::types::build_event_stream::{build_event::Payload, file::File::Uri, BuildEvent};
use serde_json::Deserializer;

#[derive(Debug, Clone, Default)]
pub struct TestResult {
    pub cached: bool,
    pub xml_files: Vec<String>,
}

const FILE_URI_PREFIX: &str = "file://";

/// Uses proto spec
/// https://github.com/TylerJang27/bazel-bep/blob/master/proto/build_event_stream.proto based on
/// https://github.com/bazelbuild/bazel/blob/master/src/main/java/com/google/devtools/build/lib/buildeventstream/proto/build_event_stream.proto
#[derive(Debug, Clone, Default)]
pub struct BazelBepParser {
    bazel_bep_path: String,
    test_results: Vec<TestResult>,
    errors: Vec<String>,
}

impl BazelBepParser {
    pub fn new(bazel_bep_path: String) -> Self {
        Self {
            bazel_bep_path,
            ..Default::default()
        }
    }

    pub fn errors(&self) -> &Vec<String> {
        &self.errors
    }

    pub fn test_counts(&self) -> (usize, usize) {
        let (test_count, cached_count) = self.test_results.iter().fold(
            (0, 0),
            |(mut test_count, mut cached_count), test_result| {
                test_count += test_result.xml_files.len();
                if test_result.cached {
                    cached_count += test_result.xml_files.len();
                }
                (test_count, cached_count)
            },
        );
        (test_count, cached_count)
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

    pub fn parse(&mut self) -> anyhow::Result<()> {
        let file = std::fs::File::open(&self.bazel_bep_path)?;
        let reader = std::io::BufReader::new(file);

        let (errors, test_results) = Deserializer::from_reader(reader)
            .into_iter::<BuildEvent>()
            .fold(
                (Vec::<String>::new(), Vec::<TestResult>::new()),
                |(mut errors, mut test_results), parse_event| {
                    match parse_event {
                        Result::Err(ref err) => {
                            errors.push(format!("Error parsing build event: {}", err));
                        }
                        Result::Ok(build_event) => {
                            if let Some(Payload::TestResult(test_result)) = build_event.payload {
                                let xml_files = test_result
                                    .test_action_output
                                    .into_iter()
                                    .filter_map(|action_output| {
                                        if action_output.name.ends_with(".xml") {
                                            action_output.file.and_then(|f| {
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
                                    if let Some(execution_info) = test_result.execution_info {
                                        execution_info.cached_remotely || test_result.cached_locally
                                    } else {
                                        test_result.cached_locally
                                    };

                                test_results.push(TestResult { cached, xml_files });
                            }
                        }
                    }

                    (errors, test_results)
                },
            );

        self.errors = errors;
        self.test_results = test_results;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_utils::inputs::get_test_file_path;

    const SIMPLE_EXAMPLE: &str = "test_fixtures/bep_example";
    const EMPTY_EXAMPLE: &str = "test_fixtures/bep_empty";
    const PARTIAL_EXAMPLE: &str = "test_fixtures/bep_partially_valid";

    #[test]
    fn test_parse_simple_bep() {
        let input_file = get_test_file_path(SIMPLE_EXAMPLE);
        let mut parser = BazelBepParser::new(input_file);
        parser.parse().unwrap();

        let empty_vec: Vec<String> = Vec::new();
        assert_eq!(
            parser.uncached_xml_files(),
            vec!["/tmp/hello_test/test.xml"]
        );
        assert_eq!(*parser.errors(), empty_vec);
    }

    #[test]
    fn test_parse_empty_bep() {
        let input_file = get_test_file_path(EMPTY_EXAMPLE);
        let mut parser = BazelBepParser::new(input_file);
        parser.parse().unwrap();

        let empty_vec: Vec<String> = Vec::new();
        assert_eq!(parser.uncached_xml_files(), empty_vec);
        assert_eq!(*parser.errors(), empty_vec);
    }

    #[test]
    fn test_parse_partial_bep() {
        let input_file = get_test_file_path(PARTIAL_EXAMPLE);
        let mut parser = BazelBepParser::new(input_file);
        parser.parse().unwrap();

        assert_eq!(
            parser.uncached_xml_files(),
            vec!["/tmp/hello_test/test.xml", "/tmp/client_test/test.xml"]
        );
        assert_eq!(
            *parser.errors(),
            vec!["Error parsing build event: EOF while parsing a value at line 108 column 0"]
        );
    }
}
