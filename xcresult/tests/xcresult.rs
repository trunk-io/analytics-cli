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
    let xcresult = XCResult::new(path_str, ORG_URL_SLUG.clone(), REPO_FULL_NAME.clone());
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
