use clap::Parser;
use pretty_assertions::assert_eq;
use superconsole::Line;
use trunk_analytics_cli::{
    summary_output::{ValidateReport, text},
    validate_command::{ValidateArgs, run_validate},
};

#[derive(Parser)]
struct Validate {
    #[command(flatten)]
    args: ValidateArgs,
}

async fn validate_report(args: &[&str]) -> ValidateReport {
    let Validate { args } = Validate::parse_from(["validate"].iter().chain(args));
    ValidateReport::new(&run_validate(args).await.unwrap())
}

fn unstyled(lines: anyhow::Result<Vec<Line>>) -> Vec<String> {
    lines
        .unwrap()
        .iter()
        .map(Line::to_unstyled)
        // CODEOWNERS is printed as an absolute path.
        .map(|line| line.replace(env!("CARGO_MANIFEST_DIR"), "<cli>"))
        .collect()
}

async fn validate_summary(args: &[&str]) -> Vec<String> {
    unstyled(text::render_validate(&validate_report(args).await))
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
async fn prints_bep_parse_errors_as_notices_without_a_codeowners_file() {
    let report = validate_report(&[
        "--bazel-bep-path",
        "test_fixtures/summary/bep/bep_with_parse_error",
    ])
    .await;
    assert_eq!(
        unstyled(text::render_notices(&report.notices)),
        [
            "⚠️  BEP file had a parse error: Error parsing build event: EOF while parsing a value at line 3 column 0",
            "",
        ]
    );
    assert_eq!(
        unstyled(text::render_validate(&report)),
        [
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

/// One test that ran two days ago: old enough for Trunk to drop, too recent for the validator
/// to discard as a defaulted timestamp.
fn old_timestamp_junit_xml() -> String {
    let timestamp = (chrono::Utc::now() - chrono::TimeDelta::days(2)).format("%Y-%m-%dT%H:%M:%SZ");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="old">
  <testsuite name="MathTests" timestamp="{timestamp}">
    <testcase name="adds" classname="MathTests" file="src/math.rs" time="0.1" timestamp="{timestamp}"/>
  </testsuite>
</testsuites>
"#
    )
}

#[tokio::test]
async fn says_that_results_with_old_timestamps_will_be_dropped() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("old.xml");
    std::fs::write(&path, old_timestamp_junit_xml()).unwrap();
    let report = validate_report(&["--junit-paths", path.to_str().unwrap()]).await;
    assert_eq!(report.counts.files_with_old_timestamps, 1);
    let summary = unstyled(text::render_validate(&report));
    assert_eq!(
        summary[summary.len() - 5..].to_vec(),
        [
            "0 valid files, 1 file with warnings, and 0 files with errors, with 1 file total (a file is double counted if it has both errors and warnings)",
            "1 warning, and 0 errors",
            "",
            "   ⚠️  1 file has test results older than 24 hours. Trunk will drop these results when they're uploaded.",
            "   If Bazel restored these files from its cache, upload with --bazel-bep-path to skip cached results: https://docs.trunk.io/flaky-tests/get-started/frameworks/bazel",
        ]
    );
}
