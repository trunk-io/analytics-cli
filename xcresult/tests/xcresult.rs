use std::{fs::File, path::Path};

use context::repo::RepoUrlParts;
use flate2::read::GzDecoder;
use lazy_static::lazy_static;
use tar::Archive;
use temp_testdir::TempDir;
use xcresult::xcresult::XCResult;

fn unpack_archive_to_temp_dir<T: AsRef<Path>>(archive_file_path: T) -> TempDir {
    let file = File::open(archive_file_path).unwrap();
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    let temp_dir = TempDir::default();
    if let Err(e) = archive.unpack(temp_dir.as_ref()) {
        panic!("failed to unpack data.tar.gz: {}", e);
    }
    temp_dir
}

lazy_static! {
    static ref TEMP_DIR_TEST_1: TempDir =
        unpack_archive_to_temp_dir("tests/data/test1.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_3: TempDir =
        unpack_archive_to_temp_dir("tests/data/test3.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_4: TempDir =
        unpack_archive_to_temp_dir("tests/data/test4.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_EXPECTED_FAILURES: TempDir =
        unpack_archive_to_temp_dir("tests/data/test-ExpectedFailures.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_SWIFT_WITHOUT_TEST_SUITES: TempDir =
        unpack_archive_to_temp_dir("tests/data/test-swift-without-test-suites.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_SWIFT_MIX: TempDir =
        unpack_archive_to_temp_dir("tests/data/test-swift-mix.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_TIMESTAMP: TempDir =
        unpack_archive_to_temp_dir("tests/data/test-timestamp.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_VARIANT: TempDir =
        unpack_archive_to_temp_dir("tests/data/test-variant.xcresult.tar.gz");
    static ref ORG_URL_SLUG: String = String::from("trunk");
    static ref REPO_FULL_NAME: String = RepoUrlParts {
        host: "github.com".to_string(),
        owner: "trunk-io".to_string(),
        name: "analytics-cli".to_string()
    }
    .repo_full_name();
}

#[cfg(target_os = "macos")]
#[test]
fn test_xcresult_with_valid_path() {
    let path = TEMP_DIR_TEST_1.as_ref().join("test1.xcresult");
    let path_str = path.to_str().unwrap();
    for use_experimental_failure_summary in [true, false] {
        let xcresult = XCResult::new(
            path_str,
            ORG_URL_SLUG.clone(),
            REPO_FULL_NAME.clone(),
            use_experimental_failure_summary,
        );
        assert!(xcresult.is_ok());

        let mut junits = xcresult.unwrap().generate_junits();
        assert_eq!(junits.len(), 1);
        let junit = junits.pop().unwrap();
        let mut junit_writer: Vec<u8> = Vec::new();
        junit.serialize(&mut junit_writer).unwrap();
        pretty_assertions::assert_eq!(
            String::from_utf8(junit_writer).unwrap(),
            include_str!("data/test1.junit.xml")
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn test_xcresult_with_invalid_path() {
    let path = TempDir::default().join("does-not-exist.xcresult");
    let path_str = path.to_str().unwrap();
    for use_experimental_failure_summary in [true, false] {
        let xcresult = XCResult::new(
            path_str,
            ORG_URL_SLUG.clone(),
            REPO_FULL_NAME.clone(),
            use_experimental_failure_summary,
        );
        assert!(xcresult.is_err());
        pretty_assertions::assert_eq!(
            xcresult.err().unwrap().to_string(),
            format!(
                "failed to get absolute path for {}: No such file or directory (os error 2)",
                path.to_string_lossy()
            )
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn test_xcresult_with_invalid_xcresult() {
    let path = TEMP_DIR_TEST_3.as_ref().join("test3.xcresult");
    let path_str = path.to_str().unwrap();
    for use_experimental_failure_summary in [true, false] {
        let xcresult = XCResult::new(
            path_str,
            ORG_URL_SLUG.clone(),
            REPO_FULL_NAME.clone(),
            use_experimental_failure_summary,
        );
        assert!(xcresult.is_err());
        pretty_assertions::assert_eq!(
            xcresult.err().unwrap().to_string(),
            "failed to parse json from xcresulttool output: expected value at line 1 column 1"
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn test_complex_xcresult_with_valid_path() {
    let path = TEMP_DIR_TEST_4.as_ref().join("test4.xcresult");
    let path_str = path.to_str().unwrap();
    for use_experimental_failure_summary in [true, false] {
        let xcresult = XCResult::new(
            path_str,
            ORG_URL_SLUG.clone(),
            REPO_FULL_NAME.clone(),
            use_experimental_failure_summary,
        );
        assert!(xcresult.is_ok());

        let mut junits = xcresult.unwrap().generate_junits();
        assert_eq!(junits.len(), 1);
        let junit = junits.pop().unwrap();
        let mut junit_writer: Vec<u8> = Vec::new();
        junit.serialize(&mut junit_writer).unwrap();
        pretty_assertions::assert_eq!(
            String::from_utf8(junit_writer).unwrap(),
            include_str!("data/test4.junit.xml")
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn test_swift_without_test_suites() {
    let path = TEMP_DIR_TEST_SWIFT_WITHOUT_TEST_SUITES
        .as_ref()
        .join("test-swift-without-test-suites.xcresult");
    let path_str = path.to_str().unwrap();
    for use_experimental_failure_summary in [true, false] {
        let xcresult = XCResult::new(
            path_str,
            ORG_URL_SLUG.clone(),
            REPO_FULL_NAME.clone(),
            use_experimental_failure_summary,
        );
        assert!(xcresult.is_ok());

        let mut junits = xcresult.unwrap().generate_junits();
        assert_eq!(junits.len(), 1);
        let junit = junits.pop().unwrap();
        let mut junit_writer: Vec<u8> = Vec::new();
        junit.serialize(&mut junit_writer).unwrap();
        pretty_assertions::assert_eq!(
            String::from_utf8(junit_writer).unwrap(),
            include_str!("data/test-swift-without-test-suites.junit.xml")
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn test_swift_mix() {
    let path = TEMP_DIR_TEST_SWIFT_MIX
        .as_ref()
        .join("test-swift-mix.xcresult");
    let path_str = path.to_str().unwrap();
    for use_experimental_failure_summary in [true, false] {
        let xcresult = XCResult::new(
            path_str,
            ORG_URL_SLUG.clone(),
            REPO_FULL_NAME.clone(),
            use_experimental_failure_summary,
        );
        assert!(xcresult.is_ok());

        let mut junits = xcresult.unwrap().generate_junits();
        assert_eq!(junits.len(), 1);
        let junit = junits.pop().unwrap();
        let mut junit_writer: Vec<u8> = Vec::new();
        junit.serialize(&mut junit_writer).unwrap();
        pretty_assertions::assert_eq!(
            String::from_utf8(junit_writer).unwrap(),
            include_str!("data/test-swift-mix.junit.xml")
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn test_xcresult_with_valid_path_invalid_os() {
    let path = TEMP_DIR_TEST_1.as_ref().join("test1.xcresult");
    let path_str = path.to_str().unwrap();
    let xcresult = XCResult::new(
        path_str,
        ORG_URL_SLUG.clone(),
        REPO_FULL_NAME.clone(),
        false,
    );
    pretty_assertions::assert_eq!(
        xcresult.err().unwrap().to_string(),
        "xcrun is only available on macOS"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_expected_failures_xcresult_with_valid_path() {
    let path = TEMP_DIR_TEST_EXPECTED_FAILURES
        .as_ref()
        .join("test-ExpectedFailures.xcresult");
    let path_str = path.to_str().unwrap();
    for use_experimental_failure_summary in [true, false] {
        let xcresult = XCResult::new(
            path_str,
            ORG_URL_SLUG.clone(),
            REPO_FULL_NAME.clone(),
            use_experimental_failure_summary,
        );
        assert!(xcresult.is_ok());

        let mut junits = xcresult.unwrap().generate_junits();
        assert_eq!(junits.len(), 1);
        let junit = junits.pop().unwrap();
        let mut junit_writer: Vec<u8> = Vec::new();
        junit.serialize(&mut junit_writer).unwrap();
        pretty_assertions::assert_eq!(
            String::from_utf8(junit_writer).unwrap(),
            include_str!("data/test-ExpectedFailures.junit.xml")
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn test_xcresult_to_bindings_report_with_id_and_timestamps() {
    use std::io::BufReader;

    use context::junit::bindings::BindingsTestCase;
    use context::junit::parser::JunitParser;

    let path = TEMP_DIR_TEST_TIMESTAMP.as_ref().join("test1.xcresult");
    let path_str = path.to_str().unwrap();

    let xcresult = XCResult::new(
        path_str,
        ORG_URL_SLUG.clone(),
        REPO_FULL_NAME.clone(),
        false,
    )
    .unwrap();

    let mut junits = xcresult.generate_junits();
    assert_eq!(junits.len(), 1);
    let junit = junits.pop().unwrap();

    let mut junit_writer: Vec<u8> = Vec::new();
    junit.serialize(&mut junit_writer).unwrap();
    let junit_xml = String::from_utf8(junit_writer).unwrap();

    let mut junit_parser = JunitParser::new();
    junit_parser
        .parse(BufReader::new(junit_xml.as_bytes()))
        .expect("Failed to parse generated JUnit XML");

    let test_case_runs: Vec<BindingsTestCase> = junit_parser
        .into_test_case_runs(
            None,
            &ORG_URL_SLUG.as_str(),
            &context::repo::RepoUrlParts {
                host: "github.com".to_string(),
                owner: "trunk-io".to_string(),
                name: "analytics-cli".to_string(),
            },
            &[],
            "",
        )
        .into_iter()
        .map(BindingsTestCase::from)
        .collect();

    for test_case in test_case_runs.iter() {
        let extra = test_case.extra();
        let id = extra.get("id").expect("ID should be set in extra fields");
        assert!(!id.is_empty(), "ID should not be empty");
        assert!(
            id.len() > 10,
            "ID should be a valid UUID or hash, got: {}",
            id
        );

        let timestamp = test_case.timestamp.expect("timestamp should be set");
        let timestamp_micros = test_case
            .timestamp_micros
            .expect("timestamp_micros should be set");

        // Verify timestamp is reasonable (2024-09-30T19:12:51+00:00)
        let timestamp_2024_09_30_19_12_51 = 1727723571; // 2024-09-30T19:12:51+00:00
        assert!(
            timestamp == timestamp_2024_09_30_19_12_51,
            "Timestamp should be 2024-09-30T19:12:51+00:00, got: {} ({})",
            timestamp,
            chrono::DateTime::from_timestamp(timestamp, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default()
        );
        assert!(
            timestamp_micros == 1727723571159000,
            "Timestamp micros should be 1727723571159000, got: {} ({})",
            timestamp_micros,
            chrono::DateTime::from_timestamp(timestamp_micros, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default()
        );

        assert!(test_case.time.is_some(), "time should be set");
        let time = test_case.time.unwrap();
        assert!(time >= 0.0, "time should be non-negative, got: {}", time);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn test_xcresult_with_variant_id_generation() {
    use std::io::BufReader;

    use context::junit::bindings::BindingsTestCase;
    use context::junit::parser::JunitParser;

    // Generate JUnit from xcresult
    let path = TEMP_DIR_TEST_VARIANT.as_ref().join("test1.xcresult");
    let path_str = path.to_str().unwrap();

    let xcresult = XCResult::new(
        path_str,
        ORG_URL_SLUG.clone(),
        REPO_FULL_NAME.clone(),
        false,
    )
    .unwrap();

    let mut junits = xcresult.generate_junits();
    assert_eq!(junits.len(), 1);
    let junit = junits.pop().unwrap();

    let mut junit_writer: Vec<u8> = Vec::new();
    junit.serialize(&mut junit_writer).unwrap();
    let junit_xml = String::from_utf8(junit_writer).unwrap();

    let repo_parts = context::repo::RepoUrlParts {
        host: "github.com".to_string(),
        owner: "trunk-io".to_string(),
        name: "analytics-cli".to_string(),
    };

    // Parse WITHOUT variant
    let mut junit_parser_no_variant = JunitParser::new();
    junit_parser_no_variant
        .parse(BufReader::new(junit_xml.as_bytes()))
        .expect("Failed to parse generated JUnit XML");

    let test_case_runs_no_variant: Vec<BindingsTestCase> = junit_parser_no_variant
        .into_test_case_runs(None, &ORG_URL_SLUG.as_str(), &repo_parts, &[], "")
        .into_iter()
        .map(BindingsTestCase::from)
        .collect();

    // Parse WITH variant
    let variant = "ios-simulator";
    let mut junit_parser_with_variant = JunitParser::new();
    junit_parser_with_variant
        .parse(BufReader::new(junit_xml.as_bytes()))
        .expect("Failed to parse generated JUnit XML");

    let test_case_runs_with_variant: Vec<BindingsTestCase> = junit_parser_with_variant
        .into_test_case_runs(None, &ORG_URL_SLUG.as_str(), &repo_parts, &[], variant)
        .into_iter()
        .map(BindingsTestCase::from)
        .collect();

    assert!(
        !test_case_runs_no_variant.is_empty(),
        "Should have test cases without variant"
    );
    assert!(
        !test_case_runs_with_variant.is_empty(),
        "Should have test cases with variant"
    );
    assert_eq!(
        test_case_runs_no_variant.len(),
        test_case_runs_with_variant.len(),
        "Should have same number of test cases"
    );

    for (test_no_variant, test_with_variant) in test_case_runs_no_variant
        .iter()
        .zip(test_case_runs_with_variant.iter())
    {
        let extra_no_variant = test_no_variant.extra();
        let id_no_variant = extra_no_variant
            .get("id")
            .expect("ID should be set for test without variant");

        let extra_with_variant = test_with_variant.extra();
        let id_with_variant = extra_with_variant
            .get("id")
            .expect("ID should be set for test with variant");

        assert!(
            !id_no_variant.is_empty(),
            "ID without variant should not be empty"
        );
        assert!(
            !id_with_variant.is_empty(),
            "ID with variant should not be empty"
        );
    }
}
