//! Where the declaration path attributes a test whose own suite does not declare it.
//!
//! The rest of the declaration-path coverage lives in `xcresult.rs` alongside the
//! failure-summary tests it is compared against. These two are separate because their
//! fixtures exist only to pin *which* declaration is reported when more than one file
//! could plausibly answer.

mod common;

use common::unpack_archive_to_temp_dir;
use lazy_static::lazy_static;
use temp_testdir::TempDir;

lazy_static! {
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
