use std::io::Read;
use std::path::PathBuf;

use bazel_bep::types::build_event_stream::BuildEvent;
use prost::bytes::Buf;
use prost::Message;

use crate::bazel_bep::common::BepParseResult;

#[derive(Debug, Clone, Default)]
pub struct BazelBepBinParser {
    build_event_binary_file: PathBuf,
}

impl BazelBepBinParser {
    pub fn new<T: Into<PathBuf>>(build_event_binary_file: T) -> Self {
        Self {
            build_event_binary_file: build_event_binary_file.into(),
        }
    }

    pub fn parse(&mut self) -> anyhow::Result<BepParseResult> {
        tracing::info!("Attempting to parse bep file as binary");
        let mut file = std::fs::File::open(&self.build_event_binary_file)?;
        let mut raw_contents = Vec::new();
        file.read_to_end(&mut raw_contents)?;
        let mut buf = raw_contents.as_slice();

        let mut events = Vec::new();
        let mut has_hit_error = false;
        while buf.has_remaining() && !has_hit_error {
            let event = BuildEvent::decode_length_delimited(&mut buf).map_err(anyhow::Error::from);
            if event.is_err() {
                has_hit_error = true;
            }
            events.push(event);
        }

        BepParseResult::from_build_events(events)
    }
}

#[cfg(test)]
mod tests {
    use test_utils::inputs::get_test_file_path;

    use super::*;
    use crate::junit::junit_path::{JunitReportFileWithStatus, JunitReportStatus};

    const BINARY_FILE: &str = "test_fixtures/bep_binary_file.bin";
    const BROKEN_BINARY_FILE: &str = "test_fixtures/broken_bep_binary_file.bin";

    #[test]
    fn test_parse_binary() {
        let input_file = get_test_file_path(BINARY_FILE);
        let mut parser = BazelBepBinParser::new(input_file);
        let parse_result = parser.parse().unwrap();
        let empty_errors_vec: Vec<String> = Vec::new();

        assert_eq!(
      parse_result.uncached_xml_files(),
      vec![
        JunitReportFileWithStatus {
          junit_path: String::from("bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/37d45ccef587444393523741a3831f4a1acbeb010f74f33130ab9ba687477558/449"),
          status: Some(JunitReportStatus::Passed)
        },
        JunitReportFileWithStatus {
          junit_path: String::from("bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/46bbeb038d6f1447f6224a7db4d8a109e133884f2ee6ee78487ca4ce7e073de8/507"),
          status: Some(JunitReportStatus::Passed)
        },
        JunitReportFileWithStatus {
          junit_path: String::from("bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/d1f48dadf5679f09ce9b9c8f4778281ab25bc1dfdddec943e1255baf468630de/451"),
          status: Some(JunitReportStatus::Passed)
        },
        JunitReportFileWithStatus {
          junit_path: String::from("bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/38f1d4ce43242ed3cb08aedf1cc0c3133a8aec8e8eee61f5b84b85a5ba718bc8/1204"),
          status: Some(JunitReportStatus::Passed)
        },
        JunitReportFileWithStatus {
          junit_path: String::from("bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/ac23080b9bf5599b7781e3b62be9bf9a5b6685a8cbe76de4e9e1731a318e9283/607"),
          status: Some(JunitReportStatus::Passed)
        },
        JunitReportFileWithStatus {
          junit_path: String::from("bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/9c1db1d25ca6a4268be4a8982784c525a4b0ca99cbc7614094ad36c56bb08f2a/463"),
          status: Some(JunitReportStatus::Passed)
        },
        JunitReportFileWithStatus {
          junit_path: String::from("bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/7b3ed061a782496c7418be853caae863a9ada9618712f92346ea9e8169b8acf0/1120"),
          status: Some(JunitReportStatus::Passed)
        },
        JunitReportFileWithStatus {
          junit_path: String::from("bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/45ca1eed26b3cf1aafdb51829e32312d3b48452cc144aa041c946e89fa9c6cf6/175"),
          status: Some(JunitReportStatus::Passed)
        }
      ]
    );
        assert_eq!(parse_result.xml_file_counts(), (8, 0));
        assert_eq!(parse_result.errors, empty_errors_vec);
    }

    #[test]
    fn test_parse_broken_binary() {
        // Specifically making sure we don't go into an infinite loop if someone feeds us a malformed bin
        let input_file = get_test_file_path(BROKEN_BINARY_FILE);
        let mut parser = BazelBepBinParser::new(input_file);
        let parse_result = parser.parse().unwrap();

        assert_eq!(parse_result.uncached_xml_files(), Vec::new());
        assert_eq!(parse_result.xml_file_counts(), (0, 0));
        assert_eq!(
            parse_result.errors,
            vec![String::from(
                "Error parsing build event: failed to decode Protobuf message: buffer underflow"
            )]
        );
    }
}
