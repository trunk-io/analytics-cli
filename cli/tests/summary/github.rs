use std::fs;

use api::client::ClientErrorExplanation;
use assert_cmd::{Command, cargo};
use pretty_assertions::assert_eq;
use trunk_analytics_cli::{
    context_quarantine::QuarantineFetchStatus,
    error_report::{ErrorKind, ErrorReport, InterruptingError},
    exit_code::ExitCode,
    summary_output::{
        ErrorSummary, UploadCommand, UploadReport,
        github::annotations,
        json::{self, Report},
    },
    upload_command::UploadRunResult,
};

use super::{
    unauthorized_error,
    upload::{UPLOAD_ERROR_MESSAGE, failure, old_timestamp_validations, upload_result},
};

/// Writes the summary as `upload` would, then reads it back the way the command does.
fn upload_annotations(result: &UploadRunResult) -> Vec<String> {
    let report = UploadReport::new(result, UploadCommand::Upload);
    let summary = Report::Upload(&report)
        .to_json()
        .unwrap()
        .parse::<json::Summary>()
        .unwrap();
    annotations(&summary)
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn error_annotations(report: &ErrorReport) -> Vec<String> {
    let error = ErrorSummary::new(report);
    let summary = Report::Error(&error)
        .to_json()
        .unwrap()
        .parse::<json::Summary>()
        .unwrap();
    annotations(&summary)
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn reads_back_the_summary_it_wrote() {
    let mut result = upload_result(5);
    result.quarantine_context.exit_code = ExitCode::Failure;
    result.quarantine_context.failures = vec![failure("adds", "MathTests", Some("src/math.rs"))];
    let report = UploadReport::new(&result, UploadCommand::Upload);
    let written = Report::Upload(&report).to_json().unwrap();

    let json::Summary::Upload(read) = written.parse().unwrap() else {
        panic!("expected an upload summary");
    };
    assert_eq!(Report::Upload(&read).to_json().unwrap(), written);
}

#[test]
fn rejects_a_newer_schema_version() {
    let error = r#"{"schemaVersion": 2, "type": "ERROR"}"#.parse::<json::Summary>().unwrap_err();
    assert_eq!(
        error.to_string(),
        "schemaVersion 2 is newer than this CLI understands (1)"
    );
    assert!("{}".parse::<json::Summary>().is_err());
}

#[test]
fn reads_a_newer_schema_version_that_still_parses() {
    let error = ErrorSummary::new(&ErrorReport::new(
        unauthorized_error(),
        String::from("fake-org-slug"),
        None,
    ));
    let mut written: serde_json::Value =
        serde_json::from_str(&Report::Error(&error).to_json().unwrap()).unwrap();
    written["schemaVersion"] = serde_json::json!(2);
    written["kind"] = serde_json::json!("SOME_NEW_KIND");
    written["someNewField"] = serde_json::json!(true);

    let json::Summary::Error(read) = written.to_string().parse().unwrap() else {
        panic!("expected an error summary");
    };
    assert_eq!(read.kind, ErrorKind::Other);
}

#[test]
fn says_nothing_about_a_passing_run() {
    assert_eq!(upload_annotations(&upload_result(5)), Vec::<String>::new());
}

#[test]
fn says_nothing_when_no_tests_were_found_with_exit_code_0() {
    assert_eq!(upload_annotations(&upload_result(0)), Vec::<String>::new());
}

#[test]
fn says_the_test_step_failed_without_running_tests() {
    let mut result = upload_result(0);
    result.quarantine_context.exit_code = ExitCode::Failure;

    assert_eq!(
        upload_annotations(&result),
        [
            "::error::No tests were found in the provided test results, but non zero exit code provided: 1%0AThe test step appears to have failed without running tests -- see above for the original failure."
        ]
    );
}

#[test]
fn says_some_tests_may_not_have_run() {
    let mut result = upload_result(5);
    result.quarantine_context.exit_code = ExitCode::PassedThrough(2);

    assert_eq!(
        upload_annotations(&result),
        [
            "::error::No test failures found, but non zero exit code provided: 2%0AThis may indicate that some tests were not run or that there were other issues during the test run."
        ]
    );
}

#[test]
fn annotates_each_failure_that_was_not_quarantined() {
    let mut result = upload_result(5);
    result.quarantine_context.exit_code = ExitCode::Failure;
    result.quarantine_context.fetch_status = QuarantineFetchStatus::FetchSucceeded;
    result
        .quarantine_context
        .quarantine_status
        .quarantine_results = vec![failure("adds", "MathTests", Some("src/math.rs"))];
    let mut divides = failure("divides", "MathTests", None);
    divides.failure_text = Some(String::from("expected 1, got 2\n100% wrong"));
    result.quarantine_context.failures = vec![
        failure("subtracts", "Math, Tests: unit", Some("src/math.rs")),
        divides,
    ];

    assert_eq!(
        upload_annotations(&result),
        [
            "::error::2 tests failed and were not quarantined, using exit code: 1",
            "::error file=src/math.rs,title=Math%2C Tests%3A unit/subtracts::Failed%0Ahttps://app.trunk.io/test-org/flaky-tests/test/subtracts-id?repo=trunk-io%252Fanalytics-cli",
            "::error title=MathTests/divides::expected 1, got 2%0A100%25 wrong%0Ahttps://app.trunk.io/test-org/flaky-tests/test/divides-id?repo=trunk-io%252Fanalytics-cli",
        ]
    );
}

#[test]
fn says_why_failures_could_not_be_quarantined() {
    let mut result = upload_result(3);
    result.quarantine_context.exit_code = ExitCode::Failure;
    result.quarantine_context.fetch_status =
        QuarantineFetchStatus::FetchFailed(anyhow::anyhow!("request timed out"));
    result.quarantine_context.failures = vec![failure("adds", "MathTests", None)];

    assert_eq!(
        upload_annotations(&result)[0],
        "::error::1 test failed and was not quarantined, using exit code: 1%0AWe were unable to determine the quarantine status for tests. Any failing tests will be reported as failures"
    );

    result.quarantine_context.fetch_status = QuarantineFetchStatus::FetchSucceeded;
    result.disable_quarantining = true;
    assert_eq!(
        upload_annotations(&result)[0],
        "::error::1 test failed and was not quarantined, using exit code: 1%0AQuarantining was turned off with --disable-quarantining"
    );
}

#[test]
fn annotates_at_most_nine_failures() {
    let mut result = upload_result(12);
    result.quarantine_context.exit_code = ExitCode::Failure;
    result.quarantine_context.failures = (0..12)
        .map(|i| failure(&format!("test{i}"), "MathTests", None))
        .collect();

    let annotations = upload_annotations(&result);
    assert_eq!(annotations.len(), 10);
    assert_eq!(
        annotations[0],
        "::error::12 tests failed and were not quarantined, using exit code: 1%0AThe first 9 are annotated; see the log for the rest."
    );
}

#[test]
fn says_only_the_error_when_it_blocks_quarantining() {
    let mut result = upload_result(3);
    result.quarantine_context.exit_code = ExitCode::Failure;
    result.quarantine_context.failures = vec![failure("adds", "MathTests", None)];
    result.error_report = Some(ErrorReport::new(
        unauthorized_error(),
        String::from("test-org"),
        Some(String::from(UPLOAD_ERROR_MESSAGE)),
    ));

    assert_eq!(
        upload_annotations(&result),
        [
            "::error::There was an unexpected error that occurred while uploading test results%0AUnathorized access, your Trunk organization URL slug or token may be incorrect%0AHint: You can find it under the settings page at https://app.trunk.io/test-org/settings"
        ]
    );
}

#[test]
fn warns_about_an_upload_error_before_the_failures() {
    let mut result = upload_result(3);
    result.quarantine_context.exit_code = ExitCode::Failure;
    result.quarantine_context.failures = vec![failure("adds", "MathTests", None)];
    result.error_report = Some(ErrorReport::new(
        anyhow::anyhow!("HTTP status client error (400 Bad Request)")
            .context(ClientErrorExplanation {
                code: Some(String::from("SOME_CODE")),
                message: Some(String::from("Pass --test-collection-id.")),
            })
            .context("Error in create bundle upload endpoint"),
        String::from("test-org"),
        Some(String::from(UPLOAD_ERROR_MESSAGE)),
    ));

    assert_eq!(
        upload_annotations(&result)[..2].to_vec(),
        [
            "::warning::There was an unexpected error that occurred while uploading test results%0APass --test-collection-id.",
            "::error::1 test failed and was not quarantined, using exit code: 1",
        ]
    );
}

#[test]
fn warns_that_results_with_old_timestamps_will_be_dropped() {
    let mut result = upload_result(5);
    result.validations = old_timestamp_validations(2);

    assert_eq!(
        upload_annotations(&result),
        [
            "::warning::2 files have test results older than 24 hours. Trunk will drop these results, but the test counts include them.%0AIf Bazel restored these files from its cache, upload with --bazel-bep-path to skip cached results: https://docs.trunk.io/flaky-tests/get-started/frameworks/bazel"
        ]
    );
}

#[test]
fn reads_a_summary_written_before_old_timestamps_were_counted() {
    let mut result = upload_result(5);
    result.validations = old_timestamp_validations(1);
    let report = UploadReport::new(&result, UploadCommand::Upload);
    let mut written: serde_json::Value =
        serde_json::from_str(&Report::Upload(&report).to_json().unwrap()).unwrap();
    written
        .as_object_mut()
        .unwrap()
        .remove("filesWithOldTimestamps");
    written["validations"][0]
        .as_object_mut()
        .unwrap()
        .remove("oldTimestamps");

    let json::Summary::Upload(read) = written.to_string().parse().unwrap() else {
        panic!("expected an upload summary");
    };
    assert_eq!(read.files_with_old_timestamps, 0);
    assert!(!read.validations[0].old_timestamps);
}

#[test]
fn annotates_a_run_that_failed_outright() {
    let report = ErrorReport::new(
        anyhow::Error::from(InterruptingError::new(String::from(
            "No test output files found to upload.",
        ))),
        String::from("test-org"),
        None,
    );

    assert_eq!(
        error_annotations(&report),
        ["::error::No test output files found to upload."]
    );
}

fn github_annotations(summary_file: &std::path::Path) -> assert_cmd::assert::Assert {
    Command::new(cargo::cargo_bin!())
        .args(["github-annotations", "--summary-file"])
        .arg(summary_file)
        .assert()
}

#[test]
fn the_command_prints_annotations_and_exits_0() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("summary.json");
    let mut result = upload_result(0);
    result.quarantine_context.exit_code = ExitCode::Failure;
    let report = UploadReport::new(&result, UploadCommand::Upload);
    Report::Upload(&report).write_file(&path).unwrap();

    github_annotations(&path).success().stdout(
        "::error::No tests were found in the provided test results, but non zero exit code provided: 1%0AThe test step appears to have failed without running tests -- see above for the original failure.\n",
    );
}

#[test]
fn the_command_exits_non_zero_with_nothing_to_say() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("summary.json");
    let report = UploadReport::new(&upload_result(5), UploadCommand::Upload);
    Report::Upload(&report).write_file(&path).unwrap();

    github_annotations(&path).code(1).stdout("");

    fs::remove_file(&path).unwrap();
    github_annotations(&path).code(1).stdout("");
}
