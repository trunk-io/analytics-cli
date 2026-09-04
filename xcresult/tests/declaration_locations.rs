//! Where the declaration path attributes a test whose own suite does not declare it.
//!
//! The rest of the declaration-path coverage lives in `xcresult.rs` alongside the
//! failure-summary tests it is compared against. These two are separate because their
//! fixtures exist only to pin *which* declaration is reported when more than one file
//! could plausibly answer.

mod common;

use common::{
    ORG_URL_SLUG, REPO_FULL_NAME, assert_junit, assert_the_declaration_flag_moves_only_the_file,
    declaration_files, declaration_report, unpack_archive_to_temp_dir,
};
use lazy_static::lazy_static;
use rstest::rstest;
use temp_testdir::TempDir;
use xcresult::test_locations::{Limits, TestKey};
use xcresult::xcresult::XCResult;

lazy_static! {
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
    static ref TEMP_DIR_TEST_INHERITED_TEST: TempDir =
        unpack_archive_to_temp_dir("tests/data/test-inherited-test.xcresult.tar.gz");
    static ref TEMP_DIR_TEST_OBJC_CATEGORY: TempDir =
        unpack_archive_to_temp_dir("tests/data/test-objc-category.xcresult.tar.gz");
}

// XCTest runs a base class's `test*` method again under every concrete subclass, so the
// same method arrives twice under two different suites. Both raise the failure at the same
// line of `BaseTests.swift`, and `ConcreteTests.swift` is named nowhere in the bundle — so
// this only passes if the file comes from the suite that ran the test. Reporting the base
// class would hand `ConcreteTests`' failures to whoever owns `BaseTests.swift`, which is
// the misattribution the declaration path exists to prevent.
#[cfg(target_os = "macos")]
#[test]
fn test_an_inherited_test_is_attributed_to_the_suite_that_ran_it() {
    let report = common::declaration_report(
        TEMP_DIR_TEST_INHERITED_TEST
            .as_ref()
            .join("InheritedTest.xcresult"),
        "tests/fixture-src/inherited-test",
    );
    // Both suites run a case of the same name, so the suite has to be part of the key.
    let files = report
        .test_suites
        .iter()
        .flat_map(|test_suite| {
            test_suite.test_cases.iter().map(move |test_case| {
                (
                    format!("{}/{}", test_suite.name.as_str(), test_case.name.as_str()),
                    test_case
                        .extra
                        .iter()
                        .find(|(key, _)| key.as_str() == "file")
                        .map(|(_, value)| value.as_str().to_owned()),
                )
            })
        })
        .collect::<std::collections::HashMap<_, _>>();

    for (suite, expected) in [
        ("BaseTests", "BaseTests.swift"),
        ("ConcreteTests", "ConcreteTests.swift"),
    ] {
        let key = format!("InheritedTestTests.{suite}/testInheritedFails()");
        let file = files
            .get(&key)
            .unwrap_or_else(|| panic!("{key} is missing from the report (found {files:?})"))
            .as_deref()
            .unwrap_or_else(|| panic!("{key} got no file from its declaration"));
        assert!(
            file.ends_with(expected),
            "expected {key} to be attributed to {expected}, got {file}"
        );
    }
}

// A category's `documentSymbol` container is `ObjcCategoryTests(Extra)`, and the class's
// own file declares no tests at all — so if that name is not read back as the class it
// extends, the declaration is never matched and the file falls to the class's file.
#[cfg(target_os = "macos")]
#[test]
fn test_declaration_locations_resolve_a_test_declared_in_an_objc_category() {
    let files = common::declaration_files(
        TEMP_DIR_TEST_OBJC_CATEGORY
            .as_ref()
            .join("ObjcCategory.xcresult"),
        "tests/fixture-src/objc-category",
    );
    let file = files
        .get("testDeclaredInACategory")
        .unwrap_or_else(|| panic!("the test is missing from the report (found {files:?})"));
    assert!(
        file.ends_with("ObjcCategoryTests+Extra.m"),
        "expected the category's file, got {file}"
    );
}

#[rstest]
#[case::plain(
    "test://com.apple.xcode/InRepoHelper/InRepoHelperTests/Suite/case()",
    Some("InRepoHelperTests")
)]
#[case::percent_encoded(
    "test://com.apple.xcode/swift%20testing/swift%20testing%20exampleTests/helloWorld()",
    Some("swift testing exampleTests")
)]
#[case::too_short("test://com.apple.xcode/OnlyAScheme", None)]
#[case::not_a_url("Suite/case()", None)]
fn an_identifier_url_names_the_target(#[case] url: &str, #[case] expected: Option<&str>) {
    assert_eq!(
        TestKey::target_from_identifier_url(url).as_deref(),
        expected
    );
}

// A checkout that declares none of the tests. Where a failure surfaced is not where the
// test is written, so with nothing to resolve the reported file is absent rather than the
// helper the failure came from — that path resolves the wrong codeowners.
#[cfg(target_os = "macos")]
#[test]
fn test_a_test_with_no_declaration_in_the_checkout_gets_no_file() {
    let empty_checkout = TempDir::default();
    let report = common::declaration_report(
        TEMP_DIR_TEST_INHERITED_TEST
            .as_ref()
            .join("InheritedTest.xcresult"),
        empty_checkout.as_ref(),
    );
    let cases = report
        .test_suites
        .iter()
        .flat_map(|test_suite| test_suite.test_cases.iter())
        .collect::<Vec<_>>();
    assert!(
        !cases.is_empty(),
        "the bundle must still emit its test cases"
    );
    for test_case in cases {
        assert_eq!(
            test_case
                .extra
                .iter()
                .find(|(key, _)| key.as_str() == "file")
                .map(|(_, value)| value.as_str()),
            None,
            "{} was given a file with nothing declaring it",
            test_case.name.as_str()
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
#[case::inherited_test(
    "tests/data/test-inherited-test.xcresult.tar.gz",
    "InheritedTest.xcresult",
    Some("tests/fixture-src/inherited-test")
)]
#[case::objc_category(
    "tests/data/test-objc-category.xcresult.tar.gz",
    "ObjcCategory.xcresult",
    Some("tests/fixture-src/objc-category")
)]
fn test_the_declaration_flag_moves_the_file_and_nothing_else(
    #[case] archive: &str,
    #[case] bundle: &str,
    #[case] repo_root: Option<&str>,
) {
    assert_the_declaration_flag_moves_only_the_file(archive, bundle, repo_root);
}
