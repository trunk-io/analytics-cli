use std::env;
use std::io::Write;

use bundle::RunResult;
use clap::{Args, Parser, Subcommand};
use codeowners::CodeOwners;
use constants::{EXIT_FAILURE, SENTRY_DSN};
use context::{bazel_bep::parser::BazelBepParser, repo::BundleRepo};
use trunk_analytics_cli::{
    api_client::ApiClient,
    runner::{run_quarantine, run_test_command, JunitSpec},
    upload::{run_upload, UploadArgs},
    validate::validate,
};

#[derive(Debug, Parser)]
#[command(
    version = std::env!("CARGO_PKG_VERSION"),
    name = "trunk flakytests",
    about = "Trunk Flaky Tests CLI",
    bin_name = "trunk flakytests",
)]
struct Cli {
    #[command(subcommand)]
    pub command: Commands,
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
        required_unless_present = "bazel_bep_path",
        conflicts_with = "bazel_bep_path",
        value_delimiter = ',',
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        help = "Comma-separated list of glob paths to junit files."
    )]
    junit_paths: Vec<String>,
    #[arg(
        long,
        required_unless_present = "junit_paths",
        help = "Path to bazel build event protocol JSON file."
    )]
    bazel_bep_path: Option<String>,
    #[arg(long, help = "Show warning-level log messages in output.")]
    show_warnings: bool,
    #[arg(long, help = "Value to override CODEOWNERS file or directory path.")]
    pub codeowners_path: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Upload data to Trunk Flaky Tests
    Upload(UploadArgs),
    /// Run a test command and upload data to Trunk Flaky Tests
    Test(TestArgs),
    /// Validate that your test runner output is suitable for Trunk Flaky Tests
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
                Err(e) => match (*(e.root_cause())).downcast_ref::<std::io::Error>() {
                    Some(io_error) if io_error.kind() == std::io::ErrorKind::ConnectionRefused => {
                        log::warn!("Could not connect to trunk's server: {:?}", e);
                        std::process::exit(exitcode::OK);
                    }
                    _ => {
                        log::error!("Error: {:?}", e);
                        std::process::exit(exitcode::SOFTWARE);
                    }
                },
            }
        })
}

async fn run_test(test_args: TestArgs) -> anyhow::Result<i32> {
    let TestArgs {
        command,
        upload_args,
    } = test_args;
    let UploadArgs {
        junit_paths,
        bazel_bep_path,
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

    if junit_paths.is_empty() && bazel_bep_path.is_none() {
        return Err(anyhow::anyhow!("No junit paths provided."));
    }

    let api_client = ApiClient::new(String::from(token))?;

    let codeowners = CodeOwners::find_file(&repo.repo_root, codeowners_path);
    let junit_spec = if !junit_paths.is_empty() {
        JunitSpec::Paths(junit_paths.clone())
    } else {
        JunitSpec::BazelBep(bazel_bep_path.as_deref().unwrap_or_default().to_string())
    };

    log::info!("running command: {:?}", command);
    let run_result = run_test_command(
        &repo,
        &org_url_slug,
        command.first().unwrap(),
        command.iter().skip(1).collect(),
        junit_spec,
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
    if let Err(e) = run_upload(
        upload_args,
        Some(command.join(" ")),
        None, // don't re-run quarantine checks
        codeowners,
        exec_start,
    )
    .await
    {
        log::error!("Error uploading test results: {:?}", e)
    };

    Ok(exit_code)
}

async fn run(cli: Cli) -> anyhow::Result<i32> {
    match cli.command {
        Commands::Upload(upload_args) => {
            print_cli_start_info();
            run_upload(upload_args, None, None, None, None).await
        }
        Commands::Test(test_args) => run_test(test_args).await,
        Commands::Validate(validate_args) => {
            let ValidateArgs {
                junit_paths,
                bazel_bep_path,
                show_warnings,
                codeowners_path,
            } = validate_args;

            let junit_file_paths = match bazel_bep_path {
                Some(bazel_bep_path) => {
                    let mut parser = BazelBepParser::new(bazel_bep_path);
                    parser.parse()?;
                    parser.uncached_xml_files()
                }
                None => junit_paths,
            };

            print_cli_start_info();
            validate(junit_file_paths, show_warnings, codeowners_path).await
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
        "Starting trunk flakytests {} (git={}) rustc={}",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_SHA"),
        env!("VERGEN_RUSTC_SEMVER")
    );
}
