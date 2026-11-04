//! The hidden `github-annotations` command. It turns a summary file into GitHub Actions workflow
//! commands, so the analytics-uploader action can say why a run failed without reading the
//! summary's format itself.

use std::{fmt, path::Path};

use pluralizer::pluralize;

use super::{
    ErrorSummary, FailedTest, Outcome, Quarantining, UploadReport,
    json::Summary,
    text::{
        BAZEL_BEP_HINT, FAILED_WITHOUT_RUNNING_TESTS, FAILURE_MESSAGE_MAX_LINES, NO_TEST_FAILURES,
        NO_TESTS_FOUND, QUARANTINE_LOOKUP_FAILED, SOME_TESTS_MAY_NOT_HAVE_RUN, UNAUTHORIZED_HINT,
        old_timestamps_message,
    },
};
use crate::{
    error_report::{CONNECTION_REFUSED_CONTEXT, ErrorKind, UNAUTHORIZED_CONTEXT},
    exit_code::ExitCode,
};

/// GitHub shows at most 10 error annotations per step, and the summary takes one of them.
const MAX_TEST_ANNOTATIONS: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    pub level: Level,
    pub file: Option<String>,
    pub title: Option<String>,
    pub message: String,
}

/// Prints the summary's annotations on stdout. The action passes them through when this exits
/// 0, and otherwise falls back to its own message, so anything else exits non-zero: a summary
/// with nothing to say, and one that can't be read.
pub fn run(summary_file: &Path) -> ExitCode {
    let summary = match Summary::read_file(summary_file) {
        Ok(summary) => summary,
        Err(error) => {
            eprintln!("{error:#}");
            return ExitCode::Failure;
        }
    };
    let annotations = annotations(&summary);
    if annotations.is_empty() {
        return ExitCode::Failure;
    }
    for annotation in annotations {
        println!("{annotation}");
    }
    ExitCode::Success
}

pub fn annotations(summary: &Summary) -> Vec<Annotation> {
    match summary {
        Summary::Upload(report) => upload_annotations(report),
        Summary::Error(error) => vec![Annotation::error(error_message(error))],
    }
}

/// Follows `text::render_upload`: an error that blocks quarantining is all there is to say,
/// and any other error is a warning before the outcome.
fn upload_annotations(report: &UploadReport) -> Vec<Annotation> {
    let mut annotations = Vec::new();
    if let Some(error) = &report.error {
        if error.kind.blocks_quarantining() {
            return vec![Annotation::error(error_message(error))];
        }
        annotations.push(Annotation::warning(error_message(error)));
    }
    if report.files_with_old_timestamps > 0 {
        annotations.push(Annotation::warning(format!(
            "{}, but the test counts include them.\n{BAZEL_BEP_HINT}",
            old_timestamps_message(report.files_with_old_timestamps)
        )));
    }

    let exit_code = report.cli_exit_code();
    match report.outcome {
        Outcome::NoTestsFound if !exit_code.is_success() => {
            annotations.push(Annotation::error(format!(
                "{NO_TESTS_FOUND}, but non zero exit code provided: {exit_code}\n{FAILED_WITHOUT_RUNNING_TESTS}"
            )));
        }
        Outcome::NoFailuresButNonZeroExitCode => {
            annotations.push(Annotation::error(format!(
                "{NO_TEST_FAILURES}, but non zero exit code provided: {exit_code}\n{SOME_TESTS_MAY_NOT_HAVE_RUN}"
            )));
        }
        Outcome::FailuresNotQuarantined => {
            let tests = report.failures.iter().flat_map(|group| &group.tests);
            annotations.push(Annotation::error(failures_message(report)));
            annotations.extend(tests.take(MAX_TEST_ANNOTATIONS).map(test_annotation));
        }
        Outcome::NoTestsFound | Outcome::NoFailures | Outcome::AllFailuresQuarantined => {}
    }
    annotations
}

fn failures_message(report: &UploadReport) -> String {
    let failed = report.counts.failed;
    let mut message = format!(
        "{} failed and {} not quarantined, using exit code: {}",
        pluralize("test", failed as isize, true),
        pluralize("was", failed as isize, false),
        report.cli_exit_code(),
    );
    let reason = match report.quarantining {
        Quarantining::LookupFailed => Some(QUARANTINE_LOOKUP_FAILED),
        Quarantining::DisabledViaCli => {
            Some("Quarantining was turned off with --disable-quarantining")
        }
        Quarantining::DisabledViaApi => Some("Quarantining is turned off for this repo in Trunk"),
        Quarantining::Checked | Quarantining::NotNeeded => None,
    };
    if let Some(reason) = reason {
        message.push_str(&format!("\n{reason}"));
    }
    if failed > MAX_TEST_ANNOTATIONS {
        message.push_str(&format!(
            "\nThe first {MAX_TEST_ANNOTATIONS} are annotated; see the log for the rest."
        ));
    }
    message
}

fn test_annotation(test: &FailedTest) -> Annotation {
    let separator = if test.suite.is_empty() { "" } else { "/" };
    let mut lines: Vec<&str> = test
        .failure_text
        .as_deref()
        .or(test.failure_message.as_deref())
        .map(|message| message.lines().take(FAILURE_MESSAGE_MAX_LINES).collect())
        .unwrap_or_default();
    if lines.is_empty() {
        lines.push("Failed");
    }
    if let Some(link) = &test.link {
        lines.push(link);
    }
    Annotation {
        level: Level::Error,
        file: test.file.clone(),
        title: Some(format!("{}{separator}{}", test.suite, test.name)),
        message: lines.join("\n"),
    }
}

/// The same lines as `text::render_error`, without its exit code line.
fn error_message(error: &ErrorSummary) -> String {
    let mut lines = Vec::new();
    match error.kind {
        ErrorKind::ConnectionRefused => {
            lines.extend(error.message.clone());
            lines.push(CONNECTION_REFUSED_CONTEXT.to_string());
        }
        ErrorKind::Unauthorized => {
            lines.extend(error.message.clone());
            lines.push(UNAUTHORIZED_CONTEXT.to_string());
            lines.extend(
                error
                    .settings_url
                    .as_ref()
                    .map(|url| format!("{UNAUTHORIZED_HINT} {url}")),
            );
        }
        ErrorKind::GitRepository | ErrorKind::Interrupted | ErrorKind::Other => {
            match &error.message {
                Some(message) => {
                    lines.push(message.clone());
                    lines.extend(error.explanation.clone());
                }
                None => lines.push(error.error.clone()),
            }
        }
    }
    lines.join("\n")
}

impl Annotation {
    fn error(message: String) -> Self {
        Self {
            level: Level::Error,
            file: None,
            title: None,
            message,
        }
    }

    fn warning(message: String) -> Self {
        Self {
            level: Level::Warning,
            ..Self::error(message)
        }
    }
}

/// A workflow command, escaped per GitHub's rules:
/// https://github.com/actions/toolkit/blob/main/packages/core/src/command.ts
impl fmt::Display for Annotation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let command = match self.level {
            Level::Error => "error",
            Level::Warning => "warning",
        };
        let properties = [("file", &self.file), ("title", &self.title)]
            .into_iter()
            .filter_map(|(key, value)| Some(format!("{key}={}", escape_property(value.as_ref()?))))
            .collect::<Vec<_>>();
        let separator = if properties.is_empty() { "" } else { " " };
        write!(
            f,
            "::{command}{separator}{}::{}",
            properties.join(","),
            escape_data(&self.message)
        )
    }
}

fn escape_data(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn escape_property(value: &str) -> String {
    escape_data(value).replace(':', "%3A").replace(',', "%2C")
}
