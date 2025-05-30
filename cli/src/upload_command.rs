use std::env;

use api::client::ApiClient;
use bundle::{BundleMeta, BundlerUtil};
use clap::{ArgAction, Args};
use constants::EXIT_SUCCESS;
use context::bazel_bep::common::BepParseResult;

use crate::{
    context::{
        gather_debug_props, gather_exit_code_and_quarantined_tests_context,
        gather_initial_test_context, gather_post_test_context, gather_upload_id_context,
        generate_internal_file, PreTestContext,
    },
    error_report::error_reason,
    test_command::TestRunResult,
};

#[cfg(target_os = "macos")]
const JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG: &str = "xcresult_path";
#[cfg(not(target_os = "macos"))]
const JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG: &str = "junit_paths";

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
        help = "Hide the top-level flaky tests banner",
        action = ArgAction::Set,
        required = false,
        require_equals = true,
        num_args = 0..=1,
        default_value = "false",
        default_missing_value = "true",
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

pub struct UploadRunResult {
    pub exit_code: i32,
    pub upload_bundle_error: Option<anyhow::Error>,
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
    if let Ok(internal_bundled_file) = internal_bundled_file {
        meta.internal_bundled_file = Some(internal_bundled_file);
    }

    let default_exit_code = if let Some(exit_code) = upload_args.test_process_exit_code {
        Some(exit_code)
    } else {
        test_run_result.as_ref().map(|r| r.exit_code)
    };
    let exit_code = gather_exit_code_and_quarantined_tests_context(
        &mut meta,
        upload_args.disable_quarantining || !upload_args.use_quarantining,
        &api_client,
        &file_set_builder,
        default_exit_code,
    )
    .await;

    let upload_started_at = chrono::Utc::now();
    tracing::info!("Uploading test results...");
    let upload_bundle_result =
        upload_bundle(meta.clone(), &api_client, bep_result, exit_code).await;
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
    Ok(UploadRunResult {
        exit_code,
        upload_bundle_error: upload_bundle_result.err(),
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
