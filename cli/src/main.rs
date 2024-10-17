use std::io::Write;
use std::time::SystemTime;
use std::{env, time::UNIX_EPOCH};

use api::BundleUploadStatus;
use clap::{Args, Parser, Subcommand};
use context::repo::BundleRepo;
use trunk_analytics_cli::{
    api_client::ApiClient,
    bundler::BundlerUtil,
    codeowners::CodeOwners,
    constants::{EXIT_FAILURE, EXIT_SUCCESS, SENTRY_DSN},
    runner::{build_filesets, extract_failed_tests, run_quarantine, run_test_command},
    scanner::EnvScanner,
    types::{BundleMeta, QuarantineBulkTestStatus, QuarantineRunResult, RunResult, META_VERSION},
    utils::parse_custom_tags,
    validate::validate,
};
#[cfg(target_os = "macos")]
use xcresult::XCResult;

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

    let api_client = ApiClient::new(token)?;

    let codeowners =
        codeowners.or_else(|| CodeOwners::find_file(&repo.repo_root, &codeowners_path));

    print_cli_start_info();

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

    let failures = extract_failed_tests(&repo, &org_url_slug, &file_sets).await;

    // Run the quarantine step and update the exit code.
    let exit_code = if failures.is_empty() {
        EXIT_SUCCESS
    } else {
        EXIT_FAILURE
    };
    let quarantine_run_results = if use_quarantining && quarantine_results.is_none() {
        Some(
            run_quarantine(
                &api_client,
                &api::GetQuarantineBulkTestStatusRequest {
                    repo: repo.repo.clone(),
                    org_url_slug: org_url_slug.clone(),
                },
                failures,
                exit_code,
            )
            .await,
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
    let upload = api_client
        .create_bundle_upload_intent(&api::CreateBundleUploadRequest {
            repo: repo.repo.clone(),
            org_url_slug: org_url_slug.clone(),
            client_version,
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
                println!("    {}", file.original_path);
            }
        }
    }

    let bundle_temp_dir = tempfile::tempdir()?;
    let bundle_time_file = bundle_temp_dir.path().join("bundle.tar.zstd");
    let bundler = BundlerUtil::new(meta);
    bundler.make_tarball(&bundle_time_file)?;
    log::info!("Flushed temporary tarball to {:?}", bundle_time_file);

    if dry_run {
        if let Err(e) = api_client
            .update_bundle_upload_status(&api::UpdateBundleUploadRequest {
                id: upload.id.clone(),
                upload_status: BundleUploadStatus::DryRun,
            })
            .await
        {
            log::warn!("{}", e);
        } else {
            log::debug!("Updated bundle upload status to DRY_RUN");
        }
        log::info!("Dry run, skipping upload.");
        return Ok(exit_code);
    }

    api_client
        .put_bundle_to_s3(&upload.url, &bundle_time_file)
        .await?;

    if let Err(e) = api_client
        .update_bundle_upload_status(&api::UpdateBundleUploadRequest {
            id: upload.id.clone(),
            upload_status: BundleUploadStatus::UploadComplete,
        })
        .await
    {
        log::warn!("{}", e)
    } else {
        log::debug!(
            "Updated bundle upload status to {:#?}",
            BundleUploadStatus::UploadComplete
        )
    }

    api_client
        .create_trunk_repo(&api::CreateRepoRequest {
            repo: repo.repo,
            org_url_slug,
            remote_urls: vec![repo.repo_url.clone()],
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

    let api_client = ApiClient::new(String::from(token))?;

    let codeowners = CodeOwners::find_file(&repo.repo_root, codeowners_path);

    log::info!("running command: {:?}", command);
    let run_result = run_test_command(
        &repo,
        &org_url_slug,
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
                &api_client,
                &api::GetQuarantineBulkTestStatusRequest {
                    repo: repo.repo,
                    org_url_slug: org_url_slug.clone(),
                },
                failures,
                run_exit_code,
            )
            .await,
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

async fn run(cli: Cli) -> anyhow::Result<i32> {
    match cli.command {
        Commands::Upload(upload_args) => run_upload(upload_args, None, None, None, None).await,
        Commands::Test(test_args) => run_test(test_args).await,
        Commands::Validate(validate_args) => {
            let ValidateArgs {
                junit_paths,
                show_warnings,
            } = validate_args;
            print_cli_start_info();
            validate(junit_paths, show_warnings).await
        }
    }
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

fn print_cli_start_info() {
    log::info!(
        "Starting trunk-analytics-cli {} (git={}) rustc={}",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_SHA"),
        env!("VERGEN_RUSTC_SEMVER")
    );
}
