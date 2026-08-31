//! `swift test --xunit-output` reports no file at all, and needs no Xcode to produce — so
//! this is the shape the declaration path takes on Linux. See the fixture's README.

use std::{collections::HashMap, path::Path};

use xcresult::test_locations::{Limits, TestKey, TestLocationIndex};

const FIXTURE_ROOT: &str = "tests/fixture-src/swift-test-xunit";
const XUNIT: &str = include_str!("data/swift-test-xunit.junit.xml");
/// `--xunit-output` needs `--parallel` to emit XCTest results, and writes them to a separate
/// file from swift-testing's — which goes to `<name>-swift-testing.xml`.
const XUNIT_XCTEST: &str = include_str!("data/swift-test-xunit-xctest.junit.xml");

fn testcases(xunit: &str) -> Vec<(String, String)> {
    xunit
        .lines()
        .filter(|line| line.contains("<testcase"))
        .map(|line| {
            // `classname="` contains `name="`, so the needle has to include the separator.
            let attribute = |name: &str| {
                let needle = format!(" {name}=\"");
                let start = line.find(&needle).expect("attribute is present") + needle.len();
                line[start..]
                    .split('"')
                    .next()
                    .expect("attribute is terminated")
                    .to_owned()
            };
            (attribute("classname"), attribute("name"))
        })
        .collect()
}

fn resolve() -> HashMap<(String, String), String> {
    resolve_from(XUNIT)
}

fn resolve_from(xunit: &str) -> HashMap<(String, String), String> {
    let cases = testcases(xunit);
    assert!(!cases.is_empty(), "the fixture must have testcases");

    let keys = cases
        .iter()
        .map(|(classname, name)| {
            (
                TestKey::from_junit_classname(classname, name),
                TestKey::target_from_junit_classname(classname),
            )
        })
        .collect::<Vec<_>>();
    let index = TestLocationIndex::resolve(Path::new(FIXTURE_ROOT), &keys, Limits::default());

    cases
        .into_iter()
        .filter_map(|(classname, name)| {
            let key = TestKey::from_junit_classname(&classname, &name);
            index
                .lookup(&key)
                .map(|site| ((classname, name), site.file.as_str().to_owned()))
        })
        .collect()
}

#[test]
fn every_swift_testing_case_resolves_to_the_file_it_is_declared_in() {
    let resolved = resolve();
    for (classname, name, expected) in [
        ("MyCLITests", "helloworld()", "TopLevel.swift"),
        ("MyCLITests.AlphaSuite", "shared()", "Suites.swift"),
        ("MyCLITests.AlphaSuite.Inner", "deep()", "Suites.swift"),
        ("MyCLITests.BetaSuite", "shared()", "BetaSuite.swift"),
        (
            "MyCLITests.ParamSuite",
            "squares(n:)",
            "Parameterized.swift",
        ),
        (
            "MyCLITests.ParamSuite",
            "pairs(s:flag:)",
            "Parameterized.swift",
        ),
    ] {
        let key = (String::from(classname), String::from(name));
        let file = resolved
            .get(&key)
            .unwrap_or_else(|| panic!("{classname} {name} resolved to nothing"));
        assert!(
            file.ends_with(expected),
            "expected {classname} {name} in {expected}, got {file}"
        );
    }
}

// Collapsing the classname to its target would make one `shared()` borrow the other's file.
#[test]
fn two_suites_declaring_the_same_case_resolve_separately() {
    let resolved = resolve();
    let alpha = resolved
        .get(&(
            String::from("MyCLITests.AlphaSuite"),
            String::from("shared()"),
        ))
        .expect("alpha resolved");
    let beta = resolved
        .get(&(
            String::from("MyCLITests.BetaSuite"),
            String::from("shared()"),
        ))
        .expect("beta resolved");
    assert_ne!(alpha, beta, "both `shared()` cases resolved to {alpha}");
}

// XCTest goes to its own file and names a case `Module.Class` + a bare method, so it needs no
// separate handling — the innermost component is still the declaring type.
#[test]
fn an_xctest_case_resolves_to_the_class_that_declares_it() {
    let resolved = resolve_from(XUNIT_XCTEST);
    let file = resolved
        .get(&(
            String::from("MyCLITests.LegacyXCTests"),
            String::from("testOldStyle"),
        ))
        .expect("the XCTest case resolved");
    assert!(file.ends_with("Legacy.swift"), "got {file}");
}
