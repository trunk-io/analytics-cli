use std::env;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Args, Parser, Subcommand};
use tokio_retry::strategy::ExponentialBackoff;
use tokio_retry::Retry;
use trunk_analytics_cli::bundler::BundlerUtil;
use trunk_analytics_cli::clients::{
    create_trunk_repo, get_bundle_upload_location, put_bundle_to_s3,
};
use trunk_analytics_cli::constants::{EXIT_FAILURE, EXIT_SUCCESS, TRUNK_PUBLIC_API_ADDRESS_ENV};
use trunk_analytics_cli::runner::{get_failures, get_files, run_quarantine, run_test_command};
use trunk_analytics_cli::scanner::{BundleRepo, EnvScanner};
use trunk_analytics_cli::types::{BundleMeta, QuarantineBulkTestStatus, RunResult, META_VERSION};
use trunk_analytics_cli::utils::{from_non_empty_or_default, parse_custom_tags};

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

#[derive(Args, Clone, Debug)]
struct UploadArgs {
    #[arg(
        long,
        required = true,
        value_delimiter = ',',
        help = "Comma-separated list of glob paths to junit files."
    )]
    junit_paths: Vec<String>,
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
    #[arg(long, help = "Run commands without the quarantining step.")]
    quarantining: bool,
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

#[derive(Debug, Subcommand)]
enum Commands {
    Upload(UploadArgs),
    Test(TestArgs),
}

const DEFAULT_ORIGIN: &str = "https://api.trunk.io";
// Tokio-retry uses base ^ retry * factor formula.
// This will give us 8ms, 64ms, 512ms, 4096ms, 32768ms
const RETRY_BASE_MS: u64 = 8;
const RETRY_FACTOR: u64 = 1;
const RETRY_COUNT: usize = 5;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    setup_logger()?;
    let cli = Cli::parse();
    match run(cli).await {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(e) => {
            log::error!("Error: {:?}", e);
            std::process::exit(exitcode::SOFTWARE);
        }
    }
}

async fn run_upload(
    upload_args: UploadArgs,
    test_command: Option<String>,
    quarantine_results: Option<QuarantineBulkTestStatus>,
) -> anyhow::Result<i32> {
    let UploadArgs {
        junit_paths,
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
        quarantining,
        team,
        codeowners_path,
    } = upload_args;

    let repo = BundleRepo::try_read_from_root(
        repo_root,
        repo_url,
        repo_head_sha,
        repo_head_branch,
        repo_head_commit_epoch,
    )?;

    if junit_paths.is_empty() {
        return Err(anyhow::anyhow!("No junit paths provided."));
    }

    let api_address = from_non_empty_or_default(
        std::env::var(TRUNK_PUBLIC_API_ADDRESS_ENV).ok(),
        DEFAULT_ORIGIN.to_string(),
        |s| s,
    );
    let mut exit_code: i32 = EXIT_SUCCESS;

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

    let (file_sets, file_counter) =
        get_files(&repo, &junit_paths, team.clone(), codeowners_path.clone())?;
    let failures = get_failures(&file_sets, None).await?;

    // Run the quarantine step and update the exit code.
    let quarantine_run_result = run_quarantine(
        &RunResult {
            exit_code,
            failures,
        },
        &api_address,
        &token,
        &org_url_slug,
        &repo,
        default_delay(),
        quarantining,
    )
    .await?;
    exit_code = quarantine_run_result.exit_code;

    let envs = EnvScanner::scan_env();
    let os_info: String = env::consts::OS.to_string();
    let resolved_quarantine_results = quarantine_results.unwrap_or(QuarantineBulkTestStatus {
        group_is_quarantined: false,
        quarantine_results: Vec::new(),
    });
    let meta = BundleMeta {
        version: META_VERSION.to_string(),
        cli_version: format!(
            "cargo={} git={} rustc={}",
            env!("CARGO_PKG_VERSION"),
            env!("VERGEN_GIT_SHA"),
            env!("VERGEN_RUSTC_SEMVER")
        ),
        org: org_url_slug.clone(),
        repo: repo.clone(),
        tags,
        file_sets,
        envs,
        upload_time_epoch: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        test_command,
        group_is_quarantined: resolved_quarantine_results.group_is_quarantined,
        quarantined_tests: resolved_quarantine_results
            .quarantine_results
            .iter()
            .map(|qr| qr.run_info_id.clone())
            .collect(),
        os_info: Some(os_info),
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

    let upload_op = Retry::spawn(default_delay(), || {
        get_bundle_upload_location(&api_address, &token, &org_url_slug, &repo.repo)
    })
    .await?;

    if dry_run {
        log::info!("Dry run, skipping upload.");
        return Ok(exit_code);
    }

    if let Some(upload) = upload_op {
        Retry::spawn(default_delay(), || {
            put_bundle_to_s3(&upload.url, &bundle_time_file)
        })
        .await?;
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
        quarantining,
        team,
        codeowners_path,
        ..
    } = &upload_args;

    let repo = BundleRepo::try_read_from_root(
        repo_root.clone(),
        repo_url.clone(),
        repo_head_sha.clone(),
        repo_head_branch.clone(),
        repo_head_commit_epoch.clone(),
    )?;

    if junit_paths.is_empty() {
        return Err(anyhow::anyhow!("No junit paths provided."));
    }

    let api_address = from_non_empty_or_default(
        std::env::var(TRUNK_PUBLIC_API_ADDRESS_ENV).ok(),
        DEFAULT_ORIGIN.to_string(),
        |s| s,
    );

    log::info!("running command: {:?}", command);
    let run_result = run_test_command(
        &repo,
        command.first().unwrap(),
        command.iter().skip(1).collect(),
        junit_paths,
        team.clone(),
        codeowners_path.clone(),
    )
    .await
    .unwrap_or(RunResult {
        exit_code: EXIT_FAILURE,
        failures: Vec::new(),
    });

    let quarantine_run_result = run_quarantine(
        &run_result,
        &api_address,
        token,
        org_url_slug,
        &repo,
        default_delay(),
        *quarantining,
    )
    .await?;

    match run_upload(
        upload_args,
        Some(command.join(" ")),
        Some(quarantine_run_result.quarantine_status),
    )
    .await
    {
        Ok(EXIT_SUCCESS) => (),
        Ok(code) => log::error!("Error uploading test results: {}", code),
        Err(e) => log::error!("Error uploading test results: {:?}", e),
    }

    Ok(quarantine_run_result.exit_code)
}

async fn run(cli: Cli) -> anyhow::Result<i32> {
    match cli.command {
        Commands::Upload(upload_args) => run_upload(upload_args, None, None).await,
        Commands::Test(test_args) => run_test(test_args).await,
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
