use clap::Parser;
use display::end_output::EndOutput;
use pretty_assertions::assert_eq;
use superconsole::Line;
use trunk_analytics_cli::validate_command::{ValidateArgs, run_validate};

#[derive(Parser)]
struct Validate {
    #[command(flatten)]
    args: ValidateArgs,
}

async fn validate_summary(args: &[&str]) -> Vec<String> {
    let Validate { args } = Validate::parse_from(["validate"].iter().chain(args));
    run_validate(args)
        .await
        .unwrap()
        .output()
        .unwrap()
        .iter()
        .map(Line::to_unstyled)
        // CODEOWNERS is printed as an absolute path.
        .map(|line| line.replace(env!("CARGO_MANIFEST_DIR"), "<cli>"))
        .collect()
}

#[tokio::test]
async fn prints_validation_errors_warnings_and_codeowners() {
    assert_eq!(
        validate_summary(&[
            "--junit-paths",
            "test_fixtures/summary/validate/*.xml",
            "--codeowners-path",
            "test_fixtures/summary/validate",
        ])
        .await,
        [
            "📚 Validating 4 files:",
            "  Files matching: test_fixtures/summary/validate/*.xml:",
            "    test_fixtures/summary/validate/broken.xml",
            "    test_fixtures/summary/validate/empty.xml",
            "    test_fixtures/summary/validate/multiple.xml",
            "    test_fixtures/summary/validate/unit.xml",
            "",
            "File parse issues",
            "test_fixtures/summary/validate/broken.xml",
            "1 errors, 0 warnings",
            " ❌ Errors:",
            "  ill-formed document: expected `</testsuite>`, but `</testsuites>` was foundCaused by:    expected `</testsuite>`, but `</testsuites>` was found",
            "",
            "test_fixtures/summary/validate/empty.xml",
            "0 errors, 1 warnings",
            " ⚠️  Warnings:",
            "  no reports found",
            "",
            "test_fixtures/summary/validate/multiple.xml",
            "1 errors, 0 warnings",
            " ❌ Errors:",
            "  multiple reports found",
            "",
            "Test Validation Issues",
            "test_fixtures/summary/validate/unit.xml",
            "1 test suites, 3 test cases, 1 errors, 3 warnings",
            " ❌ Errors:",
            "  test case name too short",
            " ⚠️  Warnings:",
            "  report has test cases with missing file or filepath",
            "  report has test cases with missing timestamp",
            "  test case or parent has no time duration",
            "",
            "Checking for codeowners file...",
            "  Found codeowners path: \"<cli>/test_fixtures/summary/validate/CODEOWNERS\"",
            "  CODEOWNERS found but test cases are missing filepaths. We will not be able to correlate flaky tests with owners.",
            "",
            "0 valid files, 2 files with warnings, and 3 files with errors, with 4 files total (a file is double counted if it has both errors and warnings)",
            "4 warnings, and 3 errors",
        ]
    );
}

#[tokio::test]
async fn prints_bep_parse_errors_without_a_codeowners_file() {
    assert_eq!(
        validate_summary(&[
            "--bazel-bep-path",
            "test_fixtures/summary/bep/bep_with_parse_error"
        ])
        .await,
        [
            "⚠️  BEP file had parse errors: [\"Error parsing build event: EOF while parsing a value at line 3 column 0\"]",
            "",
            "📚 Validating 1 files:",
            "  Files matching: test_fixtures/summary/bep/test.xml:",
            "    test_fixtures/summary/bep/test.xml",
            "",
            "File parse issues",
            "Test Validation Issues",
            "test_fixtures/summary/bep/test.xml",
            "1 test suites, 1 test cases, 0 errors, 1 warnings",
            " ⚠️  Warnings:",
            "  report has test cases with missing timestamp",
            "",
            "Checking for codeowners file...",
            "  No codeowners file found",
            "",
            "0 valid files, 1 file with warnings, and 0 files with errors, with 1 file total (a file is double counted if it has both errors and warnings)",
            "1 warning, and 0 errors",
        ]
    );
}
