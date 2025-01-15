use std::{env, io::Write};

use clap::{Parser, Subcommand};
use constants::SENTRY_DSN;

use trunk_analytics_cli::{
    test_command::{run_test, TestArgs},
    upload_command::{run_upload, UploadArgs},
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

async fn run(cli: Cli) -> anyhow::Result<i32> {
    log::info!(
        "Starting trunk flakytests {} (git={}) rustc={}",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_SHA"),
        env!("VERGEN_RUSTC_SEMVER")
    );
    match cli.command {
        Commands::Upload(upload_args) => run_upload(upload_args, None, None).await,
        Commands::Test(test_args) => run_test(test_args).await,
        Commands::Validate(validate_args) => run_validate(validate_args).await,
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
