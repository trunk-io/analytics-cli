use std::path::Path;

use lazy_static::lazy_static;
use rstest::rstest;
use temp_testdir::TempDir;
#[cfg(target_os = "macos")]
use xcresult::test_locations::Limits;
use xcresult::xcresult::XCResult;

mod common;

use common::{ORG_URL_SLUG, REPO_FULL_NAME, unpack_archive_to_temp_dir};
#[cfg(target_os = "macos")]
use common::{
    assert_the_declaration_flag_moves_only_the_file, declaration_files, declaration_report,
};

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
// available: only the experimental path reads the per-test failure summary, and so
// the call stack, so it is the only one that can produce a `FileSource::TestFrame`.
// The legacy path sees `FileSource::DocumentLocation` alone.
#[cfg(target_os = "macos")]
fn assert_junit<T: AsRef<Path>>(
    bundle_path: T,
    use_experimental_failure_summary: bool,
    expected_junit_xml: &str,
) {
    let path_str = bundle_path.as_ref().to_str().unwrap();
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
    pretty_assertions::assert_eq!(String::from_utf8(junit_writer).unwrap(), expected_junit_xml);
}

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

// Every file this bundle's failure summary offers is under `SourcePackages/checkouts/`.
#[cfg(target_os = "macos")]
#[test]
fn test_declaration_locations_prefer_the_tests_own_file_over_a_vendored_dependency() {
    let files = declaration_files(
        TEMP_DIR_TEST_DEPENDENCY_RAISES_FAILURE
            .as_ref()
            .join("DependencyRaisesFailure.xcresult"),
        "tests/fixture-src/dependency-raises-failure",
    );
    let file = files
        .get("failsInsideDependency()")
        .expect("the fixture's only test");
    assert!(
        file.ends_with("DependencyRaisesFailureTests.swift"),
        "expected the test's own file, got {file}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_declaration_locations_prefer_the_tests_own_file_over_an_in_repo_helper() {
    let files = declaration_files(
        TEMP_DIR_TEST_IN_REPO_HELPER_RAISES_FAILURE
            .as_ref()
            .join("InRepoHelperRaisesFailure.xcresult"),
        "tests/fixture-src/in-repo-helper-raises-failure",
    );
    let file = files
        .get("failsInsideHelper()")
        .expect("the fixture's only test");
    assert!(
        file.ends_with("InRepoHelperRaisesFailureTests.swift"),
        "expected the test's own file, got {file}"
    );
}

// The case no failure summary can serve: one test crashes inside a dependency with zero
// call-stack frames, the other is failed by a trait after its own frame is gone, so both
// failure-summary paths report no file — `data/test-crash-in-dependency.junit.xml` has none.
#[cfg(target_os = "macos")]
#[test]
fn test_declaration_locations_give_a_crashed_test_its_file() {
    let files = declaration_files(
        TEMP_DIR_TEST_CRASH_IN_DEPENDENCY
            .as_ref()
            .join("CrashInDependency.xcresult"),
        "tests/fixture-src/crash-in-dependency",
    );
    for (name, expected) in [
        (
            "testCrashesInsideDependency()",
            "CrashInDependencyTests.swift",
        ),
        (
            "failsAfterItsOwnFrameIsGone()",
            "TeardownFailureTests.swift",
        ),
    ] {
        let file = files
            .get(name)
            .unwrap_or_else(|| panic!("{name} is missing from the report"));
        assert!(
            file.ends_with(expected),
            "expected {name} to resolve to {expected}, got {file}"
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn test_declaration_locations_resolve_an_objc_test_through_clangd() {
    let files = declaration_files(
        TEMP_DIR_TEST_OBJC_XCTEST
            .as_ref()
            .join("ObjcXCTest.xcresult"),
        "tests/fixture-src/objc-xctest",
    );
    let file = files
        .get("testFailsInsideSharedHelper")
        .expect("the fixture's only test");
    assert!(
        file.ends_with("ObjcXCTestTests.m"),
        "expected the test's own file, got {file}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_declaration_locations_find_a_top_level_swift_testing_function() {
    let files = declaration_files(
        TEMP_DIR_TEST_TOPLEVEL_SWIFT_TESTING
            .as_ref()
            .join("ToplevelSwiftTesting.xcresult"),
        "tests/fixture-src/toplevel-swift-testing",
    );
    let file = files
        .get("failsInsideHelperWithoutASuite()")
        .expect("the fixture's only test");
    assert!(
        file.ends_with("ToplevelSwiftTestingTests.swift"),
        "expected the test's own file, got {file}"
    );
}

// Two things the declaration path reads that only a real bundle can confirm, both of which
// fail silently rather than loudly if the assumption is wrong.
//
// `nodeIdentifierURL` is meant to be the legacy record's `identifierURL` under another name,
// and ids are derived from it — if it is absent from the modern API, ids fall back to
// `nodeIdentifier` and every xcresult test case in the product gets a new identity.
// `get test-results summary`'s `startTime` is read as seconds since the Unix epoch; if it is
// an Apple reference-date offset instead, every timestamp lands three decades off.
//
// Both are checked as equivalence against the path already in production, on a bundle whose
// repo root is empty so no language server runs and nothing else can move.
#[cfg(target_os = "macos")]
#[test]
fn test_declaration_locations_keep_ids_and_timestamps_identical_to_the_legacy_path() {
    fn ids_and_timestamps(xcresult: &XCResult) -> Vec<(String, String, String)> {
        let mut junits = xcresult.generate_junits();
        assert_eq!(junits.len(), 1);
        junits
            .pop()
            .unwrap()
            .test_suites
            .iter()
            .flat_map(|test_suite| test_suite.test_cases.iter())
            .map(|test_case| {
                let id = test_case
                    .extra
                    .iter()
                    .find(|(key, _)| key.as_str() == "id")
                    .map(|(_, value)| value.as_str().to_owned())
                    .unwrap_or_default();
                (
                    test_case.name.as_str().to_owned(),
                    id,
                    test_case
                        .timestamp
                        .map(|timestamp| timestamp.to_string())
                        .unwrap_or_default(),
                )
            })
            .collect()
    }

    let path = TEMP_DIR_TEST_TIMESTAMP.as_ref().join("test1.xcresult");
    let path_str = path.to_str().unwrap();
    let empty_checkout = TempDir::default();

    let legacy = XCResult::new(
        path_str,
        ORG_URL_SLUG.clone(),
        REPO_FULL_NAME.clone(),
        false,
    )
    .unwrap();
    let declarations = XCResult::new_with_declaration_locations(
        path_str,
        ORG_URL_SLUG.clone(),
        REPO_FULL_NAME.clone(),
        empty_checkout.as_ref(),
        Limits::default(),
    )
    .unwrap();

    let expected = ids_and_timestamps(&legacy);
    assert!(
        !expected.is_empty(),
        "the fixture must have test cases for this to prove anything"
    );
    assert!(
        expected
            .iter()
            .all(|(_, id, timestamp)| !id.is_empty() && !timestamp.is_empty()),
        "the fixture must carry ids and timestamps on the legacy path"
    );
    pretty_assertions::assert_eq!(ids_and_timestamps(&declarations), expected);
}

// Before the fix this bundle emitted tests="2" failures="0" — the inner suite's two tests
// and its failure all vanished.
#[cfg(target_os = "macos")]
#[rstest]
#[case::experimental_failure_summary(true)]
#[case::legacy_fallback(false)]
fn test_a_nested_suite_is_flattened_rather_than_dropped(
    #[case] use_experimental_failure_summary: bool,
) {
    assert_junit(
        TEMP_DIR_TEST_NESTED_AND_PASSING
            .as_ref()
            .join("NestedAndPassing.xcresult"),
        use_experimental_failure_summary,
        include_str!("data/test-nested-and-passing.junit.xml"),
    );
}

// Three of these four passed, so no failure summary names a file for any of them. The
// status is asserted too, or the fixture could drift to all-failing and still pass here.
#[cfg(target_os = "macos")]
#[test]
fn test_declaration_locations_give_a_passing_test_its_file() {
    let report = declaration_report(
        TEMP_DIR_TEST_NESTED_AND_PASSING
            .as_ref()
            .join("NestedAndPassing.xcresult"),
        "tests/fixture-src/nested-and-passing",
    );
    let cases: std::collections::HashMap<String, &quick_junit::TestCase> = report
        .test_suites
        .iter()
        .flat_map(|test_suite| test_suite.test_cases.iter())
        .map(|test_case| (test_case.name.as_str().to_owned(), test_case))
        .collect();

    for (name, passed, expected) in [
        ("outerPasses()", true, "NestedAndPassingTests.swift"),
        ("topLevelPasses()", true, "NestedAndPassingTests.swift"),
        ("innerPasses()", true, "InnerSuite.swift"),
        ("innerFails()", false, "InnerSuite.swift"),
    ] {
        let test_case = cases
            .get(name)
            .unwrap_or_else(|| panic!("{name} is missing from the report"));
        assert_eq!(
            matches!(
                test_case.status,
                quick_junit::TestCaseStatus::Success { .. }
            ),
            passed,
            "{name} did not have the status the fixture was captured for"
        );
        let file = test_case
            .extra
            .iter()
            .find(|(key, _)| key.as_str() == "file")
            .map(|(_, value)| value.as_str())
            .unwrap_or_else(|| panic!("{name} got no file from its declaration"));
        assert!(
            file.ends_with(expected),
            "expected {name} to be declared in {expected}, got {file}"
        );
    }
}

// The flag is meant to move the `file` attribute and leave everything else alone, so every
// bundle the suite reads is run both ways and compared on all of it but that. Each case
// unpacks its own copy so another test cannot perturb the comparison.
#[cfg(target_os = "macos")]
#[rstest]
#[case::simple("tests/data/test1.xcresult.tar.gz", "test1.xcresult", None)]
#[case::complex("tests/data/test4.xcresult.tar.gz", "test4.xcresult", None)]
#[case::expected_failures(
    "tests/data/test-ExpectedFailures.xcresult.tar.gz",
    "test-ExpectedFailures.xcresult",
    None
)]
#[case::swift_mix(
    "tests/data/test-swift-mix.xcresult.tar.gz",
    "test-swift-mix.xcresult",
    None
)]
#[case::swift_without_test_suites(
    "tests/data/test-swift-without-test-suites.xcresult.tar.gz",
    "test-swift-without-test-suites.xcresult",
    None
)]
#[case::swift_snapshot_testing(
    "tests/data/test-swift-snapshot-testing.xcresult.tar.gz",
    "SnapshotRepro.xcresult",
    None
)]
#[case::dependency_raises_failure(
    "tests/data/test-dependency-raises-failure.xcresult.tar.gz",
    "DependencyRaisesFailure.xcresult",
    Some("tests/fixture-src/dependency-raises-failure")
)]
#[case::in_repo_helper_raises_failure(
    "tests/data/test-in-repo-helper-raises-failure.xcresult.tar.gz",
    "InRepoHelperRaisesFailure.xcresult",
    Some("tests/fixture-src/in-repo-helper-raises-failure")
)]
#[case::crash_in_dependency(
    "tests/data/test-crash-in-dependency.xcresult.tar.gz",
    "CrashInDependency.xcresult",
    Some("tests/fixture-src/crash-in-dependency")
)]
#[case::objc_xctest(
    "tests/data/test-objc-xctest.xcresult.tar.gz",
    "ObjcXCTest.xcresult",
    Some("tests/fixture-src/objc-xctest")
)]
#[case::toplevel_swift_testing(
    "tests/data/test-toplevel-swift-testing.xcresult.tar.gz",
    "ToplevelSwiftTesting.xcresult",
    Some("tests/fixture-src/toplevel-swift-testing")
)]
#[case::nested_and_passing(
    "tests/data/test-nested-and-passing.xcresult.tar.gz",
    "NestedAndPassing.xcresult",
    Some("tests/fixture-src/nested-and-passing")
)]
fn test_the_declaration_flag_moves_the_file_and_nothing_else(
    #[case] archive: &str,
    #[case] bundle: &str,
    #[case] repo_root: Option<&str>,
) {
    assert_the_declaration_flag_moves_only_the_file(archive, bundle, repo_root);
}
