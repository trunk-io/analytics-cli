use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use api::client::ApiClient;
use api::{client::get_api_host, urls::url_for_test_case};
use bundle::{unzip_tarball, BundleMeta, BundlerUtil, Test};
use clap::{ArgAction, Args};
use constants::EXIT_SUCCESS;
use context::bazel_bep::common::BepParseResult;
use display::{end_output::EndOutput, message::DisplayMessage};
use pluralizer::pluralize;
use superconsole::{
    style::{style, Attribute, Color, Stylize},
    Line, Lines, Span,
};
use tempfile::TempDir;

use crate::context_quarantine::QuarantineContext;
use crate::validate_command::JunitReportValidations;
use crate::{
    context::{
        gather_debug_props, gather_exit_code_and_quarantined_tests_context,
        gather_initial_test_context, gather_post_test_context, gather_upload_id_context,
        generate_internal_file, generate_internal_file_from_bep, PreTestContext,
    },
    error_report::ErrorReport,
    report_limiting::ValidationReport,
    test_command::TestRunResult,
};

#[cfg(target_os = "macos")]
const JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG: &str = "xcresult_path";
#[cfg(not(target_os = "macos"))]
const JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG: &str = "junit_paths";
pub const DRY_RUN_OUTPUT_DIR: &str = "bundle_upload";

#[derive(Args, Clone, Debug, Default)]
pub struct UploadArgs {
    #[arg(
        long,
        required_unless_present_any = [JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG, "bazel_bep_path", "test_reports"],
        conflicts_with = "bazel_bep_path",
        value_delimiter = ',',
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        help = "Comma-separated list of glob paths to junit files."
    )]
    pub junit_paths: Vec<String>,
    #[arg(
        long,
        required_unless_present_any = [JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG, "junit_paths", "test_reports"],
        help = "Path to bazel build event protocol JSON file."
    )]
    pub bazel_bep_path: Option<String>,
    #[cfg(target_os = "macos")]
    #[arg(long,
        required_unless_present_any = ["junit_paths", "bazel_bep_path", "test_reports"],
        conflicts_with_all = ["junit_paths", "bazel_bep_path"],
        required = false, help = "Path of xcresult directory"
    )]
    pub xcresult_path: Option<String>,
    #[arg(
        long,
        required_unless_present_any = [JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG, "junit_paths", "bazel_bep_path"],
        value_delimiter = ',',
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        help = "Comma-separated list of glob paths to test report files."
    )]
    pub test_reports: Vec<String>,
    #[arg(long, help = "Organization url slug.")]
    pub org_url_slug: String,
    #[arg(
        long,
        required = true,
        env = "TRUNK_API_TOKEN",
        help = "Organization token. Defaults to TRUNK_API_TOKEN env var."
    )]
    pub token: String,
    #[arg(long, help = "Path to repository root. Defaults to current directory.")]
    pub repo_root: Option<String>,
    #[arg(long, help = "Value to override URL of repository.")]
    pub repo_url: Option<String>,
    #[arg(long, help = "Value to override SHA of repository head.")]
    pub repo_head_sha: Option<String>,
    #[arg(long, help = "Value to override branch of repository head.")]
    pub repo_head_branch: Option<String>,
    #[arg(long, help = "Value to override commit epoch of repository head.")]
    pub repo_head_commit_epoch: Option<String>,
    #[arg(long, help = "Value to tag team owner of upload.", hide = true)]
    pub team: Option<String>,
    #[arg(
        long,
        help = "Print the files that would be uploaded to the server.",
        hide = true,
        required = false
    )]
    pub print_files: Option<bool>,
    #[arg(long, help = "Value to override CODEOWNERS file or directory path.")]
    pub codeowners_path: Option<String>,
    #[arg(
        long,
        help = "Run commands with the quarantining step. Deprecated, prefer disable-quarantining, which takes priority over this flag, to control quarantining.",
        action = ArgAction::Set,
        required = false,
        require_equals = true,
        num_args = 0..=1,
        default_value = "true",
        default_missing_value = "true",
        hide = true
    )]
    pub use_quarantining: bool,
    #[arg(
        long,
        help = "Does not apply quarantining if set to true",
        action = ArgAction::Set,
        required = false,
        require_equals = true,
        num_args = 0..=1,
        default_value = "false",
        default_missing_value = "true",
    )]
    pub disable_quarantining: bool,
    #[arg(
        long,
        alias = "allow-missing-junit-files",
        help = "Do not fail if test results are not found.",
        action = ArgAction::Set,
        required = false,
        require_equals = true,
        num_args = 0..=1,
        default_value = "true",
        default_missing_value = "true",
    )]
    pub allow_empty_test_results: bool,
    #[arg(
        long,
        help = "Deprecated (does nothing, left in to avoid breaking existing flows)",
        action = ArgAction::Set,
        required = false,
        require_equals = true,
        num_args = 0..=1,
        default_value = "false",
        default_missing_value = "true",
        hide = true,
    )]
    pub hide_banner: bool,
    #[arg(
        long,
        help = "Write the bundle locally to a file instead of uploading it.",
        required = false,
        num_args = 0,
        default_value = "false",
        default_missing_value = "true",
        hide = true
    )]
    pub dry_run: bool,
    #[arg(
        long,
        help = "Value to set the variant of the test results uploaded.",
        required = false,
        num_args = 1
    )]
    pub variant: Option<String>,
    #[arg(
        long,
        help = "The exit code to use when not all tests are quarantined.",
        required = false,
        num_args = 1
    )]
    pub test_process_exit_code: Option<i32>,
    #[arg(
        long,
        help = "Value to set the name of the author of the commit being tested.",
        required = false,
        num_args = 1,
        hide = true
    )]
    pub repo_head_author_name: Option<String>,
    #[arg(
        long,
        help = "Set when you want to upload to a repository which is not available in your filesystem.",
        required = false,
        require_equals = false,
        num_args = 0,
        default_value = "false",
        default_missing_value = "true",
        requires = "repo_url",
        requires = "repo_head_sha",
        requires = "repo_head_branch",
        requires = "repo_head_author_name",
        conflicts_with = "repo_root",
        hide = true
    )]
    pub use_uncloned_repo: bool,
    #[cfg(target_os = "macos")]
    #[arg(
        long,
        help = "Flag to enable populating file paths from xcresult stack traces",
        required = false,
        num_args = 0,
        hide = true
    )]
    pub use_experimental_failure_summary: bool,
    #[arg(
        long,
        help = "Change how many validation errors and warnings in your test reports we will show.",
        required = false,
        num_args = 1,
        default_value = "limited",
        default_missing_value = "limited"
    )]
    pub validation_report: ValidationReport,
    #[arg(
        long,
        help = "Show failure messages in the output.",
        required = false,
        num_args = 0,
        default_value = "false",
        default_missing_value = "true"
    )]
    pub show_failure_messages: bool,
}

impl UploadArgs {
    pub fn new(
        token: String,
        org_url_slug: String,
        junit_paths: Vec<String>,
        repo_root: Option<String>,
        use_quarantining: bool,
        disable_quarantining: bool,
    ) -> Self {
        Self {
            junit_paths,
            org_url_slug,
            token,
            repo_root,
            allow_empty_test_results: true,
            use_quarantining,
            disable_quarantining,
            show_failure_messages: false,
            ..Default::default()
        }
    }
}

fn error_reason(error: &anyhow::Error) -> String {
    let root_cause = error.root_cause();
    if let Some(io_error) = root_cause.downcast_ref::<std::io::Error>() {
        if io_error.kind() == std::io::ErrorKind::ConnectionRefused {
            return "connection".to_string();
        }
    }

    if let Some(reqwest_error) = root_cause.downcast_ref::<reqwest::Error>() {
        if let Some(status) = reqwest_error.status() {
            return status.to_string().replace(' ', "_").to_lowercase();
        }
    }
    "unknown".into()
}

pub struct UploadRunResult {
    pub error_report: Option<ErrorReport>,
    pub quarantine_context: QuarantineContext,
    pub meta: BundleMeta,
    pub validations: JunitReportValidations,
    pub validation_report: ValidationReport,
    pub show_failure_messages: bool,
}

pub async fn run_upload(
    upload_args: UploadArgs,
    pre_test_context: Option<PreTestContext>,
    test_run_result: Option<TestRunResult>,
    render_sender: Option<Sender<DisplayMessage>>,
) -> anyhow::Result<UploadRunResult> {
    // grab the exec start if provided (`test` subcommand) or use the current time
    let cli_started_at = if let Some(test_run_result) = test_run_result.as_ref() {
        test_run_result
            .exec_start
            .unwrap_or(chrono::Utc::now().into())
    } else {
        chrono::Utc::now().into()
    };

    if upload_args.print_files.is_some() {
        tracing::error!(
            "The --print-files flag is deprecated and will be removed in a future version."
        );
    }

    if let Some(team) = &upload_args.team {
        if !team.is_empty() {
            tracing::error!(
                "The --team flag is deprecated and will be removed in a future version."
            );
        }
    }

    let api_client = ApiClient::new(&upload_args.token, &upload_args.org_url_slug, render_sender)?;

    let PreTestContext {
        mut meta,
        junit_path_wrappers,
        bep_result,
        // directory is removed on drop
        junit_path_wrappers_temp_dir: _junit_path_wrappers_temp_dir,
    } = if let Some(pre_test_context) = pre_test_context {
        pre_test_context
    } else {
        gather_initial_test_context(
            upload_args.clone(),
            gather_debug_props(env::args().collect::<Vec<String>>(), upload_args.token),
        )?
    };

    let file_set_builder = gather_post_test_context(
        &mut meta,
        junit_path_wrappers,
        &upload_args.codeowners_path,
        upload_args.allow_empty_test_results,
        &test_run_result,
    )?;
    let temp_dir = tempfile::tempdir()?;
    let internal_bundled_file = if let Some(ref bep_result) = bep_result {
        generate_internal_file_from_bep(
            bep_result,
            &temp_dir,
            meta.base_props.codeowners.as_ref(),
            // hide warnings on parsed xcresult output
            #[cfg(target_os = "macos")]
            upload_args.xcresult_path.is_none(),
            #[cfg(not(target_os = "macos"))]
            true,
            upload_args.variant,
        )
    } else {
        generate_internal_file(
            &meta.base_props.file_sets,
            &temp_dir,
            meta.base_props.codeowners.as_ref(),
            // hide warnings on parsed xcresult output
            #[cfg(target_os = "macos")]
            upload_args.xcresult_path.is_none(),
            #[cfg(not(target_os = "macos"))]
            true,
            upload_args.variant,
        )
    };
    let validations = if let Ok((internal_bundled_file, junit_validations)) = internal_bundled_file
    {
        meta.internal_bundled_file = Some(internal_bundled_file);
        JunitReportValidations::new(junit_validations)
    } else {
        JunitReportValidations::new(BTreeMap::new())
    };

    let default_exit_code = if let Some(exit_code) = upload_args.test_process_exit_code {
        Some(exit_code)
    } else {
        test_run_result.as_ref().map(|r| r.exit_code)
    };
    let quarantine_context = match gather_exit_code_and_quarantined_tests_context(
        &mut meta,
        upload_args.disable_quarantining || !upload_args.use_quarantining,
        &api_client,
        &file_set_builder,
        default_exit_code,
    )
    .await
    {
        Ok(context) => context,
        Err(e) => {
            tracing::error!("Failed to gather quarantine context: {}", e);
            QuarantineContext::fail_fetch(e)
        }
    };
    meta.base_props.quarantined_tests = quarantine_context
        .quarantine_status
        .quarantine_results
        .clone();

    let upload_started_at = chrono::Utc::now();
    tracing::info!("Uploading test results...");
    let upload_bundle_result = upload_bundle(
        meta.clone(),
        &api_client,
        bep_result,
        quarantine_context.exit_code,
        upload_args.dry_run,
    )
    .await;
    let upload_metrics = proto::upload_metrics::trunk::UploadMetrics {
        client_version: Some(proto::upload_metrics::trunk::Semver {
            major: env!("CARGO_PKG_VERSION_MAJOR").parse().unwrap_or_default(),
            minor: env!("CARGO_PKG_VERSION_MINOR").parse().unwrap_or_default(),
            patch: env!("CARGO_PKG_VERSION_PATCH").parse().unwrap_or_default(),
            suffix: env!("CARGO_PKG_VERSION_PRE").into(),
        }),
        repo: Some(proto::upload_metrics::trunk::Repo {
            host: meta.base_props.repo.repo.host.clone(),
            owner: meta.base_props.repo.repo.owner.clone(),
            name: meta.base_props.repo.repo.name.clone(),
        }),
        cli_started_at: Some(cli_started_at.into()),
        upload_started_at: Some(upload_started_at.into()),
        upload_finished_at: Some(chrono::Utc::now().into()),
        failed: false,
        failure_reason: "".into(),
    };
    let mut request = api::message::TelemetryUploadMetricsRequest { upload_metrics };
    if !upload_args.dry_run {
        let telemetry_response;
        if let Some(err) = upload_bundle_result.as_ref().err() {
            request.upload_metrics.failed = true;
            request.upload_metrics.failure_reason = error_reason(err);
            telemetry_response = api_client.telemetry_upload_metrics(&request).await;
        } else {
            request.upload_metrics.failed = false;
            telemetry_response = api_client.telemetry_upload_metrics(&request).await;
        }
        if let Err(e) = telemetry_response {
            tracing::error!(
                hidden_in_console = true,
                "Failed to send telemetry: {:?}",
                e
            );
        }
    }

    if upload_bundle_result.is_err() {
        tracing::error!("Failed to upload bundle");
    }
    let error_report = match upload_bundle_result {
        Ok(upload_bundle_result) => {
            if upload_args.dry_run {
                let curr_dir = env::current_dir()?;
                let bundle_file = curr_dir.join(DRY_RUN_OUTPUT_DIR);
                unzip_tarball(&upload_bundle_result.0, &bundle_file)?;
            }
            None
        }
        Err(e) => Some(ErrorReport::new(
            e,
            upload_args.org_url_slug.clone(),
            Some("There was an unexpected error that occurred while uploading test results".into()),
        )),
    };
    Ok(UploadRunResult {
        quarantine_context,
        error_report,
        meta,
        validations,
        validation_report: upload_args.validation_report,
        show_failure_messages: upload_args.show_failure_messages,
    })
}

async fn upload_bundle(
    mut meta: BundleMeta,
    api_client: &ApiClient,
    bep_result: Option<BepParseResult>,
    exit_code: i32,
    dry_run: bool,
) -> anyhow::Result<(PathBuf, TempDir)> {
    let upload_result = gather_upload_id_context(&mut meta, api_client, dry_run).await;

    let (
        bundle_temp_file,
        // directory is removed on drop
        bundle_temp_dir,
    ) = BundlerUtil::new(meta, bep_result).make_tarball_in_temp_dir()?;
    tracing::info!("Flushed temporary tarball to {:?}", bundle_temp_file);

    if dry_run {
        tracing::info!("Dry run enabled, not uploading bundle to S3");
        return Ok((bundle_temp_file, bundle_temp_dir));
    }

    match upload_result {
        Ok(upload) => {
            api_client
                .put_bundle_to_s3(&upload.url, &bundle_temp_file)
                .await?;
            if exit_code == EXIT_SUCCESS {
                tracing::info!("Upload successful");
            } else {
                tracing::info!(
                    "Upload successful; returning unsuccessful exit code of test run: {}",
                    exit_code
                );
            }

            Ok((bundle_temp_file, bundle_temp_dir))
        }
        Err(e) => {
            tracing::error!("Failed to gather upload ID: {}", e);
            Err(e)
        }
    }
}

impl EndOutput for UploadRunResult {
    fn output(&self) -> anyhow::Result<Vec<Line>> {
        let mut output: Vec<Line> = Vec::new();
        // If there is an error report, we display it instead
        if let Some(error_report) = self.error_report.as_ref() {
            output.push(Line::default());
            output.extend(error_report.output()?);
            return Ok(output);
        }
        if !self.validations.validations.is_empty() {
            output.extend(
                self.validations
                    .output_with_report_limits(&self.validation_report)?,
            );
            output.push(Line::default());
        }
        // Summary statistics
        let total_tests = self.meta.junit_props.num_tests;
        let quarantined = self
            .quarantine_context
            .quarantine_status
            .quarantine_results
            .len();
        let failures = self.quarantine_context.failures.len();
        let passes = total_tests.saturating_sub(quarantined + failures);
        let pass_ratio = if total_tests > 0 {
            format!("{:.1}%", (passes as f64 / total_tests as f64) * 100.0)
        } else {
            "N/A".to_string()
        };
        output.push(Line::from_iter([Span::new_styled(
            style("📚 Test Report".to_string()).attribute(Attribute::Bold),
        )?]));
        output.push(Line::from_iter([Span::new_unstyled(format!(
            "   Total: {}   Pass: {}   Fail: {}   Quarantined: {}   Pass Ratio: {}",
            total_tests, passes, failures, quarantined, pass_ratio
        ))?]));
        if self.quarantine_context.fetch_status.is_failure() {
            output.push(Line::from_iter([Span::new_styled(
                style("   We were unable to determine the quarantine status for tests. Any failing tests will be reported as failures".to_string()).attribute(Attribute::Dim),
            )?]));
        }
        output.push(Line::default());
        let qc = &self.quarantine_context;
        let quarantined = &qc.quarantine_status.quarantine_results;
        let failures = &qc.failures;
        let quarantined_count = quarantined.len();
        let non_quarantined_count = failures.len();
        let all_quarantined = non_quarantined_count == 0 && quarantined_count > 0;

        // Helper closure to render the test table
        let render_test_table = |tests: &[Test]| -> anyhow::Result<Lines> {
            use std::collections::BTreeMap;
            let mut output = Vec::new();
            // Group tests by file path, then by suite, then standalone
            let mut groups: BTreeMap<String, Vec<&Test>> = BTreeMap::new();
            let mut suite_groups: BTreeMap<String, Vec<&Test>> = BTreeMap::new();
            let mut standalone: Vec<&Test> = Vec::new();
            for test in tests.iter() {
                if let Some(file) = &test.file {
                    groups.entry(file.clone()).or_default().push(test);
                } else if !test.parent_name.is_empty() {
                    suite_groups
                        .entry(test.parent_name.clone())
                        .or_default()
                        .push(test);
                } else {
                    standalone.push(test);
                }
            }
            // Helper to render a group
            let mut render_group = |header: &str,
                                    color: Color,
                                    group: &Vec<&Test>|
             -> anyhow::Result<()> {
                output.push(Line::from_iter([Span::new_styled(
                    style(header.to_string())
                        .with(color)
                        .attribute(Attribute::Bold),
                )?]));
                for test in group.iter().take(3) {
                    let output_name = format!(
                        "{}{}{}",
                        test.parent_name,
                        if test.parent_name.is_empty() { "" } else { "/" },
                        test.name
                    );
                    let mut test_line = Line::from_iter([Span::new_styled(
                        style(output_name.to_string()).attribute(Attribute::Bold),
                    )?]);
                    test_line.pad_left(2);
                    output.push(test_line);
                    let link = url_for_test_case(
                        &get_api_host(),
                        &self.quarantine_context.org_url_slug,
                        &self.quarantine_context.repo,
                        test,
                    )?;
                    let mut link_output = Line::from_iter([
                        Span::new_unstyled("⤷ ")?,
                        Span::new_styled(style(link.to_string()).attribute(Attribute::Underlined))?,
                    ]);
                    link_output.pad_left(4);
                    output.push(link_output);
                    // Display failure message if present and enabled
                    // TODO: show_failure_messages is a temporary flag to show failure messages
                    // in the output. It should be removed once we are confident in this flow
                    // and we should use the validation report flag instead.
                    if self.show_failure_messages && test.failure_message.is_some() {
                        let failure_message = test.failure_message.as_ref().unwrap();
                        let lines: Vec<&str> = failure_message.split('\n').collect();
                        let max_lines = 20;
                        let shown_lines = lines.iter().take(max_lines);
                        let mut failure_header = Line::from_iter([Span::new_styled(
                            style("Failure: ".to_string()).with(Color::DarkGrey),
                        )?]);
                        failure_header.pad_left(4);
                        output.push(failure_header);
                        for (j, line) in shown_lines.enumerate() {
                            let sanitized_line = line.replace('\t', "    ").replace('\r', "");
                            if !sanitized_line.trim().is_empty() || j == 0 {
                                let mut failure_output = Line::from_iter([
                                    Span::new_unstyled("   ")?,
                                    Span::new_styled_lossy(
                                        style(sanitized_line.to_string())
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
                    }
                }
                if group.len() > 3 {
                    let mut more_failures = Line::from_iter([Span::new_unstyled(format!(
                        "…and {} more failures in this group",
                        group.len() - 3
                    ))?]);
                    more_failures.pad_left(2);
                    output.push(more_failures);
                }
                output.push(Line::default());
                Ok(())
            };
            // Render file path groups (cyan)
            for (file, group) in &groups {
                render_group(&format!("📁 {}", file), Color::Cyan, group)?;
            }
            // Render suite groups (magenta)
            for (suite, group) in &suite_groups {
                render_group(&format!("📦 {}", suite), Color::Magenta, group)?;
            }
            // Render standalone (yellow)
            if !standalone.is_empty() {
                render_group("🔍 Other", Color::Yellow, &standalone)?;
            }
            let mut output = Lines(output);
            output.pad_lines_left(2);
            output.pad_lines_bottom(1);
            Ok(output)
        };

        // Quarantined section
        if quarantined_count > 0 {
            output.extend(vec![
                Line::from_iter([
                    Span::new_unstyled("❤️‍🩹  ")?,
                    Span::new_styled(
                        style(format!(
                            "{} ",
                            pluralize("test", quarantined_count as isize, true)
                        ))
                        .attribute(Attribute::Bold),
                    )?,
                    Span::new_styled(style(format!(
                        "failed and {} ",
                        pluralize("was", quarantined_count as isize, false)
                    )))?,
                    Span::new_styled(
                        style(String::from("quarantined"))
                            .with(Color::Yellow)
                            .attribute(Attribute::Bold),
                    )?,
                ]),
                Line::default(),
            ]);
            output.extend(render_test_table(quarantined)?);
        }

        // Non-quarantined failures section
        if non_quarantined_count > 0 {
            output.extend(vec![
                Line::from_iter([
                    Span::new_unstyled("❌ ")?,
                    Span::new_unstyled(format!(
                        "{} ",
                        pluralize("test", non_quarantined_count as isize, true)
                    ))?,
                    Span::new_styled(
                        style(String::from("failed "))
                            .with(Color::Red)
                            .attribute(Attribute::Bold),
                    )?,
                    Span::new_styled(style(format!(
                        "and {} ",
                        pluralize("was", non_quarantined_count as isize, false)
                    )))?,
                    Span::new_styled(style(String::from("not ")).attribute(Attribute::Bold))?,
                    Span::new_unstyled("quarantined")?,
                ]),
                Line::default(),
            ]);
            output.extend(render_test_table(failures)?);
        }

        // Final messages
        if self.meta.junit_props.num_tests == 0 {
            output.push(Line::from_iter([Span::new_unstyled(
                "⚠️  No tests were found in the provided test results",
            )?]));
        } else if all_quarantined {
            output.push(Line::from_iter([
                Span::new_unstyled("🎉 All test failures were quarantined, overriding exit code to be exit_success ")?,
                Span::new_styled(style(format!(
                    "({})",
                    EXIT_SUCCESS
                )).attribute(Attribute::Bold))?
            ]));
        } else if failures.is_empty() && self.quarantine_context.exit_code == EXIT_SUCCESS {
            output.push(Line::from_iter([Span::new_unstyled(
                "🎉 No test failures found!",
            )?]));
        } else if failures.is_empty() && self.quarantine_context.exit_code != EXIT_SUCCESS {
            // no test failures found but exit code not 0
            output.push(Line::from_iter([
                Span::new_unstyled("⚠️  No test failures found, but non zero exit code provided: ")?,
                Span::new_styled(
                    style(format!("{}", self.quarantine_context.exit_code))
                        .attribute(Attribute::Bold),
                )?,
            ]));
            output.push(
                Line::from_iter([
                Span::new_unstyled("This may indicate that some tests were not run or that there were other issues during the test run.")?,
            ]));
        } else {
            output.push(Line::from_iter([
                Span::new_unstyled(
                    "⚠️  Some test failures were not quarantined, using exit code: ".to_string(),
                )?,
                Span::new_styled(
                    style(format!("{}", self.quarantine_context.exit_code))
                        .attribute(Attribute::Bold),
                )?,
            ]));
        }
        Ok(output)
    }
}
