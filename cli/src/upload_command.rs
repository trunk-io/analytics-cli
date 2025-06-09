use std::collections::BTreeMap;
use std::env;

use api::client::ApiClient;
use api::{client::get_api_host, urls::url_for_test_case};
use bundle::Test;
use bundle::{BundleMeta, BundlerUtil};
use clap::{ArgAction, Args};
use constants::EXIT_SUCCESS;
use context::bazel_bep::common::BepParseResult;
use context::junit::validator::{
    JunitReportValidation, JunitReportValidationFlatIssue, JunitValidationLevel,
};
use pluralizer::pluralize;
use superconsole::{
    style::{style, Attribute, Color, Stylize},
    Line, Span,
};
use superconsole::{Component, Dimensions, DrawMode, Lines};
use unicode_ellipsis::truncate_str_leading;

use crate::context_quarantine::QuarantineContext;
use crate::{
    context::{
        gather_debug_props, gather_exit_code_and_quarantined_tests_context,
        gather_initial_test_context, gather_post_test_context, gather_upload_id_context,
        generate_internal_file, PreTestContext,
    },
    error_report::ErrorReport,
    test_command::TestRunResult,
};

#[cfg(target_os = "macos")]
const JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG: &str = "xcresult_path";
#[cfg(not(target_os = "macos"))]
const JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG: &str = "junit_paths";

const MAX_FILE_ISSUES_TO_SHOW: usize = 5;

#[derive(Args, Clone, Debug, Default)]
pub struct UploadArgs {
    #[arg(
        long,
        required_unless_present_any = [JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG, "bazel_bep_path"],
        conflicts_with = "bazel_bep_path",
        value_delimiter = ',',
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        help = "Comma-separated list of glob paths to junit files."
    )]
    pub junit_paths: Vec<String>,
    #[arg(
        long,
        required_unless_present_any = [JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG, "junit_paths"],
        help = "Path to bazel build event protocol JSON file."
    )]
    pub bazel_bep_path: Option<String>,
    #[cfg(target_os = "macos")]
    #[arg(long,
        required_unless_present_any = ["junit_paths", "bazel_bep_path"],
        conflicts_with_all = ["junit_paths", "bazel_bep_path"],
        required = false, help = "Path of xcresult directory"
    )]
    pub xcresult_path: Option<String>,
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
        help = "Value to set the variant of the test results uploaded.",
        required = false,
        num_args = 1,
        hide = true
    )]
    pub variant: Option<String>,
    #[arg(
        long,
        help = "The exit code to use when not all tests are quarantined.",
        required = false,
        num_args = 1,
        hide = true
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
    pub validations: BTreeMap<String, anyhow::Result<JunitReportValidation>>,
}

pub async fn run_upload(
    upload_args: UploadArgs,
    pre_test_context: Option<PreTestContext>,
    test_run_result: Option<TestRunResult>,
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

    let api_client = ApiClient::new(&upload_args.token, &upload_args.org_url_slug)?;

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
    let internal_bundled_file = generate_internal_file(
        &meta.base_props.file_sets,
        &temp_dir,
        meta.base_props.codeowners.as_ref(),
    );
    let mut validations = BTreeMap::new();
    if let Ok((internal_bundled_file, junit_validations)) = internal_bundled_file {
        meta.internal_bundled_file = Some(internal_bundled_file);
        validations = junit_validations;
    }

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
            QuarantineContext::default()
        }
    };
    // trunk-ignore(clippy/assigning_clones)
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

    if upload_bundle_result.is_err() {
        tracing::error!("Failed to upload bundle");
    }
    let error_report = match upload_bundle_result {
        Ok(_) => None,
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
    })
}

async fn upload_bundle(
    mut meta: BundleMeta,
    api_client: &ApiClient,
    bep_result: Option<BepParseResult>,
    exit_code: i32,
) -> anyhow::Result<()> {
    let upload = gather_upload_id_context(&mut meta, api_client).await?;

    let (
        bundle_temp_file,
        // directory is removed on drop
        _bundle_temp_dir,
    ) = BundlerUtil::new(meta, bep_result).make_tarball_in_temp_dir()?;
    tracing::info!("Flushed temporary tarball to {:?}", bundle_temp_file);

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

    Ok(())
}

impl Component for UploadRunResult {
    fn draw_unchecked(&self, dimensions: Dimensions, mode: DrawMode) -> anyhow::Result<Lines> {
        let mut output: Vec<Line> = Vec::new();
        // If there is an error report, we display it instead
        if let Some(error_report) = self.error_report.as_ref() {
            output.push(Line::default());
            output.extend(error_report.draw_unchecked(dimensions, mode)?);
            return Ok(Lines(output));
        }

        let mut perfect_files = 0;
        output.push(Line::from_iter([
            Span::new_unstyled("🔎 ")?,
            Span::new_styled(String::from("File Validation").attribute(Attribute::Bold))?,
        ]));
        output.push(Line::from_iter([Span::new_styled(
            format!(
                "  {}",
                pluralize("file", self.validations.len() as isize, true)
            )
            .attribute(Attribute::Italic),
        )?]));

        for (file_name, validation_reports) in self.validations.iter() {
            match validation_reports {
                Err(e) => {
                    output.push(Line::from_iter([
                        Span::new_unstyled("❌ ")?,
                        Span::new_styled(
                            format!("{file_name} Could Not Be Parsed").attribute(Attribute::Bold),
                        )?,
                    ]));
                    output.push(Line::from_iter([
                        Span::new_unstyled(" ↪ ")?,
                        Span::new_unstyled_lossy(format!("{:?}", e)),
                    ]));
                }
                Ok(report) => {
                    let issues = report.all_issues_flat();
                    let sub_optimal_issues: Vec<&JunitReportValidationFlatIssue> = issues
                        .iter()
                        .filter(|issue| issue.level == JunitValidationLevel::SubOptimal)
                        .collect();
                    let invalid_issues: Vec<&JunitReportValidationFlatIssue> = issues
                        .iter()
                        .filter(|issue| issue.level == JunitValidationLevel::Invalid)
                        .collect();
                    match (sub_optimal_issues.is_empty(), invalid_issues.is_empty()) {
                        (false, false) => {
                            output.push(Line::from_iter([
                                Span::new_unstyled("❌ ")?,
                                Span::new_styled(
                                    format!("{file_name} Has Errors And Warnings")
                                        .attribute(Attribute::Bold),
                                )?,
                            ]));
                            output.push(Line::from_iter([
                                Span::new_unstyled(" ↪ ❌ ")?,
                                Span::new_styled(
                                    String::from("Errors").attribute(Attribute::Bold),
                                )?,
                            ]));
                            for error in invalid_issues.iter().take(MAX_FILE_ISSUES_TO_SHOW) {
                                output.push(Line::from_iter([
                                    Span::new_unstyled("   ↪ ")?,
                                    Span::new_unstyled(error.error_message.clone())?,
                                ]));
                            }
                            output.push(Line::from_iter([
                                Span::new_unstyled(" ↪ ⚠️  ")?,
                                Span::new_styled(
                                    String::from("Warnings").attribute(Attribute::Bold),
                                )?,
                            ]));
                            for warning in sub_optimal_issues.iter().take(MAX_FILE_ISSUES_TO_SHOW) {
                                output.push(Line::from_iter([
                                    Span::new_unstyled("   ↪ ")?,
                                    Span::new_unstyled(warning.error_message.clone())?,
                                ]));
                            }
                        }
                        (true, false) => {
                            output.push(Line::from_iter([
                                Span::new_unstyled("❌ ")?,
                                Span::new_styled(
                                    format!("{file_name} Has Errors").attribute(Attribute::Bold),
                                )?,
                            ]));
                            for issue in invalid_issues.iter().take(MAX_FILE_ISSUES_TO_SHOW) {
                                output.push(Line::from_iter([
                                    Span::new_unstyled(" ↪ ")?,
                                    Span::new_unstyled(issue.error_message.clone())?,
                                ]));
                            }
                        }
                        (false, true) => {
                            output.push(Line::from_iter([
                                Span::new_unstyled("⚠️  ")?,
                                Span::new_styled(
                                    format!("{file_name} Has Warnings").attribute(Attribute::Bold),
                                )?,
                            ]));
                            for warning in sub_optimal_issues.iter().take(MAX_FILE_ISSUES_TO_SHOW) {
                                output.push(Line::from_iter([
                                    Span::new_unstyled(" ↪ ")?,
                                    Span::new_unstyled(warning.error_message.clone())?,
                                ]));
                            }
                        }
                        (true, true) => {
                            perfect_files += 1;
                        }
                    }
                }
            }
        }

        if perfect_files > 0 {
            output.push(Line::from_iter([
                Span::new_unstyled("✅ ")?,
                Span::new_styled(
                    pluralize("fully correct file", perfect_files as isize, true)
                        .attribute(Attribute::Bold),
                )?,
            ]));
        }
        output.push(Line::default());

        output.push(Line::from_iter([Span::new_styled(
            String::from("Test Report").attribute(Attribute::Bold),
        )?]));
        output.push(Line::default());
        let qc = &self.quarantine_context;
        let quarantined = &qc.quarantine_status.quarantine_results;
        let failures = &qc.failures;
        let quarantined_count = quarantined.len();
        let non_quarantined_count = failures.len();
        let all_quarantined = non_quarantined_count == 0 && quarantined_count > 0;

        // Helper closure to render the test table
        let render_test_table = |tests: &[Test]| -> anyhow::Result<Lines> {
            let mut output = Vec::new();
            // look at the first 12 tests
            let max_take = 12;
            for test in tests.iter().take(max_take) {
                let output_name = format!(
                    "{}{}{}",
                    test.parent_name,
                    if test.parent_name.is_empty() { "" } else { "/" },
                    test.name
                );
                let truncated = truncate_str_leading(&output_name, 60);
                let link = url_for_test_case(
                    &get_api_host(),
                    &self.quarantine_context.org_url_slug,
                    &self.quarantine_context.repo,
                    test,
                )?;
                output.push(Line::from_iter([Span::new_styled(
                    style(truncated.to_string()).attribute(Attribute::Italic),
                )?]));
                let mut link_output = Line::from_iter([
                    Span::new_unstyled("⤷ ")?,
                    Span::new_styled(style(link).attribute(Attribute::Underlined))?,
                ]);
                link_output.pad_left(2);
                output.push(link_output);
                output.push(Line::default());
            }
            let mut output = Lines(output);
            output.pad_lines_left(2);
            if tests.len() > max_take {
                output.push(Line::from_iter([Span::new_unstyled(format!(
                    "…and {} more",
                    tests.len() - max_take
                ))?]));
            }
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
        Ok(Lines(output))
    }
}
