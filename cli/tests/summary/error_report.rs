use api::client::ClientErrorExplanation;
use pretty_assertions::assert_eq;
use superconsole::Line;
use trunk_analytics_cli::{
    error_report::{ErrorReport, InterruptingError},
    exit_code::ExitCode,
    report::{
        ErrorSummary,
        json::{self, Report},
        text,
    },
};

use crate::unauthorized_error;

fn error_summary(report: &ErrorReport) -> Vec<String> {
    text::render_error(&ErrorSummary::new(report))
        .unwrap()
        .iter()
        .map(Line::to_unstyled)
        .collect()
}

fn connection_refused_error() -> anyhow::Error {
    anyhow::Error::from(std::io::Error::from(std::io::ErrorKind::ConnectionRefused))
        .context("Failed to create bundle upload")
}

#[test]
fn prints_a_client_error_explanation_beneath_the_base_message() {
    let error = anyhow::anyhow!("HTTP status client error (400 Bad Request)")
        .context(ClientErrorExplanation {
            code: Some(String::from("SOME_CODE")),
            message: Some(String::from("Pass --test-collection-id.")),
        })
        .context("Error in create bundle upload endpoint");
    let report = ErrorReport::new(
        error,
        String::from("fake-org-slug"),
        Some(String::from("There was an unexpected error")),
    );

    assert_eq!(
        error_summary(&report),
        [
            "Error Encountered",
            "There was an unexpected error",
            "",
            "Pass --test-collection-id.",
            "",
            "For more help, contact us at https://slack.trunk.io/",
            "",
            "Errors occurred during execution, using quarantining exit code",
        ]
    );
}

#[test]
fn prints_connection_refused_beneath_the_base_message() {
    let report = ErrorReport::new(
        connection_refused_error(),
        String::from("fake-org-slug"),
        Some(String::from("There was an unexpected error")),
    );

    assert_eq!(
        error_summary(&report),
        [
            "Error Encountered",
            "There was an unexpected error",
            "",
            "Unable to connect to trunk's server",
        ]
    );
}

#[test]
fn prints_connection_refused_without_a_base_message() {
    let report = ErrorReport::new(
        connection_refused_error(),
        String::from("fake-org-slug"),
        None,
    );

    assert_eq!(
        error_summary(&report),
        ["Error Encountered", "Unable to connect to trunk's server"]
    );
}

#[test]
fn prints_unauthorized_with_a_settings_hint() {
    let report = ErrorReport::new(unauthorized_error(), String::from("fake-org-slug"), None);

    assert_eq!(
        error_summary(&report),
        [
            "Error Encountered",
            "",
            "",
            "Unathorized access, your Trunk organization URL slug or token may be incorrect",
            "  Hint: You can find it under the settings page at https://app.trunk.io/fake-org-slug/settings",
        ]
    );
}

#[test]
fn prints_a_generic_error_without_a_base_message() {
    let report = ErrorReport::new(
        anyhow::anyhow!("--allow-forked-pr-uploads requires --test-collection-id"),
        String::from("fake-org-slug"),
        None,
    );

    assert_eq!(
        error_summary(&report),
        [
            "Error Encountered",
            "--allow-forked-pr-uploads requires --test-collection-id",
            "For more help, contact us at https://slack.trunk.io/",
            "",
            "Errors occurred during execution, using quarantining exit code",
        ]
    );
}

#[test]
fn prints_an_interrupting_error_with_its_exit_code() {
    let report = ErrorReport::new(
        anyhow::anyhow!(InterruptingError::new(
            "No test output files found to validate"
        )),
        String::from("fake-org-slug"),
        None,
    );

    assert_eq!(
        error_summary(&report),
        [
            "Error Encountered",
            "No test output files found to validate",
            "For more help, contact us at https://slack.trunk.io/",
            "",
            "Errors occurred during execution, returning exit code: 70",
        ]
    );
}

#[test]
fn prints_the_default_exit_code() {
    let mut report = ErrorReport::new(
        anyhow::anyhow!("something went wrong"),
        String::from("fake-org-slug"),
        None,
    );
    report.context.exit_code = Some(ExitCode::Success);

    assert_eq!(
        error_summary(&report),
        [
            "Error Encountered",
            "something went wrong",
            "For more help, contact us at https://slack.trunk.io/",
            "",
            "No errors occurred, returning default exit code: 0",
        ]
    );
}

#[test]
fn prints_any_other_exit_code() {
    let mut report = ErrorReport::new(
        anyhow::anyhow!("something went wrong"),
        String::from("fake-org-slug"),
        None,
    );
    report.context.exit_code = Some(ExitCode::PassedThrough(64));

    assert_eq!(
        error_summary(&report),
        [
            "Error Encountered",
            "something went wrong",
            "For more help, contact us at https://slack.trunk.io/",
            "",
            "Errors occurred during execution, returning exit code: 64",
        ]
    );
}

#[test]
fn writes_an_error_as_json() {
    let report = ErrorReport::new(
        anyhow::anyhow!("--allow-forked-pr-uploads requires --test-collection-id")
            .context("Failed to gather upload context"),
        String::from("fake-org-slug"),
        None,
    );
    let error = ErrorSummary::new(&report);

    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&json::to_string(Report::Error(&error)).unwrap())
            .unwrap(),
        serde_json::json!({
            "schemaVersion": 1,
            "type": "ERROR",
            "kind": "OTHER",
            "message": null,
            "error": "Failed to gather upload context",
            "causes": ["--allow-forked-pr-uploads requires --test-collection-id"],
            "explanation": null,
            "settingsUrl": null,
            "exitCode": null,
        })
    );
}

#[test]
fn writes_the_settings_url_and_exit_code_of_an_unauthorized_error_as_json() {
    let report = ErrorReport::new(unauthorized_error(), String::from("fake-org-slug"), None);
    let error = ErrorSummary::new(&report);
    let json: serde_json::Value =
        serde_json::from_str(&json::to_string(Report::Error(&error)).unwrap()).unwrap();

    assert_eq!(json["kind"], "UNAUTHORIZED");
    assert_eq!(
        json["settingsUrl"],
        "https://app.trunk.io/fake-org-slug/settings"
    );
    assert_eq!(json["exitCode"], 70);
}
