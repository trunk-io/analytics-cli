use std::collections::{BTreeMap, HashMap};

use api::{client::ClientErrorExplanation, urls::TestCaseGuidScope};
use bundle::{
    BundleMeta, BundleMetaBaseProps, BundleMetaDebugProps, BundleMetaJunitProps,
    QuarantineResolutionMode, Test, TestCollectionProps,
};
use context::{
    junit::validator::{
        JunitReportValidation, JunitReportValidationIssueSubOptimal,
        JunitTestCaseValidationIssueInvalid, JunitTestCaseValidationIssueSubOptimal,
        JunitValidationIssue, JunitValidationIssueType,
    },
    repo::{BundleRepo, RepoUrlParts},
};
use pretty_assertions::assert_eq;
use superconsole::Line;
use trunk_analytics_cli::{
    context_quarantine::{QuarantineContext, QuarantineFetchStatus},
    error_report::ErrorReport,
    report::{
        Notice, UploadCommand, UploadReport,
        json::{self, Report},
        text::{self, TextOptions},
    },
    report_limiting::ValidationReport,
    upload_command::UploadRunResult,
    validate_command::JunitReportValidations,
};

use crate::unauthorized_error;

const BUNDLE_UPLOAD_ID: &str = "4b1d2c3e-5f60-4a7b-8c9d-0e1f2a3b4c5d";
const UPLOAD_ERROR_MESSAGE: &str =
    "There was an unexpected error that occurred while uploading test results";

fn upload_summary(result: &UploadRunResult) -> Vec<String> {
    let report = UploadReport::new(result, UploadCommand::Upload);
    text::render_upload(&report, &TextOptions::from(result))
        .unwrap()
        .iter()
        .map(Line::to_unstyled)
        .collect()
}

fn repo() -> RepoUrlParts {
    RepoUrlParts {
        host: String::from("github.com"),
        owner: String::from("trunk-io"),
        name: String::from("analytics-cli"),
    }
}

fn upload_result(num_tests: usize) -> UploadRunResult {
    let mut quarantine_context = QuarantineContext::skip_fetch(Vec::new());
    quarantine_context.repo = repo();
    quarantine_context.org_url_slug = String::from("test-org");
    UploadRunResult {
        error_report: None,
        quarantine_context,
        meta: BundleMeta {
            base_props: BundleMetaBaseProps {
                version: String::new(),
                cli_version: String::new(),
                org: String::from("test-org"),
                test_collection: None,
                repo: BundleRepo {
                    repo: repo(),
                    ..Default::default()
                },
                bundle_upload_id: String::from(BUNDLE_UPLOAD_ID),
                tags: Vec::new(),
                file_sets: Vec::new(),
                envs: HashMap::new(),
                upload_time_epoch: 0,
                test_command: None,
                os_info: None,
                quarantined_tests: Vec::new(),
                codeowners: None,
                use_uncloned_repo: None,
                quarantine_resolution_mode: QuarantineResolutionMode::default(),
            },
            junit_props: BundleMetaJunitProps {
                num_files: 1,
                num_tests,
            },
            debug_props: BundleMetaDebugProps {
                command_line: String::new(),
                trunk_envs: HashMap::new(),
            },
            bundle_upload_id_v2: String::new(),
            variant: None,
            internal_bundled_file: None,
            failed_tests: Vec::new(),
        },
        validations: JunitReportValidations::new(BTreeMap::new()),
        validation_report: ValidationReport::Limited,
        show_failure_messages: false,
        test_collection_short_id: None,
        hide_test_collection_links: false,
        api_address: String::from("https://api.trunk.io"),
        guid_scope: None,
        notices: Vec::new(),
    }
}

fn in_collection(mut result: UploadRunResult) -> UploadRunResult {
    result.test_collection_short_id = Some(String::from("abc123"));
    result.meta.base_props.test_collection = Some(TestCollectionProps {
        short_id: String::from("abc123"),
        bundle_meta_id: String::from("0b6f4c1e-8a7d-4e2b-9c3f-5d1a2b3c4d5e"),
        bundle_meta_created_at: String::from("2026-10-21T12:00:00Z"),
    });
    result
}

fn failure(name: &str, parent_name: &str, file: Option<&str>) -> Test {
    Test {
        name: String::from(name),
        parent_name: String::from(parent_name),
        class_name: None,
        file: file.map(String::from),
        id: format!("{name}-id"),
        timestamp_millis: None,
        is_quarantined: false,
        failure_message: None,
        variant: None,
    }
}

fn warning(issue: JunitTestCaseValidationIssueSubOptimal) -> JunitValidationIssueType {
    JunitValidationIssueType::TestCase(JunitValidationIssue::SubOptimal(issue))
}

fn report_warning(issue: JunitReportValidationIssueSubOptimal) -> JunitValidationIssueType {
    JunitValidationIssueType::Report(JunitValidationIssue::SubOptimal(issue))
}

fn name_too_short() -> JunitValidationIssueType {
    JunitValidationIssueType::TestCase(JunitValidationIssue::Invalid(
        JunitTestCaseValidationIssueInvalid::TestCaseNameTooShort(String::new()),
    ))
}

fn validation(all_issues: Vec<JunitValidationIssueType>) -> anyhow::Result<JunitReportValidation> {
    Ok(JunitReportValidation {
        all_issues,
        ..Default::default()
    })
}

fn validations() -> JunitReportValidations {
    JunitReportValidations::new(BTreeMap::from([
        (
            String::from("test-results/a.xml"),
            validation(vec![
                name_too_short(),
                name_too_short(),
                report_warning(
                    JunitReportValidationIssueSubOptimal::TestCasesFileOrFilepathMissing,
                ),
                report_warning(JunitReportValidationIssueSubOptimal::MissingTimestamps),
                report_warning(JunitReportValidationIssueSubOptimal::FutureTimestamps),
                report_warning(JunitReportValidationIssueSubOptimal::OldTimestamps),
                report_warning(JunitReportValidationIssueSubOptimal::StaleTimestamps),
                warning(JunitTestCaseValidationIssueSubOptimal::TestCaseNoTimeDuration),
                warning(
                    JunitTestCaseValidationIssueSubOptimal::TestCaseClassnameTooShort(String::new()),
                ),
                warning(JunitTestCaseValidationIssueSubOptimal::TestCaseNameTooLong(
                    String::new(),
                )),
                warning(
                    JunitTestCaseValidationIssueSubOptimal::TestCaseFileOrFilepathTooLong(
                        String::new(),
                    ),
                ),
            ]),
        ),
        (
            String::from("test-results/b.xml"),
            Err(anyhow::anyhow!("unexpected end of file")),
        ),
        (
            String::from("test-results/c.xml"),
            validation(vec![name_too_short()]),
        ),
        (
            String::from("test-results/d.xml"),
            validation(vec![warning(
                JunitTestCaseValidationIssueSubOptimal::TestCaseNoTimeDuration,
            )]),
        ),
        (String::from("test-results/e.xml"), validation(Vec::new())),
        (String::from("test-results/f.xml"), validation(Vec::new())),
        (String::from("test-results/g.xml"), validation(Vec::new())),
        (String::from("test-results/h.xml"), validation(Vec::new())),
        (
            String::from("test-results/i.xml"),
            validation(vec![report_warning(
                JunitReportValidationIssueSubOptimal::MissingTimestamps,
            )]),
        ),
    ]))
}

#[test]
fn prints_a_passing_run() {
    let result = upload_result(5);

    assert_eq!(
        upload_summary(&result),
        [
            "🏷️  Bundle Upload ID: 4b1d2c3e-5f60-4a7b-8c9d-0e1f2a3b4c5d",
            "",
            "📚 Test Report",
            "   Total: 5   Pass: 5   Fail: 0   Quarantined: 0   Pass Ratio: 100.0%",
            "",
            "🎉 No test failures found!",
        ]
    );
}

#[test]
fn prints_that_no_tests_were_found() {
    let result = upload_result(0);

    assert_eq!(
        upload_summary(&result),
        [
            "🏷️  Bundle Upload ID: 4b1d2c3e-5f60-4a7b-8c9d-0e1f2a3b4c5d",
            "",
            "📚 Test Report",
            "   Total: 0   Pass: 0   Fail: 0   Quarantined: 0   Pass Ratio: N/A",
            "",
            "⚠️  No tests were found in the provided test results",
        ]
    );
}

#[test]
fn prints_failures_that_were_all_quarantined() {
    let mut result = upload_result(4);
    result.quarantine_context.fetch_status = QuarantineFetchStatus::FetchSucceeded;
    result
        .quarantine_context
        .quarantine_status
        .group_is_quarantined = true;
    result
        .quarantine_context
        .quarantine_status
        .quarantine_results = vec![
        failure("adds", "MathTests", Some("src/math.rs")),
        failure("subtracts", "MathTests", Some("src/math.rs")),
    ];

    assert_eq!(
        upload_summary(&result),
        [
            "🏷️  Bundle Upload ID: 4b1d2c3e-5f60-4a7b-8c9d-0e1f2a3b4c5d",
            "",
            "📚 Test Report",
            "   Total: 4   Pass: 2   Fail: 0   Quarantined: 2   Pass Ratio: 50.0%",
            "",
            "❤️‍🩹  2 tests failed and were quarantined",
            "",
            "  📁 src/math.rs",
            "    MathTests/adds",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/adds-id?repo=trunk-io%2Fanalytics-cli",
            "    MathTests/subtracts",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/subtracts-id?repo=trunk-io%2Fanalytics-cli",
            "  ",
            "",
            "🎉 All test failures were quarantined, overriding exit code to be exit_success (0)",
        ]
    );
}

#[test]
fn prints_failures_that_were_not_quarantined() {
    let mut result = upload_result(5);
    result.quarantine_context.exit_code = 1;
    result.quarantine_context.fetch_status = QuarantineFetchStatus::FetchSucceeded;
    result
        .quarantine_context
        .quarantine_status
        .quarantine_results = vec![failure("adds", "MathTests", Some("src/math.rs"))];
    result.quarantine_context.failures = vec![
        failure("subtracts", "MathTests", Some("src/math.rs")),
        failure("divides", "MathTests", Some("src/math.rs")),
    ];

    assert_eq!(
        upload_summary(&result),
        [
            "🏷️  Bundle Upload ID: 4b1d2c3e-5f60-4a7b-8c9d-0e1f2a3b4c5d",
            "",
            "📚 Test Report",
            "   Total: 5   Pass: 2   Fail: 2   Quarantined: 1   Pass Ratio: 40.0%",
            "",
            "❤️‍🩹  1 test failed and was quarantined",
            "",
            "  📁 src/math.rs",
            "    MathTests/adds",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/adds-id?repo=trunk-io%2Fanalytics-cli",
            "  ",
            "",
            "❌ 2 tests failed and were not quarantined",
            "",
            "  📁 src/math.rs",
            "    MathTests/subtracts",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/subtracts-id?repo=trunk-io%2Fanalytics-cli",
            "    MathTests/divides",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/divides-id?repo=trunk-io%2Fanalytics-cli",
            "  ",
            "",
            "⚠️  Some test failures were not quarantined, using exit code: 1",
        ]
    );
}

#[test]
fn prints_a_warning_when_the_quarantine_lookup_failed() {
    let mut result = upload_result(3);
    result.quarantine_context.exit_code = 1;
    result.quarantine_context.fetch_status =
        QuarantineFetchStatus::FetchFailed(anyhow::anyhow!("request timed out"));
    result.quarantine_context.failures = vec![failure("adds", "MathTests", None)];

    assert_eq!(
        upload_summary(&result),
        [
            "🏷️  Bundle Upload ID: 4b1d2c3e-5f60-4a7b-8c9d-0e1f2a3b4c5d",
            "",
            "📚 Test Report",
            "   Total: 3   Pass: 2   Fail: 1   Quarantined: 0   Pass Ratio: 66.7%",
            "   We were unable to determine the quarantine status for tests. Any failing tests will be reported as failures",
            "",
            "❌ 1 test failed and was not quarantined",
            "",
            "  📦 MathTests",
            "    MathTests/adds",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/adds-id?repo=trunk-io%2Fanalytics-cli",
            "  ",
            "",
            "⚠️  Some test failures were not quarantined, using exit code: 1",
        ]
    );
}

#[test]
fn stops_at_an_error_that_blocks_quarantining() {
    let mut result = upload_result(3);
    result.quarantine_context.exit_code = 1;
    result.quarantine_context.failures = vec![failure("adds", "MathTests", None)];
    result.error_report = Some(ErrorReport::new(
        unauthorized_error(),
        String::from("test-org"),
        Some(String::from(UPLOAD_ERROR_MESSAGE)),
    ));

    assert_eq!(
        upload_summary(&result),
        [
            "",
            "Error Encountered",
            "There was an unexpected error that occurred while uploading test results",
            "",
            "Unathorized access, your Trunk organization URL slug or token may be incorrect",
            "  Hint: You can find it under the settings page at https://app.trunk.io/test-org/settings",
        ]
    );
}

#[test]
fn proceeds_past_an_upload_error() {
    let mut result = upload_result(3);
    result.meta.base_props.bundle_upload_id = String::new();
    result.quarantine_context.exit_code = 1;
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
        upload_summary(&result),
        [
            "",
            "Error Encountered",
            "There was an unexpected error that occurred while uploading test results",
            "",
            "Pass --test-collection-id.",
            "",
            "For more help, contact us at https://slack.trunk.io/",
            "",
            "Errors occurred during execution, using quarantining exit code",
            "Proceeding with test report...",
            "",
            "📚 Test Report",
            "   Total: 3   Pass: 2   Fail: 1   Quarantined: 0   Pass Ratio: 66.7%",
            "",
            "❌ 1 test failed and was not quarantined",
            "",
            "  📦 MathTests",
            "    MathTests/adds",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/adds-id?repo=trunk-io%2Fanalytics-cli",
            "  ",
            "",
            "⚠️  Some test failures were not quarantined, using exit code: 1",
        ]
    );
}

#[test]
fn prints_a_limited_validation_report() {
    let mut result = upload_result(5);
    result.validations = validations();

    assert_eq!(
        upload_summary(&result),
        [
            "🏷️  Bundle Upload ID: 4b1d2c3e-5f60-4a7b-8c9d-0e1f2a3b4c5d",
            "",
            "📂 File Validation",
            "",
            "❕ 9 files found, 5 with issues",
            "",
            "  ❌ test-results/a.xml Has Errors And Warnings",
            "   ↪ ❌ Errors",
            "     ↪ test case name too short",
            "     ↪ test case name too short",
            "   ↪ ⚠️  Warnings",
            "     ↪ report has test cases with missing file or filepath",
            "     ↪ report has test cases with missing timestamp",
            "     ↪ report has test cases with future timestamp",
            "     ↪ report has old (> 24 hour(s)) timestamps",
            "     ↪ report has stale (> 1 hour(s)) timestamps",
            "     ↪ test case or parent has no time duration",
            "     ↪ test case classname too short",
            "     ↪ test case name too long, truncated to 1000",
            "  ❌ test-results/c.xml Has Errors",
            "   ↪ test case name too short",
            "  ⚠️  test-results/d.xml Has Warnings",
            "   ↪ test case or parent has no time duration",
            "…and 1 more",
            "",
            "📚 Test Report",
            "   Total: 5   Pass: 5   Fail: 0   Quarantined: 0   Pass Ratio: 100.0%",
            "",
            "🎉 No test failures found!",
        ]
    );
}

#[test]
fn prints_a_full_validation_report() {
    let mut result = upload_result(5);
    result.validations = validations();
    result.validation_report = ValidationReport::Full;

    assert_eq!(
        upload_summary(&result),
        [
            "🏷️  Bundle Upload ID: 4b1d2c3e-5f60-4a7b-8c9d-0e1f2a3b4c5d",
            "",
            "📂 File Validation",
            "",
            "❕ 9 files found, 5 with issues",
            "",
            "  ❌ test-results/a.xml Has Errors And Warnings",
            "   ↪ ❌ Errors",
            "     ↪ test case name too short",
            "     ↪ test case name too short",
            "   ↪ ⚠️  Warnings",
            "     ↪ report has test cases with missing file or filepath",
            "     ↪ report has test cases with missing timestamp",
            "     ↪ report has test cases with future timestamp",
            "     ↪ report has old (> 24 hour(s)) timestamps",
            "     ↪ report has stale (> 1 hour(s)) timestamps",
            "     ↪ test case or parent has no time duration",
            "     ↪ test case classname too short",
            "     ↪ test case name too long, truncated to 1000",
            "     ↪ test case file or filepath too long",
            "  ❌ test-results/c.xml Has Errors",
            "   ↪ test case name too short",
            "  ⚠️  test-results/d.xml Has Warnings",
            "   ↪ test case or parent has no time duration",
            "  ⚠️  test-results/i.xml Has Warnings",
            "   ↪ report has test cases with missing timestamp",
            "",
            "📚 Test Report",
            "   Total: 5   Pass: 5   Fail: 0   Quarantined: 0   Pass Ratio: 100.0%",
            "",
            "🎉 No test failures found!",
        ]
    );
}

#[test]
fn prints_a_blank_line_for_no_validation_report() {
    let mut result = upload_result(5);
    result.validations = validations();
    result.validation_report = ValidationReport::None;

    assert_eq!(
        upload_summary(&result),
        [
            "🏷️  Bundle Upload ID: 4b1d2c3e-5f60-4a7b-8c9d-0e1f2a3b4c5d",
            "",
            "",
            "📚 Test Report",
            "   Total: 5   Pass: 5   Fail: 0   Quarantined: 0   Pass Ratio: 100.0%",
            "",
            "🎉 No test failures found!",
        ]
    );
}

#[test]
fn prints_validation_when_every_file_is_correct() {
    let mut result = upload_result(5);
    result.validations = JunitReportValidations::new(BTreeMap::from([(
        String::from("test-results/a.xml"),
        validation(Vec::new()),
    )]));

    assert_eq!(
        upload_summary(&result),
        [
            "🏷️  Bundle Upload ID: 4b1d2c3e-5f60-4a7b-8c9d-0e1f2a3b4c5d",
            "",
            "📂 File Validation",
            "",
            "✅ 1 file found, all fully correct",
            "",
            "",
            "📚 Test Report",
            "   Total: 5   Pass: 5   Fail: 0   Quarantined: 0   Pass Ratio: 100.0%",
            "",
            "🎉 No test failures found!",
        ]
    );
}

#[test]
fn prints_long_failure_messages_when_asked() {
    let mut result = upload_result(1);
    result.show_failure_messages = true;
    result.quarantine_context.exit_code = 1;
    let mut message_lines = vec![
        String::from("assertion `left == right` failed\r"),
        String::from("\tleft: 1"),
        String::new(),
        String::from("   "),
        String::from("\tright: 2"),
    ];
    message_lines.extend((6..=23).map(|n| format!("frame {n}")));
    result.quarantine_context.failures = vec![Test {
        failure_message: Some(message_lines.join("\n")),
        ..failure("adds", "MathTests", None)
    }];

    assert_eq!(
        upload_summary(&result),
        [
            "🏷️  Bundle Upload ID: 4b1d2c3e-5f60-4a7b-8c9d-0e1f2a3b4c5d",
            "",
            "📚 Test Report",
            "   Total: 1   Pass: 0   Fail: 1   Quarantined: 0   Pass Ratio: 0.0%",
            "",
            "❌ 1 test failed and was not quarantined",
            "",
            "  📦 MathTests",
            "    MathTests/adds",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/adds-id?repo=trunk-io%2Fanalytics-cli",
            "      Failure: ",
            "         assertion `left == right` failed",
            "             left: 1",
            "             right: 2",
            "         frame 6",
            "         frame 7",
            "         frame 8",
            "         frame 9",
            "         frame 10",
            "         frame 11",
            "         frame 12",
            "         frame 13",
            "         frame 14",
            "         frame 15",
            "         frame 16",
            "         frame 17",
            "         frame 18",
            "         frame 19",
            "         frame 20",
            "         …and 3 more lines not shown",
            "  ",
            "",
            "⚠️  Some test failures were not quarantined, using exit code: 1",
        ]
    );
}

#[test]
fn prints_the_upload_as_a_collection_link() {
    let mut result = in_collection(upload_result(2));
    result.quarantine_context.exit_code = 1;
    result.quarantine_context.failures = vec![Test {
        id: String::from("c33a7f64-8f3e-5db9-b37b-2ea870d2441b"),
        ..failure("adds", "MathTests", None)
    }];
    result.guid_scope = Some(TestCaseGuidScope {
        test_collection_id: String::from("9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d"),
        repo_id: String::from("1f2e3d4c-5b6a-4978-8a6b-5c4d3e2f1a0b"),
    });

    assert_eq!(
        upload_summary(&result),
        [
            "🏷️  Upload: https://app.trunk.io/test-org/flaky-tests/collections/abc123/uploads/eyJpZCI6IjBiNmY0YzFlLThhN2QtNGUyYi05YzNmLTVkMWEyYjNjNGQ1ZSIsImNyZWF0ZWRBdCI6MTc5MjU4NDAwMDAwMH0",
            "",
            "📚 Test Report",
            "   Total: 2   Pass: 1   Fail: 1   Quarantined: 0   Pass Ratio: 50.0%",
            "",
            "❌ 1 test failed and was not quarantined",
            "",
            "  📦 MathTests",
            "    MathTests/adds",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/collections/abc123/tests/81d0929e-fc06-8b61-8506-991d8ebc51ea",
            "  ",
            "",
            "⚠️  Some test failures were not quarantined, using exit code: 1",
        ]
    );
}

#[test]
fn links_tests_by_short_link_without_collection_ids() {
    let mut result = in_collection(upload_result(2));
    result.quarantine_context.exit_code = 1;
    result.quarantine_context.failures = vec![failure("adds", "MathTests", None)];

    assert_eq!(
        upload_summary(&result),
        [
            "🏷️  Upload: https://app.trunk.io/test-org/flaky-tests/collections/abc123/uploads/eyJpZCI6IjBiNmY0YzFlLThhN2QtNGUyYi05YzNmLTVkMWEyYjNjNGQ1ZSIsImNyZWF0ZWRBdCI6MTc5MjU4NDAwMDAwMH0",
            "",
            "📚 Test Report",
            "   Total: 2   Pass: 1   Fail: 1   Quarantined: 0   Pass Ratio: 50.0%",
            "",
            "❌ 1 test failed and was not quarantined",
            "",
            "  📦 MathTests",
            "    MathTests/adds",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/collections/abc123/t/adds-id?repo=trunk-io%2Fanalytics-cli",
            "  ",
            "",
            "⚠️  Some test failures were not quarantined, using exit code: 1",
        ]
    );
}

#[test]
fn prints_the_bundle_upload_id_when_collection_links_are_hidden() {
    let mut result = in_collection(upload_result(2));
    result.hide_test_collection_links = true;
    result.quarantine_context.exit_code = 1;
    result.quarantine_context.failures = vec![Test {
        id: String::from("c33a7f64-8f3e-5db9-b37b-2ea870d2441b"),
        ..failure("adds", "MathTests", None)
    }];
    result.guid_scope = Some(TestCaseGuidScope {
        test_collection_id: String::from("9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d"),
        repo_id: String::from("1f2e3d4c-5b6a-4978-8a6b-5c4d3e2f1a0b"),
    });

    assert_eq!(
        upload_summary(&result),
        [
            "🏷️  Bundle Upload ID: 4b1d2c3e-5f60-4a7b-8c9d-0e1f2a3b4c5d",
            "",
            "📚 Test Report",
            "   Total: 2   Pass: 1   Fail: 1   Quarantined: 0   Pass Ratio: 50.0%",
            "",
            "❌ 1 test failed and was not quarantined",
            "",
            "  📦 MathTests",
            "    MathTests/adds",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/c33a7f64-8f3e-5db9-b37b-2ea870d2441b?repo=trunk-io%2Fanalytics-cli",
            "  ",
            "",
            "⚠️  Some test failures were not quarantined, using exit code: 1",
        ]
    );
}

#[test]
fn omits_the_upload_line_without_a_bundle_upload_id() {
    let mut result = upload_result(5);
    result.meta.base_props.bundle_upload_id = String::new();

    assert_eq!(
        upload_summary(&result),
        [
            "📚 Test Report",
            "   Total: 5   Pass: 5   Fail: 0   Quarantined: 0   Pass Ratio: 100.0%",
            "",
            "🎉 No test failures found!",
        ]
    );
}

#[test]
fn groups_failures_by_file_then_suite_then_neither() {
    let mut result = upload_result(10);
    result.quarantine_context.exit_code = 1;
    result.quarantine_context.failures = vec![
        failure("parses_dates", "DateTests", Some("src/date.rs")),
        failure("adds", "MathTests", None),
        failure("loads", "", None),
        failure("parses_times", "TimeTests", Some("src/date.rs")),
        failure("subtracts", "MathTests", None),
        failure("boots", "", None),
        failure("formats", "", Some("src/a.rs")),
    ];

    assert_eq!(
        upload_summary(&result),
        [
            "🏷️  Bundle Upload ID: 4b1d2c3e-5f60-4a7b-8c9d-0e1f2a3b4c5d",
            "",
            "📚 Test Report",
            "   Total: 10   Pass: 3   Fail: 7   Quarantined: 0   Pass Ratio: 30.0%",
            "",
            "❌ 7 tests failed and were not quarantined",
            "",
            "  📁 src/a.rs",
            "    formats",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/formats-id?repo=trunk-io%2Fanalytics-cli",
            "  ",
            "  📁 src/date.rs",
            "    DateTests/parses_dates",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/parses_dates-id?repo=trunk-io%2Fanalytics-cli",
            "    TimeTests/parses_times",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/parses_times-id?repo=trunk-io%2Fanalytics-cli",
            "  ",
            "  📦 MathTests",
            "    MathTests/adds",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/adds-id?repo=trunk-io%2Fanalytics-cli",
            "    MathTests/subtracts",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/subtracts-id?repo=trunk-io%2Fanalytics-cli",
            "  ",
            "  🔍 Other",
            "    loads",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/loads-id?repo=trunk-io%2Fanalytics-cli",
            "    boots",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/boots-id?repo=trunk-io%2Fanalytics-cli",
            "  ",
            "",
            "⚠️  Some test failures were not quarantined, using exit code: 1",
        ]
    );
}

#[test]
fn shows_three_failures_per_group() {
    let mut result = upload_result(5);
    result.quarantine_context.exit_code = 1;
    result.quarantine_context.failures = ["adds", "subtracts", "multiplies", "divides", "rounds"]
        .into_iter()
        .map(|name| failure(name, "MathTests", Some("src/math.rs")))
        .collect();

    assert_eq!(
        upload_summary(&result),
        [
            "🏷️  Bundle Upload ID: 4b1d2c3e-5f60-4a7b-8c9d-0e1f2a3b4c5d",
            "",
            "📚 Test Report",
            "   Total: 5   Pass: 0   Fail: 5   Quarantined: 0   Pass Ratio: 0.0%",
            "",
            "❌ 5 tests failed and were not quarantined",
            "",
            "  📁 src/math.rs",
            "    MathTests/adds",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/adds-id?repo=trunk-io%2Fanalytics-cli",
            "    MathTests/subtracts",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/subtracts-id?repo=trunk-io%2Fanalytics-cli",
            "    MathTests/multiplies",
            "      ⤷ https://app.trunk.io/test-org/flaky-tests/test/multiplies-id?repo=trunk-io%2Fanalytics-cli",
            "    …and 2 more failures in this group",
            "  ",
            "",
            "⚠️  Some test failures were not quarantined, using exit code: 1",
        ]
    );
}

#[test]
fn warns_about_a_non_zero_exit_code_without_failures() {
    let mut result = upload_result(5);
    result.quarantine_context.exit_code = 1;

    assert_eq!(
        upload_summary(&result),
        [
            "🏷️  Bundle Upload ID: 4b1d2c3e-5f60-4a7b-8c9d-0e1f2a3b4c5d",
            "",
            "📚 Test Report",
            "   Total: 5   Pass: 5   Fail: 0   Quarantined: 0   Pass Ratio: 100.0%",
            "",
            "⚠️  No test failures found, but non zero exit code provided: 1",
            "This may indicate that some tests were not run or that there were other issues during the test run.",
        ]
    );
}

#[test]
fn upload_exits_with_the_exit_code_of_an_error_but_test_does_not() {
    let mut result = upload_result(3);
    result.quarantine_context.exit_code = 1;
    result.error_report = Some(ErrorReport::new(
        unauthorized_error(),
        String::from("test-org"),
        Some(String::from(UPLOAD_ERROR_MESSAGE)),
    ));

    assert_eq!(
        UploadReport::new(&result, UploadCommand::Upload).exit_code,
        70
    );
    assert_eq!(UploadReport::new(&result, UploadCommand::Test).exit_code, 1);
}

#[test]
fn exits_with_the_test_run_exit_code_when_an_error_has_none() {
    let mut result = upload_result(3);
    result.quarantine_context.exit_code = 1;
    result.error_report = Some(ErrorReport::new(
        anyhow::anyhow!("HTTP status server error (500 Internal Server Error)"),
        String::from("test-org"),
        Some(String::from(UPLOAD_ERROR_MESSAGE)),
    ));

    assert_eq!(
        UploadReport::new(&result, UploadCommand::Upload).exit_code,
        1
    );
    assert_eq!(UploadReport::new(&result, UploadCommand::Test).exit_code, 1);
}

fn upload_json(result: &UploadRunResult) -> serde_json::Value {
    let report = UploadReport::new(result, UploadCommand::Upload);
    serde_json::from_str(&json::to_string(Report::Upload(&report)).unwrap()).unwrap()
}

#[test]
fn writes_the_report_as_json() {
    let mut result = in_collection(upload_result(10));
    result.error_report = Some(ErrorReport::new(
        anyhow::anyhow!("HTTP status server error (500 Internal Server Error)")
            .context("Failed to create bundle upload"),
        String::from("test-org"),
        Some(String::from(UPLOAD_ERROR_MESSAGE)),
    ));
    result.validations = JunitReportValidations::new(BTreeMap::from([
        (
            String::from("test-results/a.xml"),
            validation(vec![
                name_too_short(),
                warning(JunitTestCaseValidationIssueSubOptimal::TestCaseNoTimeDuration),
            ]),
        ),
        (
            String::from("test-results/b.xml"),
            Err(anyhow::anyhow!("unexpected end of file")),
        ),
    ]));
    result.quarantine_context.exit_code = 1;
    result.quarantine_context.fetch_status = QuarantineFetchStatus::FetchSucceeded;
    result
        .quarantine_context
        .quarantine_status
        .quarantine_results = vec![failure("adds", "MathTests", Some("src/math.rs"))];
    result.quarantine_context.failures = vec![
        Test {
            class_name: Some(String::from("Math")),
            failure_message: Some(String::from("expected 1, got 2")),
            variant: Some(String::from("linux")),
            ..failure("subtracts", "MathTests", Some("src/math.rs"))
        },
        failure("boots", "", None),
    ];
    result.notices = vec![Notice::deprecation(
        "The --team flag is deprecated and will be removed in a future version.",
    )];

    assert_eq!(
        upload_json(&result),
        serde_json::json!({
            "schemaVersion": 1,
            "type": "upload",
            "error": {
                "kind": "other",
                "message": "There was an unexpected error that occurred while uploading test results",
                "error": "Failed to create bundle upload",
                "causes": ["HTTP status server error (500 Internal Server Error)"],
                "explanation": null,
                "settingsUrl": null,
                "exitCode": null,
            },
            "bundleUploadId": "4b1d2c3e-5f60-4a7b-8c9d-0e1f2a3b4c5d",
            "uploadUrl": "https://app.trunk.io/test-org/flaky-tests/collections/abc123/uploads/eyJpZCI6IjBiNmY0YzFlLThhN2QtNGUyYi05YzNmLTVkMWEyYjNjNGQ1ZSIsImNyZWF0ZWRBdCI6MTc5MjU4NDAwMDAwMH0",
            "validations": [
                {
                    "file": "test-results/a.xml",
                    "parseError": null,
                    "errors": ["test case name too short"],
                    "warnings": ["test case or parent has no time duration"],
                    "testSuiteCount": 0,
                    "testCaseCount": 0,
                },
                {
                    "file": "test-results/b.xml",
                    "parseError": {
                        "error": "unexpected end of file",
                        "causes": [],
                    },
                    "errors": [],
                    "warnings": [],
                    "testSuiteCount": null,
                    "testCaseCount": null,
                },
            ],
            "counts": {
                "total": 10,
                "passed": 7,
                "failed": 2,
                "quarantined": 1,
                "passRatio": 0.7,
            },
            "quarantineLookupFailed": false,
            "quarantined": [
                {
                    "key": { "file": "src/math.rs" },
                    "tests": [
                        {
                            "name": "adds",
                            "suite": "MathTests",
                            "className": null,
                            "file": "src/math.rs",
                            "id": "adds-id",
                            "variant": null,
                            "failureMessage": null,
                            "quarantined": true,
                            "link": "https://app.trunk.io/test-org/flaky-tests/collections/abc123/t/adds-id?repo=trunk-io%2Fanalytics-cli",
                        },
                    ],
                },
            ],
            "failures": [
                {
                    "key": { "file": "src/math.rs" },
                    "tests": [
                        {
                            "name": "subtracts",
                            "suite": "MathTests",
                            "className": "Math",
                            "file": "src/math.rs",
                            "id": "subtracts-id",
                            "variant": "linux",
                            "failureMessage": "expected 1, got 2",
                            "quarantined": false,
                            "link": "https://app.trunk.io/test-org/flaky-tests/collections/abc123/t/subtracts-id?repo=trunk-io%2Fanalytics-cli",
                        },
                    ],
                },
                {
                    "key": "other",
                    "tests": [
                        {
                            "name": "boots",
                            "suite": "",
                            "className": null,
                            "file": null,
                            "id": "boots-id",
                            "variant": null,
                            "failureMessage": null,
                            "quarantined": false,
                            "link": "https://app.trunk.io/test-org/flaky-tests/collections/abc123/t/boots-id?repo=trunk-io%2Fanalytics-cli",
                        },
                    ],
                },
            ],
            "outcome": "failuresNotQuarantined",
            "exitCode": 1,
            "notices": [
                {
                    "kind": "deprecation",
                    "message": "The --team flag is deprecated and will be removed in a future version.",
                },
            ],
        })
    );
}
