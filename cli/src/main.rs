use anyhow::Context;
use context::junit::validator::{validate, JunitReportValidation, JunitValidationLevel};
use std::env;
use std::io::{BufReader, Write};
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(target_os = "macos")]
use xcresult::XCResult;

use api::BundleUploadStatus;
use clap::{Args, Parser, Subcommand};
use context::junit::parser::{JunitParseError, JunitParser};
use context::repo::BundleRepo;
use quick_junit::Report;
use tokio_retry::strategy::ExponentialBackoff;
use tokio_retry::Retry;
use trunk_analytics_cli::bundler::BundlerUtil;
use trunk_analytics_cli::clients::{
    create_bundle_upload_intent, create_trunk_repo, put_bundle_to_s3, update_bundle_upload_status,
};
use trunk_analytics_cli::codeowners::CodeOwners;
use trunk_analytics_cli::constants::{
    EXIT_FAILURE, EXIT_SUCCESS, SENTRY_DSN, TRUNK_PUBLIC_API_ADDRESS_ENV,
};
use trunk_analytics_cli::runner::{
    build_filesets, extract_failed_tests, run_quarantine, run_test_command,
};
use trunk_analytics_cli::scanner::EnvScanner;
use trunk_analytics_cli::types::{
    BundleMeta, QuarantineBulkTestStatus, QuarantineRunResult, RunResult, WithFilePath,
    META_VERSION,
};
use trunk_analytics_cli::utils::parse_custom_tags;

use colored::{ColoredString, Colorize};

#[derive(Debug, Parser)]
#[command(
    version = std::env!("CARGO_PKG_VERSION"),
    name = "trunk-analytics-cli",
    about = "Trunk Analytics CLI"
)]
struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

fn junit_require() -> &'static str {
    if cfg!(target_os = "macos") {
        "xcresult_path"
    } else {
        "junit_paths"
    }
}

#[derive(Args, Clone, Debug)]
struct UploadArgs {
    #[arg(
        long,
        required_unless_present = junit_require(),
        value_delimiter = ',',
        help = "Comma-separated list of glob paths to junit files."
    )]
    junit_paths: Vec<String>,
    #[cfg(target_os = "macos")]
    #[arg(long, required = false, help = "Path of xcresult directory")]
    xcresult_path: Option<String>,
    #[arg(long, help = "Organization url slug.")]
    org_url_slug: String,
    #[arg(
        long,
        required = true,
        env = "TRUNK_API_TOKEN",
        help = "Organization token. Defaults to TRUNK_API_TOKEN env var."
    )]
    token: String,
    #[arg(long, help = "Path to repository root. Defaults to current directory.")]
    repo_root: Option<String>,
    #[arg(long, help = "Value to override URL of repository.")]
    repo_url: Option<String>,
    #[arg(long, help = "Value to override SHA of repository head.")]
    repo_head_sha: Option<String>,
    #[arg(long, help = "Value to override branch of repository head.")]
    repo_head_branch: Option<String>,
    #[arg(long, help = "Value to override commit epoch of repository head.")]
    repo_head_commit_epoch: Option<String>,
    #[arg(
        long,
        value_delimiter = ',',
        help = "Comma separated list of custom tag=value pairs."
    )]
    tags: Vec<String>,
    #[arg(long, help = "Print files which will be uploaded to stdout.")]
    print_files: bool,
    #[arg(long, help = "Run metrics CLI without uploading to API.")]
    dry_run: bool,
    #[arg(long, help = "Value to tag team owner of upload.")]
    team: Option<String>,
    #[arg(long, help = "Value to override CODEOWNERS file or directory path.")]
    codeowners_path: Option<String>,
    #[arg(long, help = "Run commands with the quarantining step.")]
    use_quarantining: bool,
    #[arg(long, help = "Do not fail if no junit files are found.")]
    allow_missing_junit_files: bool,
}

#[derive(Args, Clone, Debug)]
struct TestArgs {
    #[command(flatten)]
    upload_args: UploadArgs,
    #[arg(
        required = true,
        allow_hyphen_values = true,
        trailing_var_arg = true,
        help = "Test command to invoke."
    )]
    command: Vec<String>,
}

#[derive(Args, Clone, Debug)]
struct ValidateArgs {
    #[arg(
        long,
        required = true,
        value_delimiter = ',',
        help = "Comma-separated list of glob paths to junit files."
    )]
    junit_paths: Vec<String>,
    #[arg(long, help = "Show warning-level log messages in output.")]
    show_warnings: bool,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Upload(UploadArgs),
    Test(TestArgs),
    Validate(ValidateArgs),
}

const DEFAULT_ORIGIN: &str = "https://api.trunk.io";
// Tokio-retry uses base ^ retry * factor formula.
// This will give us 8ms, 64ms, 512ms, 4096ms, 32768ms
const RETRY_BASE_MS: u64 = 8;
const RETRY_FACTOR: u64 = 1;
const RETRY_COUNT: usize = 5;

// "the Sentry client must be initialized before starting an async runtime or spawning threads"
// https://docs.sentry.io/platforms/rust/#async-main-function
fn main() -> anyhow::Result<()> {
    let mut options = sentry::ClientOptions::default();
    options.release = sentry::release_name!();

    #[cfg(feature = "force-sentry-env-dev")]
    {
        options.environment = Some("development".into())
    }

    let _guard = sentry::init((SENTRY_DSN, options));

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            setup_logger()?;
            let cli = Cli::parse();
            match run(cli).await {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    log::error!("Error: {:?}", e);
                    std::process::exit(exitcode::SOFTWARE);
                }
            }
        })
}

#[cfg(target_os = "macos")]
fn handle_xcresult(
    junit_temp_dir: &tempfile::TempDir,
    xcresult_path: Option<String>,
) -> Result<Vec<String>, anyhow::Error> {
    let mut temp_paths = Vec::new();
    if let Some(xcresult_path) = xcresult_path {
        let xcresult = XCResult::new(xcresult_path);
        let junits = xcresult?
            .generate_junits()
            .map_err(|e| anyhow::anyhow!("Failed to generate junit files from xcresult: {}", e))?;
        for (i, junit) in junits.iter().enumerate() {
            let mut junit_writer: Vec<u8> = Vec::new();
            junit.serialize(&mut junit_writer)?;
            let junit_temp_path = junit_temp_dir
                .path()
                .join(format!("xcresult_junit_{}.xml", i));
            let mut junit_temp = std::fs::File::create(&junit_temp_path)?;
            junit_temp
                .write_all(&junit_writer)
                .map_err(|e| anyhow::anyhow!("Failed to write junit file: {}", e))?;
            let junit_temp_path_str = junit_temp_path.to_str();
            if let Some(junit_temp_path_string) = junit_temp_path_str {
                temp_paths.push(junit_temp_path_string.to_string());
            } else {
                return Err(anyhow::anyhow!(
                    "Failed to convert junit temp path to string."
                ));
            }
        }
    }
    Ok(temp_paths)
}

async fn run_upload(
    upload_args: UploadArgs,
    test_command: Option<String>,
    quarantine_results: Option<QuarantineRunResult>,
    codeowners: Option<CodeOwners>,
    exec_start: Option<SystemTime>,
) -> anyhow::Result<i32> {
    let UploadArgs {
        #[cfg(target_os = "macos")]
        mut junit_paths,
        #[cfg(target_os = "linux")]
        junit_paths,
        #[cfg(target_os = "macos")]
        xcresult_path,
        org_url_slug,
        token,
        repo_root,
        repo_url,
        repo_head_sha,
        repo_head_branch,
        repo_head_commit_epoch,
        tags,
        print_files,
        dry_run,
        use_quarantining,
        allow_missing_junit_files,
        team,
        codeowners_path,
    } = upload_args;

    let repo = BundleRepo::new(
        repo_root,
        repo_url,
        repo_head_sha,
        repo_head_branch,
        repo_head_commit_epoch,
    )?;

    let api_address = get_api_address();

    let codeowners =
        codeowners.or_else(|| CodeOwners::find_file(&repo.repo_root, &codeowners_path));

    log::info!(
        "Starting trunk-analytics-cli {} (git={}) rustc={}",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_SHA"),
        env!("VERGEN_RUSTC_SEMVER")
    );

    if token.trim().is_empty() {
        return Err(anyhow::anyhow!("Trunk API token is required."));
    }

    let tags = parse_custom_tags(&tags)?;
    #[cfg(target_os = "macos")]
    let junit_temp_dir = tempfile::tempdir()?;
    #[cfg(target_os = "macos")]
    {
        let temp_paths = handle_xcresult(&junit_temp_dir, xcresult_path)?;
        junit_paths = [junit_paths.as_slice(), temp_paths.as_slice()].concat();
    }

    let (file_sets, file_counter) = build_filesets(
        &repo.repo_root,
        &junit_paths,
        team.clone(),
        &codeowners,
        exec_start,
    )?;

    if !allow_missing_junit_files && (file_counter.get_count() == 0 || file_sets.is_empty()) {
        return Err(anyhow::anyhow!("No JUnit files found to upload."));
    }

    let failures = extract_failed_tests(&repo, &org_url_slug, &file_sets).await?;

    // Run the quarantine step and update the exit code.
    let exit_code = if failures.is_empty() {
        EXIT_SUCCESS
    } else {
        EXIT_FAILURE
    };
    let quarantine_run_results = if use_quarantining && quarantine_results.is_none() {
        Some(
            run_quarantine(
                exit_code,
                failures,
                &api_address,
                &token,
                &org_url_slug,
                &repo,
                default_delay(),
            )
            .await?,
        )
    } else {
        quarantine_results
    };

    let (exit_code, resolved_quarantine_results) = if let Some(r) = quarantine_run_results.as_ref()
    {
        (r.exit_code, r.quarantine_status.clone())
    } else {
        (
            EXIT_SUCCESS,
            QuarantineBulkTestStatus {
                group_is_quarantined: false,
                quarantine_results: Vec::new(),
            },
        )
    };

    let envs = EnvScanner::scan_env();
    let os_info: String = env::consts::OS.to_string();

    let cli_version = format!(
        "cargo={} git={} rustc={}",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_SHA"),
        env!("VERGEN_RUSTC_SEMVER")
    );
    let client_version = format!("trunk-analytics-cli {}", cli_version);
    let upload = Retry::spawn(default_delay(), || {
        create_bundle_upload_intent(
            &api_address,
            &token,
            &org_url_slug,
            &repo.repo,
            &client_version,
        )
    })
    .await?;

    let meta = BundleMeta {
        version: META_VERSION.to_string(),
        org: org_url_slug.clone(),
        repo: repo.clone(),
        cli_version,
        bundle_upload_id: upload.id.clone(),
        tags,
        file_sets,
        envs,
        upload_time_epoch: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        test_command,
        quarantined_tests: resolved_quarantine_results.quarantine_results.to_vec(),
        os_info: Some(os_info),
        codeowners,
    };

    log::info!("Total files pack and upload: {}", file_counter.get_count());
    if file_counter.get_count() == 0 {
        log::warn!(
            "No JUnit files found to pack and upload using globs: {:?}",
            junit_paths
        );
    }

    if print_files {
        println!("Files to upload:");
        for file_set in &meta.file_sets {
            println!(
                "  File set ({:?}): {}",
                file_set.file_set_type, file_set.glob
            );
            for file in &file_set.files {
                println!("    {}", file.original_path_abs);
            }
        }
    }

    let bundle_temp_dir = tempfile::tempdir()?;
    let bundle_time_file = bundle_temp_dir.path().join("bundle.tar.zstd");
    let bundler = BundlerUtil::new(meta);
    bundler.make_tarball(&bundle_time_file)?;
    log::info!("Flushed temporary tarball to {:?}", bundle_time_file);

    if dry_run {
        if let Err(e) = update_bundle_upload_status(
            &api_address,
            &token,
            &upload.id,
            &BundleUploadStatus::DryRun,
        )
        .await
        {
            log::warn!("Failed to update bundle upload status: {}", e);
        } else {
            log::debug!("Updated bundle upload status to DRY_RUN");
        }
        log::info!("Dry run, skipping upload.");
        return Ok(exit_code);
    }

    let upload_status = Retry::spawn(default_delay(), || {
        put_bundle_to_s3(&upload.url, &bundle_time_file)
    })
    .await
    .map(|_| BundleUploadStatus::UploadComplete)
    .unwrap_or_else(|e| {
        log::error!("Failed to upload bundle to S3 after retries: {}", e);
        BundleUploadStatus::UploadFailed
    });
    if let Err(e) =
        update_bundle_upload_status(&api_address, &token, &upload.id, &upload_status).await
    {
        log::warn!(
            "Failed to update bundle upload status to {:#?}: {}",
            upload_status,
            e
        )
    } else {
        log::debug!("Updated bundle upload status to {:#?}", upload_status)
    }

    let remote_urls = vec![repo.repo_url.clone()];
    Retry::spawn(default_delay(), || {
        create_trunk_repo(
            &api_address,
            &token,
            &org_url_slug,
            &repo.repo,
            &remote_urls,
        )
    })
    .await?;

    log::info!("Done");
    Ok(exit_code)
}

async fn run_test(test_args: TestArgs) -> anyhow::Result<i32> {
    let TestArgs {
        command,
        upload_args,
    } = test_args;
    let UploadArgs {
        junit_paths,
        org_url_slug,
        token,
        repo_root,
        repo_url,
        repo_head_sha,
        repo_head_branch,
        repo_head_commit_epoch,
        use_quarantining,
        team,
        codeowners_path,
        ..
    } = &upload_args;

    let repo = BundleRepo::new(
        repo_root.clone(),
        repo_url.clone(),
        repo_head_sha.clone(),
        repo_head_branch.clone(),
        repo_head_commit_epoch.clone(),
    )?;

    if junit_paths.is_empty() {
        return Err(anyhow::anyhow!("No junit paths provided."));
    }

    let api_address = get_api_address();

    let codeowners = CodeOwners::find_file(&repo.repo_root, codeowners_path);

    log::info!("running command: {:?}", command);
    let run_result = run_test_command(
        &repo,
        org_url_slug,
        command.first().unwrap(),
        command.iter().skip(1).collect(),
        junit_paths,
        team.clone(),
        &codeowners,
    )
    .await
    .unwrap_or_else(|e| {
        log::error!("Test command failed to run: {}", e);
        RunResult {
            exit_code: EXIT_FAILURE,
            failures: Vec::new(),
            exec_start: None,
        }
    });

    let run_exit_code = run_result.exit_code;
    let failures = run_result.failures;

    let quarantine_run_result = if *use_quarantining {
        Some(
            run_quarantine(
                run_exit_code,
                failures,
                &api_address,
                token,
                org_url_slug,
                &repo,
                default_delay(),
            )
            .await?,
        )
    } else {
        None
    };

    let exit_code = quarantine_run_result
        .as_ref()
        .map(|r| r.exit_code)
        .unwrap_or(run_exit_code);

    let exec_start = run_result.exec_start;
    match run_upload(
        upload_args,
        Some(command.join(" ")),
        None, // don't re-run quarantine checks
        codeowners,
        exec_start,
    )
    .await
    {
        Ok(EXIT_SUCCESS) => (),
        Ok(code) => log::error!("Error uploading test results: {}", code),
        // TODO(TRUNK-12558): We should fail on configuration error _prior_ to running a test
        Err(e) => log::error!("Error uploading test results: {:?}", e),
    };

    Ok(exit_code)
}

async fn run_validate(validate_args: ValidateArgs) -> anyhow::Result<i32> {
    let ValidateArgs {
        junit_paths,
        show_warnings,
    } = validate_args;

    log::info!(
        "Starting trunk-analytics-cli {} (git={}) rustc={}",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_SHA"),
        env!("VERGEN_RUSTC_SEMVER")
    );

    let current_dir = std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_default();
    let (file_sets, file_counter) = build_filesets(&current_dir, &junit_paths, None, &None, None)?;

    if file_counter.get_count() == 0 || file_sets.is_empty() {
        return Err(anyhow::anyhow!("No JUnit files found to validate."));
    }

    log::info!("");
    log::info!(
        "Validating the following {} files matching the provided globs:",
        file_counter.get_count()
    );
    for file_set in &file_sets {
        log::info!(
            "  File set ({:?}): {}",
            file_set.file_set_type,
            file_set.glob
        );
        for file in &file_set.files {
            log::info!("    {}", file.original_path_rel);
        }
    }

    let mut reports: Vec<WithFilePath<Report>> = Vec::new();
    let mut parse_errors: Vec<WithFilePath<JunitParseError>> = Vec::new();
    file_sets.iter().try_for_each(|file_set| {
        file_set.files.iter().try_for_each(|bundled_file| {
            let path = std::path::Path::new(&bundled_file.original_path_abs);
            let file = std::fs::File::open(path)?;
            let file_buf_reader = BufReader::new(file);
            let mut junit_parser = JunitParser::new();
            junit_parser
                .parse(file_buf_reader)
                .context("Encountered unrecoverable error while parsing file")?;
            parse_errors.extend(junit_parser.errors().iter().map(|e| WithFilePath::<
                JunitParseError,
            > {
                file_path: bundled_file.original_path_rel.clone(),
                wrapped: *e,
            }));
            reports.extend(junit_parser.into_reports().iter().map(
                |report| WithFilePath::<Report> {
                    file_path: bundled_file.original_path_rel.clone(),
                    wrapped: report.clone(),
                },
            ));
            Ok::<(), anyhow::Error>(())
        })?;
        Ok::<(), anyhow::Error>(())
    })?;

    if !parse_errors.is_empty() && show_warnings {
        log::info!("");
        log::warn!(
            "Encountered the following {} non-fatal errors while parsing files:",
            parse_errors.len().to_string().yellow()
        );

        let mut current_file_original_path = parse_errors[0].file_path.clone();
        log::warn!("  File: {}", current_file_original_path);

        for error in parse_errors {
            if error.file_path != current_file_original_path {
                current_file_original_path = error.file_path;
                log::warn!("  File: {}", current_file_original_path);
            }

            log::warn!("    {}", error.wrapped);
        }
    }

    log::info!("");

    let report_validations: Vec<WithFilePath<JunitReportValidation>> = reports
        .into_iter()
        .map(|report| WithFilePath::<JunitReportValidation> {
            file_path: report.file_path,
            wrapped: validate(&report.wrapped),
        })
        .collect();

    let mut num_invalid_reports = 0;
    let mut num_optionally_invalid_reports = 0;
    for report_validation in &report_validations {
        let num_test_cases = report_validation.wrapped.test_cases_flat().len();
        let num_invalid_validation_errors = report_validation
            .wrapped
            .test_suite_invalid_validation_issues_flat()
            .len()
            + report_validation
                .wrapped
                .test_case_invalid_validation_issues_flat()
                .len();
        let num_optional_validation_errors = report_validation
            .wrapped
            .test_suite_suboptimal_validation_issues_flat()
            .len()
            + report_validation
                .wrapped
                .test_case_suboptimal_validation_issues_flat()
                .len();

        let num_validation_errors_str = if num_invalid_validation_errors > 0 {
            num_invalid_validation_errors.to_string().red()
        } else {
            num_invalid_validation_errors.to_string().green()
        };
        let num_optional_validation_errors_str = if num_optional_validation_errors > 0 {
            format!(
                ", {} optional validation errors",
                num_optional_validation_errors.to_string().yellow()
            )
        } else {
            String::from("")
        };
        log::info!(
            "{} - {} test suites, {} test cases, {} validation errors{}",
            report_validation.file_path,
            report_validation.wrapped.test_suites().len(),
            num_test_cases,
            num_validation_errors_str,
            num_optional_validation_errors_str,
        );

        for test_suite_validation_error in report_validation
            .wrapped
            .test_suite_validation_issues_flat()
        {
            log::info!(
                "  {} - {}",
                print_validation_level(JunitValidationLevel::from(test_suite_validation_error)),
                test_suite_validation_error.to_string(),
            );
        }

        for test_case_validation_error in
            report_validation.wrapped.test_case_validation_issues_flat()
        {
            log::info!(
                "  {} - {}",
                print_validation_level(JunitValidationLevel::from(test_case_validation_error)),
                test_case_validation_error.to_string(),
            );
        }

        if num_invalid_validation_errors > 0 {
            num_invalid_reports += 1;
        }
        if num_optional_validation_errors > 0 {
            num_optionally_invalid_reports += 1;
        }
    }

    log::info!("");

    if num_invalid_reports == 0 {
        let num_optional_validation_errors_str = if num_optionally_invalid_reports > 0 {
            format!(
                " ({} files with optional validation errors)",
                num_optionally_invalid_reports.to_string().yellow()
            )
        } else {
            String::from("")
        };

        log::info!(
            "All {} files are valid!{}",
            report_validations.len().to_string().green(),
            num_optional_validation_errors_str
        );
        log::info!("Navigate to <URL for next onboarding step> to continue getting started with Flaky Tests");
        return Ok(EXIT_SUCCESS);
    }

    let num_optional_validation_errors_str = if num_optionally_invalid_reports > 0 {
        format!(
            ", {} files have optional validation errors",
            num_optionally_invalid_reports.to_string().yellow()
        )
    } else {
        String::from("")
    };
    log::info!(
        "{} files are valid, {} files are not valid{}",
        (report_validations.len() - num_invalid_reports)
            .to_string()
            .green(),
        num_invalid_reports.to_string().red(),
        num_optional_validation_errors_str,
    );

    Ok(EXIT_FAILURE)
}

async fn run(cli: Cli) -> anyhow::Result<i32> {
    match cli.command {
        Commands::Upload(upload_args) => run_upload(upload_args, None, None, None, None).await,
        Commands::Test(test_args) => run_test(test_args).await,
        Commands::Validate(validate_args) => run_validate(validate_args).await,
    }
}

fn default_delay() -> std::iter::Take<ExponentialBackoff> {
    ExponentialBackoff::from_millis(RETRY_BASE_MS)
        .factor(RETRY_FACTOR)
        .take(RETRY_COUNT)
}

fn setup_logger() -> anyhow::Result<()> {
    let mut builder = env_logger::Builder::new();
    builder
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] - {}",
                chrono::Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .filter(None, log::LevelFilter::Info);
    if let Ok(log) = std::env::var("TRUNK_LOG") {
        builder.parse_filters(&log);
    }
    builder.init();
    Ok(())
}

fn get_api_address() -> String {
    std::env::var(TRUNK_PUBLIC_API_ADDRESS_ENV)
        .ok()
        .and_then(|s| if s.is_empty() { None } else { Some(s) })
        .unwrap_or_else(|| DEFAULT_ORIGIN.to_string())
}

fn print_validation_level(level: JunitValidationLevel) -> ColoredString {
    match level {
        JunitValidationLevel::SubOptimal => "OPTIONAL".yellow(),
        JunitValidationLevel::Invalid => "INVALID".red(),
        JunitValidationLevel::Valid => "VALID".green(),
    }
}
