use lazy_static::lazy_static;
use rstest::rstest;
use temp_testdir::TempDir;
use xcresult::xcresult::XCResult;

mod common;

#[cfg(target_os = "macos")]
use common::assert_junit;
use common::{ORG_URL_SLUG, REPO_FULL_NAME, unpack_archive_to_temp_dir};

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
    static ref TEMP_DIR_TEST_SWIFT_SNAPSHOT_TESTING: TempDir =
        unpack_archive_to_temp_dir("tests/data/test-swift-snapshot-testing.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_DEPENDENCY_RAISES_FAILURE: TempDir =
        unpack_archive_to_temp_dir("tests/data/test-dependency-raises-failure.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_IN_REPO_HELPER_RAISES_FAILURE: TempDir =
        unpack_archive_to_temp_dir("tests/data/test-in-repo-helper-raises-failure.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_CRASH_IN_DEPENDENCY: TempDir =
        unpack_archive_to_temp_dir("tests/data/test-crash-in-dependency.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_OBJC_XCTEST: TempDir =
        unpack_archive_to_temp_dir("tests/data/test-objc-xctest.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_TOPLEVEL_SWIFT_TESTING: TempDir =
        unpack_archive_to_temp_dir("tests/data/test-toplevel-swift-testing.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_NESTED_AND_PASSING: TempDir =
        unpack_archive_to_temp_dir("tests/data/test-nested-and-passing.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_TIMESTAMP: TempDir =
        unpack_archive_to_temp_dir("tests/data/test-timestamp.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_VARIANT: TempDir =
        unpack_archive_to_temp_dir("tests/data/test-variant.xcresult.tar.gz");
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
#[rstest]
#[case::experimental_failure_summary(true)]
#[case::legacy_fallback(false)]
fn test_swift_snapshot_testing_trait_failure_uses_assertion_file(
    #[case] use_experimental_failure_summary: bool,
) {
    let path = TEMP_DIR_TEST_SWIFT_SNAPSHOT_TESTING
        .as_ref()
        .join("SnapshotRepro.xcresult");
    let path_str = path.to_str().unwrap();
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
        include_str!("data/test-swift-snapshot-testing.junit.xml")
    );
}

// Real bundles for the file sources in `xcresult::file_attribution`, one per shape
// that used to attribute a failed test to a vendored dependency. Each test below
// names the source it exercises; `tests/fixture-src/README.md` covers what each
// bundle must exhibit and how to regenerate it.
//
// The two cases expect different JUnit because they have different sources

// `FileSource::TestFrame` is the only usable source. The failure is recorded inside
// the dependency, so `RaisedFrom`, `SourceCodeLocation` and the innermost
// `LastStackFrame` all point into `DerivedData/SourcePackages/checkouts/` and are
// rejected as vendored. The legacy path has only `DocumentLocation`, which points
// there too, so it reports nothing at all.
#[cfg(target_os = "macos")]
#[rstest]
#[case::experimental_failure_summary(
    true,
    include_str!("data/test-dependency-raises-failure.junit.xml")
)]
#[case::legacy_fallback(
    false,
    include_str!("data/test-dependency-raises-failure.legacy.junit.xml")
)]
fn test_dependency_raised_failure_uses_the_tests_own_file(
    #[case] use_experimental_failure_summary: bool,
    #[case] expected_junit_xml: &str,
) {
    assert_junit(
        TEMP_DIR_TEST_DEPENDENCY_RAISES_FAILURE
            .as_ref()
            .join("DependencyRaisesFailure.xcresult"),
        use_experimental_failure_summary,
        expected_junit_xml,
    );
}

// `FileSource::TestFrame` versus `RaisedFrom` with nothing to separate them by path.
// The helper is in the test target, so `is_vendored_dependency` says nothing useful
// and the ordering of the sources is what decides. The legacy path, having only
// `DocumentLocation`, still lands on the helper.
#[cfg(target_os = "macos")]
#[rstest]
#[case::experimental_failure_summary(
    true,
    include_str!("data/test-in-repo-helper-raises-failure.junit.xml")
)]
#[case::legacy_fallback(
    false,
    include_str!("data/test-in-repo-helper-raises-failure.legacy.junit.xml")
)]
fn test_in_repo_helper_raised_failure_uses_the_tests_own_file(
    #[case] use_experimental_failure_summary: bool,
    #[case] expected_junit_xml: &str,
) {
    assert_junit(
        TEMP_DIR_TEST_IN_REPO_HELPER_RAISES_FAILURE
            .as_ref()
            .join("InRepoHelperRaisesFailure.xcresult"),
        use_experimental_failure_summary,
        expected_junit_xml,
    );
}

// No source survives vetting. Neither test reaches its own frame — one crashes
// inside the dependency, the other is failed by the dependency's trait after its
// body returned — so there is no `TestFrame`, and every remaining candidate is
// either absent or vendored. Both cases must come out with no `file` attribute at
// all rather than the dependency's.
#[cfg(target_os = "macos")]
#[rstest]
#[case::experimental_failure_summary(true, include_str!("data/test-crash-in-dependency.junit.xml"))]
#[case::legacy_fallback(false, include_str!("data/test-crash-in-dependency.junit.xml"))]
fn test_crash_in_dependency_reports_no_file(
    #[case] use_experimental_failure_summary: bool,
    #[case] expected_junit_xml: &str,
) {
    assert_junit(
        TEMP_DIR_TEST_CRASH_IN_DEPENDENCY
            .as_ref()
            .join("CrashInDependency.xcresult"),
        use_experimental_failure_summary,
        expected_junit_xml,
    );
}

// `TestIdentity::is_named_by` against real symbolication: Xcode spells the frame
// `-[ObjcXCTestTests testFailsInsideSharedHelper]`, not `Suite.testCase()`. The
// legacy path reports nothing here for an unrelated reason — the `DocumentLocation`
// candidates are keyed by test case name, and Xcode's Objective-C spelling never
// matches the `Suite.testCase` key the lookup builds.
#[cfg(target_os = "macos")]
#[rstest]
#[case::experimental_failure_summary(true, include_str!("data/test-objc-xctest.junit.xml"))]
#[case::legacy_fallback(false, include_str!("data/test-objc-xctest.legacy.junit.xml"))]
fn test_objc_xctest_helper_failure_uses_the_tests_own_file(
    #[case] use_experimental_failure_summary: bool,
    #[case] expected_junit_xml: &str,
) {
    assert_junit(
        TEMP_DIR_TEST_OBJC_XCTEST
            .as_ref()
            .join("ObjcXCTest.xcresult"),
        use_experimental_failure_summary,
        expected_junit_xml,
    );
}

// `TestIdentity` with no suite: a top-level swift-testing `@Test func` symbolicates
// as the bare function, so there is nothing to qualify the match with.
#[cfg(target_os = "macos")]
#[rstest]
#[case::experimental_failure_summary(
    true,
    include_str!("data/test-toplevel-swift-testing.junit.xml")
)]
#[case::legacy_fallback(
    false,
    include_str!("data/test-toplevel-swift-testing.legacy.junit.xml")
)]
fn test_toplevel_swift_testing_helper_failure_uses_the_tests_own_file(
    #[case] use_experimental_failure_summary: bool,
    #[case] expected_junit_xml: &str,
) {
    assert_junit(
        TEMP_DIR_TEST_TOPLEVEL_SWIFT_TESTING
            .as_ref()
            .join("ToplevelSwiftTesting.xcresult"),
        use_experimental_failure_summary,
        expected_junit_xml,
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
        .into_test_case_runs(context::junit::parser::IntoTestCaseRunsOptions {
            org_slug: ORG_URL_SLUG.as_str(),
            repo: &context::repo::RepoUrlParts {
                host: "github.com".to_string(),
                owner: "trunk-io".to_string(),
                name: "analytics-cli".to_string(),
            },
            codeowners: None,
            quarantined_test_ids: &[],
            variant: "",
            test_runner_config: None,
        })
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
        .into_test_case_runs(context::junit::parser::IntoTestCaseRunsOptions {
            org_slug: ORG_URL_SLUG.as_str(),
            repo: &repo_parts,
            codeowners: None,
            quarantined_test_ids: &[],
            variant: "",
            test_runner_config: None,
        })
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
        .into_test_case_runs(context::junit::parser::IntoTestCaseRunsOptions {
            org_slug: ORG_URL_SLUG.as_str(),
            repo: &repo_parts,
            codeowners: None,
            quarantined_test_ids: &[],
            variant,
            test_runner_config: None,
        })
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
