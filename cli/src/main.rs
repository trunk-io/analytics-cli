use std::env;

use clap::{Parser, Subcommand};
use clap_verbosity_flag::{log::LevelFilter, InfoLevel, Verbosity};
use third_party::sentry;
use tracing_subscriber::prelude::*;
use trunk_analytics_cli::{
    quarantine_command::{run_quarantine, QuarantineArgs},
    test_command::{run_test, TestArgs},
    upload_command::{run_upload, UploadArgs, UploadRunResult},
    validate_command::{run_validate, ValidateArgs},
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
    #[command(flatten)]
    pub verbose: Verbosity<InfoLevel>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Quarantine flaky tests and upload data to Trunk Flaky Tests
    Quarantine(QuarantineArgs),
    /// Run a test command and upload data to Trunk Flaky Tests
    Test(TestArgs),
    /// Upload data to Trunk Flaky Tests
    Upload(UploadArgs),
    /// Validate that your test runner output is suitable for Trunk Flaky Tests
    Validate(ValidateArgs),
}

impl Commands {
    pub fn command_name(&self) -> &str {
        match self {
            Commands::Quarantine(..) => "quarantine",
            Commands::Test(..) => "test",
            Commands::Upload(..) => "upload",
            Commands::Validate(..) => "validate",
        }
    }
}

// "the Sentry client must be initialized before starting an async runtime or spawning threads"
// https://docs.sentry.io/platforms/rust/#async-main-function
fn main() -> anyhow::Result<()> {
    let release_name = env!("CARGO_PKG_VERSION");
    let _guard = sentry::init(release_name.into(), None);

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let cli = Cli::parse();
            let log_level_filter = cli.verbose.log_level_filter();
            setup_logger(log_level_filter)?;
            tracing::info_span!("Running command", command = cli.command.command_name());
            match run(cli).await {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => match (*(e.root_cause())).downcast_ref::<std::io::Error>() {
                    Some(io_error) if io_error.kind() == std::io::ErrorKind::ConnectionRefused => {
                        tracing::warn!("Could not connect to trunk's server: {:?}", e);
                        std::process::exit(exitcode::OK);
                    }
                    _ => {
                        tracing::warn!("Error: {:?}", e);
                        std::process::exit(exitcode::SOFTWARE);
                    }
                },
            }
        })
}

async fn run(cli: Cli) -> anyhow::Result<i32> {
    tracing::info!(
        "Starting trunk flakytests {} (git={}) rustc={}",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_SHA"),
        env!("VERGEN_RUSTC_SEMVER")
    );
    match cli.command {
        Commands::Quarantine(quarantine_args) => run_quarantine(quarantine_args).await,
        Commands::Upload(upload_args) => {
            let UploadRunResult {
                exit_code,
                upload_bundle_error,
            } = run_upload(upload_args, None, None).await?;
            if let Some(upload_bundle_error) = upload_bundle_error {
                return Err(upload_bundle_error);
            }
            Ok(exit_code)
        }
        Commands::Test(test_args) => run_test(test_args).await,
        Commands::Validate(validate_args) => run_validate(validate_args).await,
    }
}

fn to_trace_filter(filter: log::LevelFilter) -> tracing::level_filters::LevelFilter {
    match filter {
        log::LevelFilter::Debug => tracing::level_filters::LevelFilter::DEBUG,
        log::LevelFilter::Error => tracing::level_filters::LevelFilter::ERROR,
        log::LevelFilter::Info => tracing::level_filters::LevelFilter::INFO,
        log::LevelFilter::Off => tracing::level_filters::LevelFilter::OFF,
        log::LevelFilter::Trace => tracing::level_filters::LevelFilter::TRACE,
        log::LevelFilter::Warn => tracing::level_filters::LevelFilter::WARN,
    }
}

fn setup_logger(log_level_filter: LevelFilter) -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .with(sentry_tracing::layer())
        .with(to_trace_filter(log_level_filter))
        .init();
    Ok(())
}
