use anyhow::Ok;
use bazel_bep::types::build_event_stream::{build_event::Payload, file::File::Uri, BuildEvent};
use serde_json::Deserializer;

#[derive(Debug, Clone)]
pub struct TestResult {
    pub cached: bool,
    pub xml_files: Vec<String>,
}

const FILE_URI_PREFIX: &str = "file://";

/// Uses proto spec
/// https://github.com/TylerJang27/bazel-bep/blob/master/proto/build_event_stream.proto based on
/// https://github.com/bazelbuild/bazel/blob/master/src/main/java/com/google/devtools/build/lib/buildeventstream/proto/build_event_stream.proto
pub struct BazelBepParser {
    bazel_bep_path: String,
    test_results: Vec<TestResult>,
    errors: Vec<String>,
}

impl BazelBepParser {
    pub fn new(bazel_bep_path: String) -> Self {
        Self {
            bazel_bep_path,
            test_results: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn errors(&self) -> &Vec<String> {
        &self.errors
    }

    pub fn xml_files(&self) -> Vec<String> {
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

        let build_events = Deserializer::from_reader(reader).into_iter::<BuildEvent>();
        build_events.for_each(|parse_event| {
            if let Some(err) = parse_event.as_ref().err() {
                self.errors
                    .push(format!("Error parsing build event: {}", err));
                return;
            }
            if let Some(build_event) = parse_event.ok() {
                if let Some(Payload::TestResult(test_result)) = build_event.payload {
                    let xml_files = test_result.test_action_output.into_iter().fold(
                        Vec::new(),
                        |mut xml_files, action_output| {
                            if action_output.name.ends_with(".xml") {
                                if let Some(Uri(file)) = action_output.file {
                                    xml_files.push(
                                        file.strip_prefix(FILE_URI_PREFIX)
                                            .unwrap_or(&file)
                                            .to_string(),
                                    );
                                }
                            }
                            xml_files
                        },
                    );

                    let cached = if let Some(execution_info) = test_result.execution_info {
                        execution_info.cached_remotely || test_result.cached_locally
                    } else {
                        test_result.cached_locally
                    };

                    self.test_results.push(TestResult { cached, xml_files });
                }
            }
        });

        if !&self.errors.is_empty() {
            log::warn!("Errors parsing BEP file: {:?}", &self.errors);
        }

        log::info!(
            "Parsed {} ({} cached) test results from BEP file",
            self.test_results.len(),
            self.test_results.iter().filter(|r| r.cached).count()
        );
        Ok(())
    }
}
