use std::path::PathBuf;

use constants::EXIT_SUCCESS;
use display::end_output::EndOutput;
use pluralizer::pluralize;
use superconsole::{
    Hyperlink, Line, Lines, Span,
    style::{Attribute, Color, Stylize, style},
};

use super::{
    ErrorSummary, FailureGroup, FileValidation, GroupKey, NoticeKind, Outcome, ParseError,
    TestCounts, UploadReport, ValidateReport, ValidationCounts,
};
use crate::{
    error_report::{CONNECTION_REFUSED_CONTEXT, ErrorKind, HELP_TEXT, UNAUTHORIZED_CONTEXT},
    report_limiting::ValidationReport,
    upload_command::{UploadRunResult, get_bundle_upload_id_message},
    validate_command::ValidateRunResult,
};

/// Flags that change how much of the summary to show, not what it says.
#[derive(Debug, Clone)]
pub struct TextOptions {
    pub validation_report: ValidationReport,
    // TODO: show_failure_messages is a temporary flag to show failure messages
    // in the output. It should be removed once we are confident in this flow
    // and we should use the validation report flag instead.
    pub show_failure_messages: bool,
}

impl From<&UploadRunResult> for TextOptions {
    fn from(result: &UploadRunResult) -> Self {
        Self {
            validation_report: result.validation_report.clone(),
            show_failure_messages: result.show_failure_messages,
        }
    }
}

pub struct UploadText {
    pub report: UploadReport,
    pub options: TextOptions,
}

impl EndOutput for UploadText {
    fn output(&self) -> anyhow::Result<Vec<Line>> {
        render_upload(&self.report, &self.options)
    }
}

impl EndOutput for ErrorSummary {
    fn output(&self) -> anyhow::Result<Vec<Line>> {
        render_error(self)
    }
}

pub struct SummaryFileText {
    pub path: PathBuf,
    pub error: Option<String>,
}

impl EndOutput for SummaryFileText {
    fn output(&self) -> anyhow::Result<Vec<Line>> {
        let path = self.path.display();
        let message = match &self.error {
            None => format!("Summary written to {path}"),
            Some(error) => format!("⚠️  Failed to write the summary to {path}: {error}"),
        };
        Ok(vec![
            Line::default(),
            Line::from_iter([Span::new_unstyled_lossy(message)]),
        ])
    }
}

pub fn render_error(error: &ErrorSummary) -> anyhow::Result<Vec<Line>> {
    let mut lines = Vec::new();
    lines.push(Line::from_iter([Span::new_styled(
        String::from("Error Encountered").attribute(Attribute::Bold),
    )?]));
    match error.kind {
        ErrorKind::ConnectionRefused => {
            if let Some(message) = &error.message {
                lines.push(Line::from_iter([Span::new_unstyled_lossy(message)]));
                lines.push(Line::default());
            }
            lines.push(Line::from_iter([Span::new_unstyled(
                CONNECTION_REFUSED_CONTEXT,
            )?]));
            return Ok(lines);
        }
        ErrorKind::Unauthorized => {
            let mut hint = Line::from_iter([Span::new_unstyled_lossy(format!(
                "Hint: You can find it under the settings page at {}",
                error.settings_url.as_deref().unwrap_or_default()
            ))]);
            hint.pad_left(2);
            lines.extend([
                Line::from_iter([Span::new_unstyled_lossy(
                    error.message.as_deref().unwrap_or_default(),
                )]),
                Line::default(),
                Line::from_iter([Span::new_unstyled(UNAUTHORIZED_CONTEXT)?]),
                hint,
            ]);
            return Ok(lines);
        }
        ErrorKind::GitRepository | ErrorKind::Interrupted | ErrorKind::Other => {}
    }
    if let Some(message) = &error.message {
        lines.push(Line::from_iter([Span::new_unstyled_lossy(message)]));
        lines.push(Line::default());
        if let Some(explanation) = &error.explanation {
            lines.push(Line::from_iter([Span::new_unstyled_lossy(explanation)]));
            lines.push(Line::default());
        }
    } else {
        lines.push(Line::from_iter([Span::new_unstyled_lossy(&error.error)]));
    }
    lines.push(Line::from_iter([Span::new_unstyled(HELP_TEXT)?]));
    lines.push(Line::default());
    match error.exit_code {
        Some(exitcode::OK) => lines.push(Line::from_iter([
            Span::new_unstyled("No errors occurred, returning default exit code: ")?,
            Span::new_styled(style(exitcode::OK.to_string()).attribute(Attribute::Bold))?,
        ])),
        Some(exit_code) => lines.push(Line::from_iter([
            Span::new_unstyled("Errors occurred during execution, returning exit code: ")?,
            Span::new_styled(style(exit_code.to_string()).attribute(Attribute::Bold))?,
        ])),
        // If uploads fail because trunk is down, we fall back to whatever came out of quarantining
        // to minimize customer impact
        None => lines.push(Line::from_iter([Span::new_unstyled(
            "Errors occurred during execution, using quarantining exit code",
        )?])),
    }
    Ok(lines)
}

pub fn render_upload(report: &UploadReport, options: &TextOptions) -> anyhow::Result<Vec<Line>> {
    let mut output: Vec<Line> = Vec::new();
    if let Some(error) = &report.error {
        output.push(Line::default());
        output.extend(render_error(error)?);
        if error.kind.blocks_quarantining() {
            return Ok(output);
        }
        output.push(Line::from_iter([Span::new_unstyled(
            "Proceeding with test report...",
        )?]));
        output.push(Line::default());
    }

    let bold = |text: String| Span::new_styled(style(text).attribute(Attribute::Bold));
    let upload_line = match (&report.upload_url, &report.bundle_upload_id) {
        (Some(url), _) => Some(Line::from_iter([
            bold(String::from("🏷️  Upload: "))?,
            hyperlink_span(url.clone(), Attribute::Bold)?,
        ])),
        (None, Some(bundle_upload_id)) => Some(Line::from_iter([bold(
            get_bundle_upload_id_message(bundle_upload_id),
        )?])),
        (None, None) => None,
    };
    if let Some(line) = upload_line {
        output.push(line);
        output.push(Line::default());
    }

    if !report.validations.is_empty() {
        output.extend(render_validations(
            &report.validations,
            &options.validation_report,
        )?);
        output.push(Line::default());
    }

    let TestCounts {
        total,
        passed,
        failed,
        quarantined,
        pass_ratio,
    } = report.counts;
    let pass_ratio = pass_ratio.map_or_else(
        || String::from("N/A"),
        |pass_ratio| format!("{:.1}%", pass_ratio * 100.0),
    );
    output.push(Line::from_iter([Span::new_styled(
        style("📚 Test Report".to_string()).attribute(Attribute::Bold),
    )?]));
    output.push(Line::from_iter([Span::new_unstyled(format!(
        "   Total: {}   Pass: {}   Fail: {}   Quarantined: {}   Pass Ratio: {}",
        total, passed, failed, quarantined, pass_ratio
    ))?]));
    if report.quarantine_lookup_failed {
        output.push(Line::from_iter([Span::new_styled(
            style("   We were unable to determine the quarantine status for tests. Any failing tests will be reported as failures".to_string()).with(Color::Yellow).attribute(Attribute::Bold),
        )?]));
    }
    output.push(Line::default());

    if quarantined > 0 {
        output.extend(vec![
            Line::from_iter([
                Span::new_unstyled("❤️‍🩹  ")?,
                Span::new_styled(
                    style(format!(
                        "{} ",
                        pluralize("test", quarantined as isize, true)
                    ))
                    .attribute(Attribute::Bold),
                )?,
                Span::new_styled(style(format!(
                    "failed and {} ",
                    pluralize("was", quarantined as isize, false)
                )))?,
                Span::new_styled(
                    style(String::from("quarantined"))
                        .with(Color::Yellow)
                        .attribute(Attribute::Bold),
                )?,
            ]),
            Line::default(),
        ]);
        output.extend(render_groups(&report.quarantined, options)?);
    }

    if failed > 0 {
        output.extend(vec![
            Line::from_iter([
                Span::new_unstyled("❌ ")?,
                Span::new_unstyled(format!("{} ", pluralize("test", failed as isize, true)))?,
                Span::new_styled(
                    style(String::from("failed "))
                        .with(Color::Red)
                        .attribute(Attribute::Bold),
                )?,
                Span::new_styled(style(format!(
                    "and {} ",
                    pluralize("was", failed as isize, false)
                )))?,
                Span::new_styled(style(String::from("not ")).attribute(Attribute::Bold))?,
                Span::new_unstyled("quarantined")?,
            ]),
            Line::default(),
        ]);
        output.extend(render_groups(&report.failures, options)?);
    }

    match report.outcome {
        Outcome::NoTestsFound => output.push(Line::from_iter([Span::new_unstyled(
            "⚠️  No tests were found in the provided test results",
        )?])),
        Outcome::AllFailuresQuarantined => output.push(Line::from_iter([
            Span::new_unstyled(
                "🎉 All test failures were quarantined, overriding exit code to be exit_success ",
            )?,
            Span::new_styled(style(format!("({})", EXIT_SUCCESS)).attribute(Attribute::Bold))?,
        ])),
        Outcome::NoFailures => output.push(Line::from_iter([Span::new_unstyled(
            "🎉 No test failures found!",
        )?])),
        Outcome::NoFailuresButNonZeroExitCode => {
            output.push(Line::from_iter([
                Span::new_unstyled(
                    "⚠️  No test failures found, but non zero exit code provided: ",
                )?,
                Span::new_styled(
                    style(format!("{}", report.exit_code)).attribute(Attribute::Bold),
                )?,
            ]));
            output.push(Line::from_iter([Span::new_unstyled(
                "This may indicate that some tests were not run or that there were other issues during the test run.",
            )?]));
        }
        Outcome::FailuresNotQuarantined => output.push(Line::from_iter([
            Span::new_unstyled(
                "⚠️  Some test failures were not quarantined, using exit code: ".to_string(),
            )?,
            Span::new_styled(style(format!("{}", report.exit_code)).attribute(Attribute::Bold))?,
        ])),
    }
    Ok(output)
}

fn render_groups(groups: &[FailureGroup], options: &TextOptions) -> anyhow::Result<Lines> {
    let mut output = Vec::new();
    for group in groups {
        let (header, color) = match &group.key {
            GroupKey::File(file) => (format!("📁 {}", file), Color::Cyan),
            GroupKey::Suite(suite) => (format!("📦 {}", suite), Color::Magenta),
            GroupKey::Other => (String::from("🔍 Other"), Color::Yellow),
        };
        output.push(Line::from_iter([Span::new_styled(
            style(header).with(color).attribute(Attribute::Bold),
        )?]));
        for test in group.tests.iter().take(3) {
            let output_name = format!(
                "{}{}{}",
                test.suite,
                if test.suite.is_empty() { "" } else { "/" },
                test.name
            );
            let mut test_line = Line::from_iter([Span::new_styled_lossy(
                style(output_name).attribute(Attribute::Bold),
            )]);
            test_line.pad_left(2);
            output.push(test_line);
            // A test without a link has always failed the whole summary.
            let link = test
                .link
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Unable to link to test {}", test.id))?;
            let mut link_output = Line::from_iter([
                Span::new_unstyled("⤷ ")?,
                hyperlink_span(link, Attribute::Underlined)?,
            ]);
            link_output.pad_left(4);
            output.push(link_output);
            if let Some(failure_message) = test
                .failure_message
                .as_ref()
                .filter(|_| options.show_failure_messages)
            {
                output.extend(render_failure_message(failure_message)?);
            }
        }
        if group.tests.len() > 3 {
            let mut more_failures = Line::from_iter([Span::new_unstyled(format!(
                "…and {} more failures in this group",
                group.tests.len() - 3
            ))?]);
            more_failures.pad_left(2);
            output.push(more_failures);
        }
        output.push(Line::default());
    }
    let mut output = Lines(output);
    output.pad_lines_left(2);
    output.pad_lines_bottom(1);
    Ok(output)
}

fn render_failure_message(failure_message: &str) -> anyhow::Result<Vec<Line>> {
    let mut output = Vec::new();
    let lines: Vec<&str> = failure_message.split('\n').collect();
    let max_lines = 20;
    let mut failure_header = Line::from_iter([Span::new_styled(
        style("Failure: ".to_string()).with(Color::DarkGrey),
    )?]);
    failure_header.pad_left(4);
    output.push(failure_header);
    for (j, line) in lines.iter().take(max_lines).enumerate() {
        let sanitized_line = line.replace('\t', "    ").replace('\r', "");
        if !sanitized_line.trim().is_empty() || j == 0 {
            let mut failure_output = Line::from_iter([
                Span::new_unstyled("   ")?,
                Span::new_styled_lossy(
                    style(sanitized_line)
                        .with(Color::Grey)
                        .attribute(Attribute::Italic),
                ),
            ]);
            failure_output.pad_left(4);
            output.push(failure_output);
        }
    }
    if lines.len() > max_lines {
        let omitted = lines.len() - max_lines;
        let mut more_output = Line::from_iter([
            Span::new_unstyled("   ")?,
            Span::new_styled(
                style(format!("…and {omitted} more lines not shown"))
                    .with(Color::Grey)
                    .attribute(Attribute::Italic),
            )?,
        ]);
        more_output.pad_left(4);
        output.push(more_output);
    }
    Ok(output)
}

fn render_validations(
    validations: &[FileValidation],
    limits: &ValidationReport,
) -> anyhow::Result<Vec<Line>> {
    let mut output: Vec<Line> = Vec::new();
    if limits == &ValidationReport::None {
        return Ok(output);
    }

    output.push(Line::from_iter([Span::new_styled(
        String::from("📂 File Validation").attribute(Attribute::Bold),
    )?]));
    output.push(Line::default());

    let with_issues = validations
        .iter()
        .filter(|validation| validation.has_issues())
        .count();
    let files = pluralize("file", validations.len() as isize, true);
    let summary = if with_issues > 0 {
        format!("❕ {files} found, {with_issues} with issues")
    } else {
        format!("✅ {files} found, all fully correct")
    };
    output.push(Line::from_iter([Span::new_styled(
        summary.attribute(Attribute::Bold),
    )?]));
    output.push(Line::default());

    for validation in limits.limit_files(validations.iter()) {
        // Files that couldn't be parsed have never been listed; listing them is a visible change.
        if validation.parse_error.is_some() {
            continue;
        }
        let file_name = &validation.file;
        let mut lines: Vec<Line> = vec![];
        match (validation.warnings.is_empty(), validation.errors.is_empty()) {
            (false, false) => {
                lines.push(Line::from_iter([
                    Span::new_unstyled("❌ ")?,
                    Span::new_styled_lossy(
                        format!("{file_name} Has Errors And Warnings").attribute(Attribute::Bold),
                    ),
                ]));
                lines.push(Line::from_iter([
                    Span::new_unstyled(" ↪ ❌ ")?,
                    Span::new_styled(String::from("Errors").attribute(Attribute::Bold))?,
                ]));
                for error in limits.limit_issues(validation.errors.iter()) {
                    lines.push(Line::from_iter([
                        Span::new_unstyled("   ↪ ")?,
                        Span::new_unstyled_lossy(error),
                    ]));
                }
                lines.push(Line::from_iter([
                    Span::new_unstyled(" ↪ ⚠️  ")?,
                    Span::new_styled(String::from("Warnings").attribute(Attribute::Bold))?,
                ]));
                for warning in limits.limit_issues(validation.warnings.iter()) {
                    lines.push(Line::from_iter([
                        Span::new_unstyled("   ↪ ")?,
                        Span::new_unstyled_lossy(warning),
                    ]));
                }
            }
            (true, false) => {
                lines.push(Line::from_iter([
                    Span::new_unstyled("❌ ")?,
                    Span::new_styled_lossy(
                        format!("{file_name} Has Errors").attribute(Attribute::Bold),
                    ),
                ]));
                for error in limits.limit_issues(validation.errors.iter()) {
                    lines.push(Line::from_iter([
                        Span::new_unstyled(" ↪ ")?,
                        Span::new_unstyled_lossy(error),
                    ]));
                }
            }
            (false, true) => {
                lines.push(Line::from_iter([
                    Span::new_unstyled("⚠️  ")?,
                    Span::new_styled_lossy(
                        format!("{file_name} Has Warnings").attribute(Attribute::Bold),
                    ),
                ]));
                for warning in limits.limit_issues(validation.warnings.iter()) {
                    lines.push(Line::from_iter([
                        Span::new_unstyled(" ↪ ")?,
                        Span::new_unstyled_lossy(warning),
                    ]));
                }
            }
            (true, true) => {}
        }
        let mut lines = Lines::from_iter(lines);
        lines.pad_lines_left(2);
        output.extend(lines);
    }
    if let Some(extra_files) = limits.num_exceeding_files_limit(validations.len()) {
        output.push(Line::from_iter([Span::new_unstyled(format!(
            "…and {} more",
            extra_files
        ))?]));
    }

    Ok(output)
}

fn hyperlink_span(url: String, attribute: Attribute) -> anyhow::Result<Span> {
    let span = Span::new_styled(style(url.clone()).attribute(attribute))?;
    Ok(span.with_hyperlink(Some(Hyperlink::new(url))))
}

impl EndOutput for ValidateReport {
    fn output(&self) -> anyhow::Result<Vec<Line>> {
        render_validate(self)
    }
}

impl EndOutput for ValidateRunResult {
    fn output(&self) -> anyhow::Result<Vec<Line>> {
        render_validate(&ValidateReport::new(self))
    }
}

pub fn render_validate(report: &ValidateReport) -> anyhow::Result<Vec<Line>> {
    let mut output: Vec<Line> = Vec::new();

    let bep_parse_errors: Vec<&String> = report
        .notices
        .iter()
        .filter(|notice| notice.kind == NoticeKind::BepParseError)
        .map(|notice| &notice.message)
        .collect();
    if !bep_parse_errors.is_empty() {
        output.push(Line::from_iter([
            Span::new_unstyled("⚠️  BEP file had parse errors: ")?,
            Span::new_unstyled_lossy(format!("{:?}", bep_parse_errors)),
        ]));
        output.push(Line::default());
    }

    let num_files: usize = report
        .matched_files
        .iter()
        .map(|matched| matched.files.len())
        .sum();
    output.push(Line::from_iter([Span::new_unstyled(format!(
        "📚 Validating {} files:",
        num_files
    ))?]));
    for matched in report.matched_files.iter() {
        output.push(Line::from_iter([Span::new_unstyled(format!(
            "  Files matching: {}:",
            matched.glob
        ))?]));
        for file in matched.files.iter() {
            output.push(Line::from_iter([Span::new_unstyled(format!(
                "    {}",
                file
            ))?]))
        }
        output.push(Line::default());
    }

    // Files that weren't read as exactly one report have no test counts.
    output.push(Line::from_iter([Span::new_styled(
        style(String::from("File parse issues")).attribute(Attribute::Bold),
    )?]));
    for validation in report
        .validations
        .iter()
        .filter(|validation| validation.test_case_count.is_none())
    {
        let num_errors = validation.errors.len() + usize::from(validation.parse_error.is_some());
        output.push(file_title(validation, num_errors)?);
        output.push(Line::from_iter([Span::new_styled(
            style(format!(
                "{} errors, {} warnings",
                num_errors,
                validation.warnings.len()
            ))
            .attribute(Attribute::Italic),
        )?]));
        let parse_error = validation.parse_error.as_ref().map(anyhow_debug);
        output.extend(render_issues(
            parse_error.iter().chain(validation.errors.iter()),
            &validation.warnings,
        )?);
        output.push(Line::default());
    }

    output.push(Line::from_iter([Span::new_styled(
        style(String::from("Test Validation Issues")).attribute(Attribute::Bold),
    )?]));
    for validation in report.validations.iter() {
        let (Some(num_test_suites), Some(num_test_cases)) =
            (validation.test_suite_count, validation.test_case_count)
        else {
            continue;
        };
        output.push(file_title(validation, validation.errors.len())?);
        output.push(Line::from_iter([Span::new_styled(
            style(format!(
                "{} test suites, {} test cases, {} errors, {} warnings",
                num_test_suites,
                num_test_cases,
                validation.errors.len(),
                validation.warnings.len(),
            ))
            .attribute(Attribute::Italic),
        )?]));
        output.extend(render_issues(
            validation.errors.iter(),
            &validation.warnings,
        )?);
    }

    output.push(Line::default());
    output.push(Line::from_iter([Span::new_styled(
        style(String::from("Checking for codeowners file...")).attribute(Attribute::Bold),
    )?]));
    match &report.codeowners {
        None => {
            output.push(Line::from_iter([Span::new_unstyled(
                "  No codeowners file found",
            )?]));
        }
        Some(codeowners) => {
            output.push(Line::from_iter([Span::new_styled_lossy(
                style(format!("  Found codeowners path: {:?}", codeowners.path))
                    .attribute(Attribute::Italic),
            )]));
            for warning in codeowners.warnings.iter() {
                output.push(Line::from_iter([Span::new_unstyled_lossy(format!(
                    "  {}",
                    warning
                ))]));
            }
        }
    }
    output.push(Line::default());

    let ValidationCounts {
        files,
        valid_files,
        files_with_warnings,
        files_with_errors,
        warnings,
        errors,
    } = report.counts;
    output.push(Line::from_iter([
        Span::new_styled(style(format!("{}", valid_files)).with(Color::Green))?,
        Span::new_unstyled(format!(
            " {}, ",
            pluralize("valid file", valid_files as isize, false)
        ))?,
        Span::new_styled(style(format!("{}", files_with_warnings)).with(Color::Yellow))?,
        Span::new_unstyled(format!(
            " {} with warnings, and ",
            pluralize("file", files_with_warnings as isize, false)
        ))?,
        Span::new_styled(style(format!("{}", files_with_errors)).with(Color::Red))?,
        Span::new_unstyled(format!(
            " {} with errors, ",
            pluralize("file", files_with_errors as isize, false)
        ))?,
        Span::new_unstyled(format!(
            "with {} total (a file is double counted if it has both errors and warnings)",
            pluralize("file", files as isize, true)
        ))?,
    ]));
    output.push(Line::from_iter([
        Span::new_styled(style(format!("{}", warnings)).with(Color::Yellow))?,
        Span::new_unstyled(format!(
            " {}, and ",
            pluralize("warning", warnings as isize, false)
        ))?,
        Span::new_styled(style(format!("{}", errors)).with(Color::Red))?,
        Span::new_unstyled(format!(" {}", pluralize("error", errors as isize, false)))?,
    ]));

    Ok(output)
}

fn file_title(validation: &FileValidation, num_errors: usize) -> anyhow::Result<Line> {
    let color = if num_errors > 0 {
        Color::Red
    } else if !validation.warnings.is_empty() {
        Color::Yellow
    } else {
        Color::Green
    };
    Ok(Line::from_iter([Span::new_styled_lossy(
        style(validation.file.clone()).with(color),
    )]))
}

fn render_issues<'a>(
    errors: impl Iterator<Item = &'a String>,
    warnings: &[String],
) -> anyhow::Result<Vec<Line>> {
    let mut output = Vec::new();
    let mut errors = errors.peekable();
    if errors.peek().is_some() {
        output.push(Line::from_iter([Span::new_unstyled(" ❌ Errors:")?]));
        for error in errors {
            output.push(Line::from_iter([Span::new_unstyled_lossy(format!(
                "  {}",
                error
            ))]));
        }
    }
    if !warnings.is_empty() {
        output.push(Line::from_iter([Span::new_unstyled(" ⚠️  Warnings:")?]));
        for warning in warnings {
            output.push(Line::from_iter([Span::new_unstyled_lossy(format!(
                "  {}",
                warning
            ))]));
        }
    }
    Ok(output)
}

/// The error as `{:?}` on an `anyhow::Error` prints it, which `validate` has always shown:
/// the error, then its causes, indented (and numbered when there's more than one).
fn anyhow_debug(parse_error: &ParseError) -> String {
    let mut text = parse_error.error.clone();
    if parse_error.causes.is_empty() {
        return text;
    }
    text.push_str("\n\nCaused by:");
    let numbered = parse_error.causes.len() > 1;
    for (n, cause) in parse_error.causes.iter().enumerate() {
        text.push('\n');
        let (first, rest) = if numbered {
            (format!("{n: >5}: "), "       ")
        } else {
            (String::from("    "), "    ")
        };
        text.push_str(&first);
        text.push_str(&cause.replace('\n', &format!("\n{rest}")));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anyhow_debug_matches_anyhow() {
        let errors = [
            anyhow::anyhow!("no causes"),
            anyhow::anyhow!("cause").context("error"),
            anyhow::anyhow!("root\nspans lines")
                .context("middle")
                .context("error\nspans lines"),
        ];
        for error in errors {
            assert_eq!(anyhow_debug(&ParseError::new(&error)), format!("{error:?}"));
        }
    }
}
