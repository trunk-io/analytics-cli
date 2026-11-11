use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use api::client::{ApiClient, ApiErrorEndpoint, GithubForkPullRequest};
use api::credential::{LegacyTokenRefused, TokenKind};
use api::urls::TestCaseGuidScope;
use bundle::{BundleMeta, BundlerUtil, QuarantineResolutionMode, unzip_tarball};
use clap::{ArgAction, Args};
use codeowners::OwnersSource;
use context::bazel_bep::common::BepParseResult;
use display::message::DisplayMessage;
use tempfile::TempDir;

use crate::context_quarantine::{QuarantineContext, quarantine_query_result};
use crate::exit_code::ExitCode;
use crate::validate_command::JunitReportValidations;
use crate::{
    context::{
        PreTestContext, gather_debug_props, gather_exit_code_and_quarantined_tests_context,
        gather_initial_test_context, gather_post_test_context, gather_upload_id_context,
        generate_internal_file, generate_internal_file_from_bep,
    },
    error_report::{ErrorReport, InterruptingError},
    report_limiting::ValidationReport,
    summary_output::Notice,
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
        required_unless_present_any = [JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG, "bazel_bep_path", "test_reports", "swift_test_xunit_paths"],
        conflicts_with = "bazel_bep_path",
        value_delimiter = ',',
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        help = "Comma-separated list of glob patterns to locate JUnit XML files (e.g., '**/test-results/**/*.xml')."
    )]
    pub junit_paths: Vec<String>,
    #[arg(
        long,
        required_unless_present_any = [JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG, "junit_paths", "test_reports", "swift_test_xunit_paths"],
        help = "Path to Bazel Build Event Protocol JSON file. BEP files contain test results and build metadata."
    )]
    pub bazel_bep_path: Option<String>,
    #[cfg(target_os = "macos")]
    #[arg(long,
        required_unless_present_any = ["junit_paths", "bazel_bep_path", "test_reports", "swift_test_xunit_paths"],
        conflicts_with_all = ["junit_paths", "bazel_bep_path"],
        required = false, help = "Path to Xcode XCResult bundle directory (macOS only)."
    )]
    pub xcresult_path: Option<String>,
    #[arg(
        long,
        required_unless_present_any = [JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG, "junit_paths", "bazel_bep_path", "swift_test_xunit_paths"],
        value_delimiter = ',',
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        help = "Comma-separated list of glob patterns to test report files. Supports JUnit XML, Bazel BEP, and XCResult formats."
    )]
    pub test_reports: Vec<String>,
    #[arg(
        long,
        env = constants::TRUNK_SWIFT_TEST_XUNIT_PATHS_ENV,
        value_delimiter = ',',
        help = "Comma-separated list of glob patterns to JUnit files written by \
                `swift test --xunit-output` (e.g. 'junit*.xml'), resolved against the \
                repository root. These carry no file path, so each test's file is taken from \
                where a language server says it is declared in the repository. One run writes \
                two files: swift-testing to `<name>-swift-testing.xml` and XCTest to \
                `<name>`, the latter only when `--parallel` is also passed. Upload both if \
                the project uses both frameworks.",
        required = false
    )]
    pub swift_test_xunit_paths: Vec<String>,
    /// Always required — do not make this optional. It is what attributes a run to an organization
    /// in our telemetry: it is tagged onto every error we forward to Sentry (see `setup_logger` in
    /// `main.rs`), recorded on the bundle as `base_props.org`, and scopes the upload metrics we
    /// report. Without it, neither a reported CI failure nor a run's metrics can be traced back to
    /// a customer.
    #[arg(long, env = constants::TRUNK_ORG_URL_SLUG_ENV, help = "Organization url slug.")]
    pub org_url_slug: String,
    /// Required on the command line. Kept an `Option` because the test-runner plugins build
    /// `UploadArgs` without clap; `run_upload` refuses a run that names no collection.
    #[arg(
        long = "test-collection-id",
        env = constants::TRUNK_TEST_COLLECTION_ID_ENV,
        help = "ID of the test collection to upload to: 8 alphanumeric characters, shown on the collection's page in Trunk.",
        required = true,
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        num_args = 1
    )]
    pub test_collection_short_id: Option<String>,
    #[arg(
        long,
        required_unless_present = "allow_forked_pr_uploads",
        env = constants::TRUNK_API_TOKEN_ENV,
        help = "A Trunk v2 API key (trunk_v2_...) with the Upload test results permission, from Settings → Developer → API Keys in the Trunk web app; or a Trunk JWT from `trunk login --api-key`, which lasts 15 minutes, so mint it right before this step. The organization token is refused: pin analytics-cli 0.17.x to keep using it. Defaults to TRUNK_API_TOKEN env var."
    )]
    pub token: Option<String>,
    /// Opting in explicitly is what keeps a job whose token secret failed to interpolate failing
    /// loudly: without this, "no credential" would silently become an anonymous upload, and the
    /// fail-open behaviour on auth errors would hide it behind a green CI step.
    #[arg(
        long,
        env = constants::TRUNK_ALLOW_FORKED_PR_UPLOADS_ENV,
        help = "Upload with no Trunk credential, as a forked pull request must since it cannot read repository secrets. Works only for a fork pull request on GitHub Actions, using the job's GITHUB_TOKEN: trunk-io/analytics-uploader passes it by default; when running the CLI directly, set `GITHUB_TOKEN: ${{ github.token }}` on the step. The collection must have forked-PR uploads enabled in Trunk."
    )]
    pub allow_forked_pr_uploads: bool,
    #[arg(
        long,
        env = constants::TRUNK_REPO_ROOT_ENV,
        help = "Path to repository root. Defaults to current directory."
    )]
    pub repo_root: Option<String>,
    #[arg(
        long,
        env = constants::TRUNK_REPO_URL_ENV,
        help = "Override the repository URL (normally from git config remote.origin.url)."
    )]
    pub repo_url: Option<String>,
    #[arg(
        long,
        env = constants::TRUNK_REPO_HEAD_SHA_ENV,
        help = "Override the repository HEAD commit SHA (normally from git HEAD).",
        value_parser = parse_sha
    )]
    pub repo_head_sha: Option<String>,
    #[arg(
        long,
        env = constants::TRUNK_REPO_HEAD_BRANCH_ENV,
        help = "Override the repository HEAD branch name (normally from git branch)."
    )]
    pub repo_head_branch: Option<String>,
    #[arg(
        long,
        env = constants::TRUNK_REPO_HEAD_COMMIT_EPOCH_ENV,
        help = "Override the HEAD commit timestamp in seconds since Unix epoch (normally from git commit timestamp)."
    )]
    pub repo_head_commit_epoch: Option<String>,
    #[arg(
        long,
        env = constants::TRUNK_CODEOWNERS_PATH_ENV,
        help = "Override the path to a CODEOWNERS file. Used to associate test failures with code owners."
    )]
    pub codeowners_path: Option<String>,
    #[arg(
        long,
        env = constants::TRUNK_CODEOWNERS_TYPE_ENV,
        value_enum,
        help = "Specify CODEOWNERS parser type ('github' or 'gitlab'). Optional; when unset, codeowners type will be inferred."
    )]
    pub codeowners_type: Option<CodeownersType>,
    #[arg(
        long,
        env = constants::TRUNK_CODEOWNERS_USE_BAZEL_TARGET_ENV,
        help = "Use the Bazel target label as a fallback path for CODEOWNERS association when no file attribute is present in the JUnit XML.",
        action = ArgAction::Set,
        required = false,
        require_equals = true,
        num_args = 0..=1,
        default_value = "false",
        default_missing_value = "true",
    )]
    pub use_bazel_target_for_codeowners: bool,
    #[arg(
        long,
        env = constants::TRUNK_DISABLE_QUARANTINING_ENV,
        help = "Disable test quarantining. When disabled, all test failures will be reported and affect the exit code.",
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
        env = constants::TRUNK_ALLOW_EMPTY_TEST_RESULTS_ENV,
        alias = "allow-missing-junit-files",
        help = "Allow upload to succeed even when no test result files are found. Useful for optional test runs.",
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
        env = constants::TRUNK_DRY_RUN_ENV,
        help = "Write the test bundle to a local file (./bundle_upload) instead of uploading to Trunk servers. Useful for debugging.",
        required = false,
        num_args = 0,
        default_value = "false",
        default_missing_value = "true"
    )]
    pub dry_run: bool,
    #[arg(
        long,
        env = constants::TRUNK_VARIANT_ENV,
        help = "Variant name for the test results (e.g., 'linux', 'macos', 'pr'). Used to group test runs. Max 64 characters.",
        required = false,
        num_args = 1
    )]
    pub variant: Option<String>,
    #[arg(
        long,
        env = constants::TRUNK_TEST_PROCESS_EXIT_CODE_ENV,
        help = "Override the test process exit code. Useful when the test runner exit code doesn't reflect the actual test results.",
        required = false,
        num_args = 1
    )]
    pub test_process_exit_code: Option<i32>,
    #[arg(
        long,
        env = constants::TRUNK_REPO_HEAD_AUTHOR_NAME_ENV,
        help = "Override the HEAD commit author name (normally from git commit author).",
        required = false,
        num_args = 1
    )]
    pub repo_head_author_name: Option<String>,
    #[arg(
        long,
        env = constants::TRUNK_PR_NUMBER_ENV,
        help = "Override the PR number (normally inferred from CI environment variables).",
        required = false,
        num_args = 1
    )]
    pub pr_number: Option<usize>,
    #[arg(
        long,
        env = constants::TRUNK_USE_UNCLONED_REPO_ENV,
        help = "Enable upload mode for repositories not cloned locally. Requires --repo-url, --repo-head-sha, --repo-head-branch, and --repo-head-author-name to be set.",
        required = false,
        require_equals = false,
        num_args = 0,
        default_value = "false",
        default_missing_value = "true",
        requires = "repo_url",
        requires = "repo_head_sha",
        requires = "repo_head_branch",
        requires = "repo_head_author_name",
        conflicts_with = "repo_root"
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
    #[cfg(target_os = "macos")]
    #[arg(
        long,
        env = constants::TRUNK_USE_EXPERIMENTAL_XCRESULT_TEST_LOCATIONS_ENV,
        help = "Flag to take an xcresult test's file from where a language server says it is declared, rather than from the failure that surfaced it. Reads the bundle with no legacy `xcresulttool get object` calls.",
        action = ArgAction::Set,
        required = false,
        require_equals = true,
        num_args = 0..=1,
        default_value = "false",
        default_missing_value = "true",
        hide = true,
        conflicts_with = "use_experimental_failure_summary"
    )]
    pub use_experimental_xcresult_test_locations: bool,
    #[cfg(target_os = "macos")]
    #[arg(
        long,
        env = constants::TRUNK_XCRESULT_TEST_LOCATIONS_MAX_FILES_ENV,
        help = "Most source files to parse when resolving xcresult test declarations.",
        required = false,
        default_value_t = xcresult::test_locations::Limits::default().max_files,
        hide = true
    )]
    pub xcresult_test_locations_max_files: usize,
    #[cfg(target_os = "macos")]
    #[arg(
        long,
        env = constants::TRUNK_XCRESULT_TEST_LOCATIONS_BUDGET_SECS_ENV,
        help = "Seconds to spend per language server when resolving xcresult test declarations. The clang server answers far slower per file than the Swift one, so an Objective-C heavy repo wants this raised.",
        required = false,
        default_value_t = xcresult::test_locations::Limits::default().budget.as_secs(),
        hide = true
    )]
    pub xcresult_test_locations_budget_secs: u64,
    #[cfg(target_os = "macos")]
    #[arg(
        long,
        env = constants::TRUNK_XCRESULT_TEST_LOCATIONS_REQUEST_TIMEOUT_SECS_ENV,
        help = "Seconds to wait for a single language server reply before giving up on it.",
        required = false,
        default_value_t = xcresult::test_locations::Limits::default().request_timeout.as_secs(),
        hide = true
    )]
    pub xcresult_test_locations_request_timeout_secs: u64,
    #[cfg(target_os = "macos")]
    #[arg(
        long,
        env = constants::TRUNK_XCRESULT_TEST_LOCATIONS_RETRIES_ENV,
        help = "How many times to replace a language server that stops answering with a fresh one.",
        required = false,
        default_value_t = xcresult::test_locations::Limits::default().retries,
        hide = true
    )]
    pub xcresult_test_locations_retries: usize,
    #[cfg(target_os = "macos")]
    #[arg(
        long,
        env = constants::TRUNK_XCRESULT_TEST_LOCATIONS_MAX_FILE_BYTES_ENV,
        help = "Largest source file to parse for declarations. One request carries the whole file, so an outsized generated one costs memory and usually a timeout for symbols it does not declare. Skipped without being read.",
        required = false,
        default_value_t = xcresult::test_locations::Limits::default().max_file_bytes,
        hide = true
    )]
    pub xcresult_test_locations_max_file_bytes: u64,
    #[arg(
        long,
        env = constants::TRUNK_VALIDATION_REPORT_ENV,
        help = "Control validation reporting verbosity. Options: 'limited' (default, shows first few), 'full' (shows all), or 'none' (hides validation output).",
        required = false,
        num_args = 1,
        default_value = "limited",
        default_missing_value = "limited"
    )]
    pub validation_report: ValidationReport,
    #[arg(
        long,
        env = constants::TRUNK_SHOW_FAILURE_MESSAGES_ENV,
        help = "Include test failure messages in the CLI output. Useful for debugging test failures.",
        required = false,
        num_args = 0,
        default_value = "false",
        default_missing_value = "true"
    )]
    pub show_failure_messages: bool,
    #[arg(
        long,
        env = constants::TRUNK_SUMMARY_OUTPUT_FILE_ENV,
        help = "Also write the summary as JSON to this file, e.g. for a step that comments on the pull request.",
        required = false,
        num_args = 1
    )]
    pub summary_output_file: Option<PathBuf>,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[value(rename_all = "lower")]
pub enum CodeownersType {
    #[default]
    GitHub,
    GitLab,
}

impl From<CodeownersType> for OwnersSource {
    fn from(value: CodeownersType) -> Self {
        match value {
            CodeownersType::GitHub => OwnersSource::GitHub,
            CodeownersType::GitLab => OwnersSource::GitLab,
        }
    }
}

fn parse_sha(s: &str) -> anyhow::Result<String> {
    if s.len() > 40 {
        anyhow::bail!(anyhow::Error::msg(format!(
            "Sha code must be at most 40 characters, was {}",
            s.len()
        )))
    } else {
        anyhow::Ok(s.into())
    }
}

impl UploadArgs {
    pub fn new(
        token: String,
        org_url_slug: String,
        junit_paths: Vec<String>,
        repo_root: Option<String>,
        disable_quarantining: bool,
    ) -> Self {
        Self {
            junit_paths,
            org_url_slug,
            token: Some(token),
            repo_root,
            allow_empty_test_results: true,
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

    for cause in error.chain() {
        if let Some(endpoint_context) = cause.downcast_ref::<ApiErrorEndpoint>() {
            return endpoint_context.unknown_error_reason();
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
    pub test_collection_short_id: Option<String>,
    pub api_address: String,
    pub guid_scope: Option<TestCaseGuidScope>,
    pub notices: Vec<Notice>,
    pub dry_run: bool,
    pub disable_quarantining: bool,
}

pub struct RunUploadOptions {
    pub pre_test_context: Option<PreTestContext>,
    pub test_run_result: Option<TestRunResult>,
    pub render_sender: Option<Sender<DisplayMessage>>,
    pub quarantine_query_result_override:
        Option<proto::upload_metrics::trunk::QuarantineQueryResult>,
    pub quarantine_resolution_mode_override: Option<QuarantineResolutionMode>,
    pub caller: UploadCaller,
}

/// Who runs the upload, which decides how its messages name a setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UploadCaller {
    /// `trunk-analytics-cli upload` or `test`, which take flags.
    #[default]
    Cli,
    /// A test-runner plugin, which reads the environment.
    TestRunnerPlugin,
}

impl Default for RunUploadOptions {
    fn default() -> Self {
        Self {
            pre_test_context: None,
            test_run_result: None,
            render_sender: None,
            quarantine_query_result_override: None,
            quarantine_resolution_mode_override: None,
            caller: UploadCaller::Cli,
        }
    }
}

pub async fn run_upload(
    upload_args: UploadArgs,
    options: RunUploadOptions,
) -> anyhow::Result<UploadRunResult> {
    let RunUploadOptions {
        pre_test_context,
        test_run_result,
        render_sender,
        quarantine_query_result_override,
        quarantine_resolution_mode_override,
        caller,
    } = options;
    // grab the exec start if provided (`test` subcommand) or use the current time
    let cli_started_at = if let Some(test_run_result) = test_run_result.as_ref() {
        test_run_result
            .exec_start
            .unwrap_or(chrono::Utc::now().into())
    } else {
        chrono::Utc::now().into()
    };

    let mut notices = Vec::new();

    // clap enforces this for `upload` and `test`; the test-runner plugins build `UploadArgs`
    // themselves. An InterruptingError, since any other error fails open: a warning and a green
    // step that hides a run whose results go nowhere.
    if upload_args
        .test_collection_short_id
        .as_ref()
        .is_none_or(|id| id.trim().is_empty())
    {
        return Err(anyhow::anyhow!(InterruptingError::new(
            missing_test_collection_id_message(caller)
        )));
    }

    let mut api_client = ApiClient::new(
        upload_args.token.clone(),
        upload_args.allow_forked_pr_uploads,
        &upload_args.org_url_slug,
        render_sender,
    )
    .map_err(refusal_interrupts)?;
    if upload_args.allow_forked_pr_uploads
        && let Some(pull_request) = env::var("GITHUB_EVENT_PATH")
            .ok()
            .and_then(|event_path| github_fork_pull_request(&event_path))
    {
        api_client.set_github_fork_pull_request(pull_request);
    }

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
            gather_debug_props(
                env::args().collect::<Vec<String>>(),
                upload_args.token.clone().unwrap_or_default(),
            ),
        )?
    };

    // A dry run makes no call, so it has nothing to log in for.
    if !upload_args.dry_run {
        api_client.log_in_github_fork_pull_request().await;
    }

    if let Some(bep_result) = &bep_result {
        notices.extend(bep_result.errors.iter().map(Notice::bep_parse_error));
    }

    let file_set_builder = gather_post_test_context(
        &mut meta,
        junit_path_wrappers,
        &upload_args.codeowners_path,
        upload_args.codeowners_type.map(Into::into),
        upload_args.allow_empty_test_results,
        &test_run_result,
    )?;

    let default_exit_code = upload_args
        .test_process_exit_code
        .or_else(|| test_run_result.as_ref().map(|r| r.exit_code))
        .map(ExitCode::PassedThrough);
    let quarantine_context = match gather_exit_code_and_quarantined_tests_context(
        &mut meta,
        upload_args.disable_quarantining,
        &api_client,
        &file_set_builder,
        default_exit_code,
        upload_args
            .test_collection_short_id
            .clone()
            .filter(|id| !id.is_empty()),
    )
    .await
    {
        Ok(context) => context,
        Err(e) => {
            tracing::error!("Failed to gather quarantine context: {}", e);
            QuarantineContext::fail_fetch(e)
        }
    };

    let quarantined_test_ids: Vec<String> = quarantine_context
        .quarantine_status
        .quarantine_results
        .clone()
        .iter()
        .map(|test| test.id.clone())
        .collect();

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
            meta.variant.clone(),
            &upload_args.org_url_slug,
            &quarantine_context.repo,
            &quarantined_test_ids,
            upload_args.use_bazel_target_for_codeowners,
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
            meta.variant.clone(),
            &upload_args.org_url_slug,
            &quarantine_context.repo,
            &quarantined_test_ids,
        )
    };
    let validations = if let Ok((internal_bundled_file, junit_validations)) = internal_bundled_file
    {
        meta.internal_bundled_file = Some(internal_bundled_file);
        JunitReportValidations::new(junit_validations)
    } else {
        JunitReportValidations::new(BTreeMap::new())
    };

    meta.base_props.quarantined_tests = quarantine_context
        .quarantine_status
        .quarantine_results
        .clone();

    let quarantine_resolution_mode = quarantine_resolution_mode_override
        .unwrap_or(quarantine_context.quarantine_resolution_mode);
    meta.base_props.quarantine_resolution_mode = quarantine_resolution_mode;

    meta.failed_tests = quarantine_context.failures.clone();

    let upload_started_at = chrono::Utc::now();
    tracing::info!("Uploading test results...");
    let upload_bundle_result = upload_bundle(
        &mut meta,
        upload_args.test_collection_short_id.clone(),
        &api_client,
        bep_result,
        quarantine_context.exit_code,
        upload_args.dry_run,
    )
    .await;
    let quarantine_query_result = quarantine_query_result_override.unwrap_or_else(|| {
        quarantine_query_result(upload_args.disable_quarantining, &quarantine_context)
    });
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
        quarantine_query_result: quarantine_query_result.into(),
        quarantine_resolution_mode: proto::upload_metrics::trunk::QuarantineResolutionMode::from(
            quarantine_resolution_mode,
        )
        .into(),
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
    let (guid_scope, error_report) = match upload_bundle_result {
        Ok(uploaded) => {
            if upload_args.dry_run {
                let curr_dir = env::current_dir()?;
                let bundle_file = curr_dir.join(DRY_RUN_OUTPUT_DIR);
                unzip_tarball(&uploaded.tarball, &bundle_file)?;
            }
            (uploaded.guid_scope, None)
        }
        Err(e) => (
            None,
            Some(ErrorReport::new(
                e,
                upload_args.org_url_slug.clone(),
                Some(
                    "There was an unexpected error that occurred while uploading test results"
                        .into(),
                ),
            )),
        ),
    };
    // A dry run or a failed upload has no upload ids, but a v2 quarantine lookup returns the same pair.
    let guid_scope = guid_scope.or_else(|| quarantine_context.guid_scope.clone());
    if guid_scope.is_none() && upload_args.test_collection_short_id.is_some() {
        tracing::debug!(
            "No test collection ids returned for this upload; test links will use the short-link form"
        );
    }
    Ok(UploadRunResult {
        quarantine_context,
        error_report,
        meta,
        validations,
        validation_report: upload_args.validation_report,
        show_failure_messages: upload_args.show_failure_messages,
        test_collection_short_id: upload_args
            .test_collection_short_id
            .filter(|id| !id.is_empty()),
        api_address: api_client.api_host.clone(),
        guid_scope,
        notices,
        dry_run: upload_args.dry_run,
        disable_quarantining: upload_args.disable_quarantining,
    })
}

/// Refuses a token that is neither a v2 API key nor a Trunk JWT, before the run calls Trunk or, for
/// `test`, runs its tests. An InterruptingError, since any other error fails open: a green step
/// whose results go nowhere.
pub fn refuse_a_legacy_token(token: Option<&str>) -> anyhow::Result<()> {
    match token.filter(|token| !token.trim().is_empty()) {
        Some(token) if TokenKind::of(token) == TokenKind::Legacy => {
            Err(refusal_interrupts(anyhow::Error::new(LegacyTokenRefused)))
        }
        _ => Ok(()),
    }
}

fn refusal_interrupts(error: anyhow::Error) -> anyhow::Error {
    match error.downcast_ref::<LegacyTokenRefused>() {
        Some(refusal) => anyhow::anyhow!(InterruptingError::new(refusal.to_string())),
        None => error,
    }
}

pub fn missing_test_collection_id_message(caller: UploadCaller) -> String {
    let how_to_pass = match caller {
        UploadCaller::Cli => String::from("Pass --test-collection-id"),
        UploadCaller::TestRunnerPlugin => {
            format!("Set {}", constants::TRUNK_TEST_COLLECTION_ID_ENV)
        }
    };
    format!(
        "This run names no test collection. {how_to_pass} to upload its test results; the ID is on the collection's page in Trunk."
    )
}

fn github_event_pull_request(event_path: &str) -> Option<serde_json::Value> {
    let mut event: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(event_path).ok()?).ok()?;
    Some(event.get_mut("pull_request")?.take())
}

fn pull_request_is_from_fork(pull_request: &serde_json::Value) -> Option<bool> {
    let head = pull_request.pointer("/head/repo/full_name")?.as_str()?;
    let base = pull_request.pointer("/base/repo/full_name")?.as_str()?;
    Some(!head.eq_ignore_ascii_case(base))
}

/// The fork pull request a GitHub Actions run builds, with the job's `GITHUB_TOKEN`; `None` for
/// any other run.
fn github_fork_pull_request(event_path: &str) -> Option<GithubForkPullRequest> {
    let pull_request = github_event_pull_request(event_path)?;
    if !pull_request_is_from_fork(&pull_request)? {
        return None;
    }
    Some(GithubForkPullRequest {
        number: pull_request.get("number")?.as_u64()?,
        head_sha: pull_request
            .pointer("/head/sha")?
            .as_str()?
            .to_ascii_lowercase(),
        github_token: env::var("GITHUB_TOKEN").ok(),
    })
}

struct UploadedBundle {
    tarball: PathBuf,
    // directory is removed on drop
    _temp_dir: TempDir,
    guid_scope: Option<TestCaseGuidScope>,
}

async fn upload_bundle(
    meta: &mut BundleMeta,
    requested_test_collection_short_id: Option<String>,
    api_client: &ApiClient,
    bep_result: Option<BepParseResult>,
    exit_code: ExitCode,
    dry_run: bool,
) -> anyhow::Result<UploadedBundle> {
    let upload_result = gather_upload_id_context(
        meta,
        requested_test_collection_short_id,
        api_client,
        dry_run,
    )
    .await;

    let (
        bundle_temp_file,
        // directory is removed on drop
        bundle_temp_dir,
    ) = BundlerUtil::new(meta, bep_result).make_tarball_in_temp_dir()?;
    tracing::info!("Flushed temporary tarball to {:?}", bundle_temp_file);

    if dry_run {
        tracing::info!("Dry run enabled, not uploading bundle to S3");
        return Ok(UploadedBundle {
            tarball: bundle_temp_file,
            _temp_dir: bundle_temp_dir,
            guid_scope: None,
        });
    }

    match upload_result {
        Ok(upload) => {
            api_client
                .put_bundle_to_s3(&upload.url, &bundle_temp_file)
                .await?;
            if exit_code.is_success() {
                tracing::info!("Upload successful");
            } else {
                tracing::info!(
                    "Upload successful; returning unsuccessful exit code of test run: {}",
                    exit_code
                );
            }

            Ok(UploadedBundle {
                tarball: bundle_temp_file,
                _temp_dir: bundle_temp_dir,
                guid_scope: upload.test_collection_id.zip(upload.repo_id).map(
                    |(test_collection_id, repo_id)| TestCaseGuidScope {
                        test_collection_id,
                        repo_id,
                    },
                ),
            })
        }
        Err(e) => {
            tracing::error!("Failed to gather upload ID: {}", e);
            Err(e)
        }
    }
}

pub fn get_bundle_upload_id_message(bundle_upload_id: &str) -> String {
    format!("🏷️  Bundle Upload ID: {}", bundle_upload_id)
}

#[cfg(test)]
mod fork_pr_tests {
    use super::github_fork_pull_request;

    fn github_event(head: &str, base: &str) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            file.path(),
            serde_json::json!({
                "pull_request": {
                    "number": 42,
                    "head": {"sha": "ABCDEF0123", "repo": {"full_name": head}},
                    "base": {"repo": {"full_name": base}},
                }
            })
            .to_string(),
        )
        .unwrap();
        file
    }

    #[test]
    fn reads_a_github_pull_request_from_another_repository_as_a_fork() {
        let fork = github_event("someone/web", "acme/web");
        let same = github_event("acme/web", "ACME/web");
        let push = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(push.path(), r#"{"ref": "refs/heads/main"}"#).unwrap();

        assert_eq!(
            [&fork, &same, &push].map(|event| github_fork_pull_request(
                event.path().to_str().unwrap()
            )
            .map(|pull_request| (pull_request.number, pull_request.head_sha))),
            [Some((42, String::from("abcdef0123"))), None, None]
        );
    }
}
