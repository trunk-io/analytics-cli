use constants::EXIT_SUCCESS;
use display::end_output::EndOutput;
use pluralizer::pluralize;
use superconsole::{
    Hyperlink, Line, Lines, Span, SuperConsole,
    style::{Attribute, Color, Stylize, style},
};

use super::{
    ErrorSummary, FailureGroup, FileValidation, GroupKey, Outcome, TestCounts, UploadReport,
};
use crate::{
    error_report::{CONNECTION_REFUSED_CONTEXT, ErrorKind, HELP_TEXT, UNAUTHORIZED_CONTEXT},
    report_limiting::ValidationReport,
    upload_command::{UploadRunResult, get_bundle_upload_id_message},
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
                .failure_text
                .as_ref()
                .or(test.failure_message.as_ref())
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

/// Plain text unless superconsole is drawing to an interactive terminal: log viewers such as
/// GitHub Actions don't support OSC 8 and drop the link text along with the escape.
fn hyperlink_span(url: String, attribute: Attribute) -> anyhow::Result<Span> {
    url_span(url, attribute, SuperConsole::compatible())
}

fn url_span(url: String, attribute: Attribute, clickable: bool) -> anyhow::Result<Span> {
    let span = Span::new_styled(style(url.clone()).attribute(attribute))?;
    Ok(if clickable {
        span.with_hyperlink(Some(Hyperlink::new(url)))
    } else {
        span
    })
}

#[cfg(test)]
mod tests {
    use superconsole::{Line, style::Attribute};

    use super::url_span;

    const URL: &str = "https://app.trunk.io/my-org/flaky-tests/test/1";

    fn rendered(clickable: bool) -> String {
        Line::from_iter([url_span(URL.into(), Attribute::Underlined, clickable).unwrap()]).render()
    }

    #[test]
    fn url_span_adds_osc8_only_when_clickable() {
        assert!(rendered(true).contains("\x1b]8;;"));
        let plain = rendered(false);
        assert!(!plain.contains("\x1b]8;;"));
        assert!(plain.contains(URL));
    }
}
