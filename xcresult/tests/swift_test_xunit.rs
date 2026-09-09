//! `swift test --xunit-output` reports no file at all, and needs no Xcode to produce — so
//! this is the shape the declaration path takes on Linux. See the fixture's README.

use std::{collections::HashMap, path::Path};

use rstest::rstest;
use xcresult::test_locations::{Limits, TestKey, TestLocationIndex};
use xcresult::xcrun::find_program;

const FIXTURE_ROOT: &str = "tests/fixture-src/swift-test-xunit";
const XUNIT: &str = include_str!("data/swift-test-xunit.junit.xml");
/// `--xunit-output` needs `--parallel` to emit XCTest results, and writes them to a separate
/// file from swift-testing's — which goes to `<name>-swift-testing.xml`.
const XUNIT_XCTEST: &str = include_str!("data/swift-test-xunit-xctest.junit.xml");

/// Every case below resolves through a real `sourcekit-lsp` -- the fixture is all Swift, so
/// no `clangd` is involved. A missing server leaves the index empty rather than failing, so
/// without this the whole file fails on a machine that simply has no Swift toolchain.
///
/// Skipping is only safe while something still runs these, so `REQUIRE_LANGUAGE_SERVER` turns
/// the skip back into a failure. CI sets it wherever it has just put the toolchain on `PATH`,
/// which is what stops the coverage from lapsing there unnoticed.
fn language_server_is_available() -> bool {
    if find_program("sourcekit-lsp").is_some() {
        return true;
    }
    // Empty counts as unset: a workflow computing this per-runner writes `""` for the ones it
    // does not apply to, and `var_os` would otherwise read that as "required".
    let required = std::env::var("REQUIRE_LANGUAGE_SERVER").is_ok_and(|value| !value.is_empty());
    assert!(
        !required,
        "sourcekit-lsp is not on PATH and REQUIRE_LANGUAGE_SERVER is set -- the toolchain this \
         job is supposed to provide is missing, so these tests would have silently skipped"
    );
    eprintln!("skipping: sourcekit-lsp is not on PATH, so no declaration can be resolved");
    false
}

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
    if !language_server_is_available() {
        return;
    }
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

#[test]
fn overloads_differing_only_by_argument_label_resolve_separately() {
    if !language_server_is_available() {
        return;
    }
    let resolved = resolve();
    let file = |name: &str| {
        resolved
            .get(&(String::from("MyCLITests.OverloadSuite"), String::from(name)))
            .unwrap_or_else(|| panic!("{name} resolved to nothing"))
            .clone()
    };
    let (a, b, none) = (file("check(a:)"), file("check(b:)"), file("check()"));
    assert_ne!(a, b, "both labelled overloads resolved to {a}");
    assert!(a.ends_with("OverloadA.swift") && none.ends_with("OverloadA.swift"));
    assert!(b.ends_with("OverloadB.swift"));
}

// Collapsing the classname to its target would make one `shared()` borrow the other's file.
#[test]
fn two_suites_declaring_the_same_case_resolve_separately() {
    if !language_server_is_available() {
        return;
    }
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
//
// `ChildATests` is the case that cannot be answered. It declares no `testInherited` of its
// own, so the method it ran is written elsewhere — but `testInherited` is declared twice in
// this target, by `BaseTests` (which it inherits from) and by `ChildBTests` (which merely
// overrides it). An override is a declaration of the same name, so by name alone the ancestor
// and the sibling are the same fact, and telling them apart needs an inheritance hierarchy,
// which is semantic and would cost a build to resolve. Reporting `BaseTests.swift` here would
// be a guess that happens to be right; reporting the suite's own file is the honest answer,
// and `inherited_declaration` warns when it declines.
//
// Where only one class declares the case there is nothing to guess between, and the base
// class's file is reported — `test_an_inherited_test_is_attributed_to_the_class_that_declares_it`
// covers that shape.
#[rstest]
#[case::declared("MyCLITests.BaseTests", "BaseTests.swift")]
#[case::inherited_but_ambiguous("MyCLITests.ChildATests", "ChildATests.swift")]
#[case::overridden("MyCLITests.ChildBTests", "ChildBTests.swift")]
fn an_xctest_method_resolves_to_its_declaration_unless_two_classes_declare_it(
    #[case] classname: &str,
    #[case] expected: &str,
) {
    if !language_server_is_available() {
        return;
    }
    let resolved = resolve_from(XUNIT_XCTEST);
    let file = resolved
        .get(&(String::from(classname), String::from("testInherited")))
        .unwrap_or_else(|| panic!("{classname} resolved to nothing"));
    assert!(file.ends_with(expected), "expected {expected}, got {file}");
}

#[test]
fn an_xctest_case_resolves_to_the_class_that_declares_it() {
    if !language_server_is_available() {
        return;
    }
    let resolved = resolve_from(XUNIT_XCTEST);
    let file = resolved
        .get(&(
            String::from("MyCLITests.LegacyXCTests"),
            String::from("testOldStyle"),
        ))
        .expect("the XCTest case resolved");
    assert!(file.ends_with("Legacy.swift"), "got {file}");
}

/// One package captured both ways, which must land every test on the same file.
#[cfg(target_os = "macos")]
mod parity {
    use std::{fs::File, path::PathBuf};

    use flate2::read::GzDecoder;
    use tar::Archive;
    use temp_testdir::TempDir;
    use xcresult::xcresult::XCResult;

    use super::*;

    fn xcresult_report() -> Vec<quick_junit::Report> {
        let temp_dir = TempDir::default();
        Archive::new(GzDecoder::new(
            File::open("tests/data/swift-test-parity.xcresult.tar.gz").unwrap(),
        ))
        .unpack(temp_dir.as_ref())
        .unwrap();
        let bundle: PathBuf = temp_dir.as_ref().join("MyCLI.xcresult");

        let xcresult = XCResult::new_with_declaration_locations(
            bundle.to_str().unwrap(),
            String::from("trunk"),
            String::from("github.com/trunk-io/analytics-cli"),
            Path::new(FIXTURE_ROOT),
            Limits::default(),
        )
        .expect("the declaration path reads the bundle");

        xcresult.generate_junits()
    }

    fn without_parens(name: &str) -> String {
        name.trim_end_matches(['(', ')']).to_owned()
    }

    fn xcresult_raw_names() -> Vec<String> {
        xcresult_report()
            .iter()
            .flat_map(|report| report.test_suites.iter())
            .flat_map(|test_suite| test_suite.test_cases.iter())
            .map(|test_case| test_case.name.as_str().to_owned())
            .collect()
    }

    /// Pairs rather than a map keyed by name: two suites here both declare `shared()`.
    fn xcresult_pairs() -> Vec<(String, String)> {
        let mut pairs = xcresult_report()
            .iter()
            .flat_map(|report| report.test_suites.iter())
            .flat_map(|test_suite| test_suite.test_cases.iter())
            .filter_map(|test_case| {
                test_case
                    .extra
                    .iter()
                    .find(|(key, _)| key.as_str() == "file")
                    .map(|(_, value)| {
                        (
                            without_parens(test_case.name.as_str()),
                            file_name(value.as_str()),
                        )
                    })
            })
            .collect::<Vec<_>>();
        pairs.sort();
        pairs
    }

    fn xunit_pairs() -> Vec<(String, String)> {
        let mut pairs = [XUNIT, XUNIT_XCTEST]
            .into_iter()
            .flat_map(|xunit| resolve_from(xunit).into_iter())
            .map(|((_, name), file)| (without_parens(&name), file_name(&file)))
            .collect::<Vec<_>>();
        pairs.sort();
        pairs
    }

    fn file_name(path: &str) -> String {
        Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| String::from(path))
    }

    #[test]
    fn both_inputs_resolve_every_test_to_the_same_file() {
        let from_xcresult = xcresult_pairs();
        assert!(
            from_xcresult.len() >= 6,
            "the bundle should carry every test, got {from_xcresult:?}"
        );
        pretty_assertions::assert_eq!(xunit_pairs(), from_xcresult);
    }

    // Upstream, not ours: `name` feeds `gen_info_id_base`, so the same test arriving through
    // the two inputs does not land on one identity. Pinned so it cannot change unnoticed.
    #[test]
    fn the_two_inputs_spell_an_xctest_method_differently() {
        assert!(
            testcases(XUNIT_XCTEST)
                .iter()
                .any(|(_, name)| name == "testOldStyle"),
            "the xunit spells it with parens"
        );
        assert!(
            xcresult_raw_names().contains(&String::from("testOldStyle()")),
            "the bundle spells it {:?}",
            xcresult_raw_names()
        );
    }
}
