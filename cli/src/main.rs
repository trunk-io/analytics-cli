use std::env;
use std::ffi::OsString;

use clap::{Parser, Subcommand};
use clap_verbosity_flag::{log::LevelFilter, InfoLevel, Verbosity};
use third_party::sentry;
use tracing_subscriber::prelude::*;
use trunk_analytics_cli::{
    context::gather_debug_props,
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

impl Cli {
    pub fn debug_props(&self) -> String {
        let token = match &self.command {
            Commands::Quarantine(args) => Some(args.token()),
            Commands::Test(args) => Some(args.token()),
            Commands::Upload(args) => Some(args.token.clone()),
            Commands::Validate(..) => None,
        };

        token.map_or(
            format!("{:#?}", env::args().collect::<Vec<String>>()),
            |token| gather_debug_props(token).command_line,
        )
    }
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

// "the Sentry client must be initialized before starting an async runtime or spawning threads"
// https://docs.sentry.io/platforms/rust/#async-main-function
fn main() -> anyhow::Result<()> {
    let release_name = format!("analytics-cli@{}", env!("CARGO_PKG_VERSION"));
    let guard = sentry::init(release_name.into(), None);

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let cli = Cli::parse_from(build_command_line());
            let log_level_filter = cli.verbose.log_level_filter();
            setup_logger(log_level_filter)?;
            tracing::info!("{}", TITLE_CARD);
            tracing::info!(
                command = cli.debug_props(),
                "Trunk Flaky Test running command"
            );
            match run(cli).await {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => match (*(e.root_cause())).downcast_ref::<std::io::Error>() {
                    Some(io_error) if io_error.kind() == std::io::ErrorKind::ConnectionRefused => {
                        tracing::warn!("Could not connect to trunk's server: {:?}", e);
                        guard.flush(None);
                        std::process::exit(exitcode::OK);
                    }
                    _ => {
                        tracing::error!("Error: {:?}", e);
                        guard.flush(None);
                        std::process::exit(exitcode::SOFTWARE);
                    }
                },
            }
        })
}

fn build_command_line() -> Vec<OsString> {
    let mut base_command: Vec<OsString> = std::env::args_os().collect();
    let insertion_index = if base_command.len() > 2 {
        2
    } else {
        base_command.len()
    };
    let configs = configs_for_repo();
    configs
        .into_iter()
        .rev()
        .for_each(|config| base_command.insert(insertion_index, config));
    base_command
}

fn configs_for_repo() -> Vec<OsString> {
    home::home_dir()
        .into_iter()
        .map(|mut home_dir| {
            home_dir.push(".trunk-analytics-cli");
            home_dir.push("config.toml");
            home_dir
        })
        .map(move |home_dir| home_dir.into_boxed_path())
        .flat_map(|config_path| std::fs::read_to_string(config_path).into_iter())
        .flat_map(|config_data| config_data.parse::<toml::Table>().into_iter())
        .flat_map(|config_table| {
            let config_vec: Vec<OsString> = config_table
                .into_iter()
                .flat_map(|(key, value)| match value {
                    toml::Value::String(val) => {
                        vec![Into::<OsString>::into(format!("--{}={}", key, val))]
                    }
                    _ => vec![],
                })
                .collect();
            config_vec
        })
        .collect()
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

fn to_trace_filter(filter: log::LevelFilter) -> tracing::Level {
    match filter {
        log::LevelFilter::Debug => tracing::Level::DEBUG,
        log::LevelFilter::Error => tracing::Level::ERROR,
        log::LevelFilter::Info => tracing::Level::INFO,
        log::LevelFilter::Off => tracing::Level::TRACE,
        log::LevelFilter::Trace => tracing::Level::TRACE,
        log::LevelFilter::Warn => tracing::Level::WARN,
    }
}

fn setup_logger(log_level_filter: LevelFilter) -> anyhow::Result<()> {
    // trunk-ignore(clippy/match_ref_pats)
    let sentry_layer = sentry_tracing::layer().event_filter(|md| match md.level() {
        &tracing::Level::ERROR => sentry_tracing::EventFilter::Event,
        &tracing::Level::WARN => sentry_tracing::EventFilter::Breadcrumb,
        &tracing::Level::INFO => sentry_tracing::EventFilter::Breadcrumb,
        &tracing::Level::DEBUG => sentry_tracing::EventFilter::Breadcrumb,
        _ => sentry_tracing::EventFilter::Ignore,
    });
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::Layer::new()
                .without_time()
                .with_target(false)
                .with_writer(std::io::stdout.with_max_level(to_trace_filter(log_level_filter))),
        )
        .with(sentry_layer)
        .init();
    Ok(())
}

// Uses a raw string to avoid needing to escape quotes in the title card. This is mostly just so you can see
// what it looks like in code rather than needing to print.
const TITLE_CARD: &str = r#"
%%%%%%%%%%%  %%              %%                        %%%%%%%%%%%%                                        
%%           %%              %%                             %%                           ,d                
%%           %%              %%                             %%                           %%                
%%aaaaa      %%  ,adPPYYba,  %%   ,d%  %b       d%          %%   ,adPPYba,  ,adPPYba,  MM%%MMM  ,adPPYba,  
%%"""""      %%  ""     `Y%  %% ,a%"   `%b     d%'          %%  a%P_____%%  I%[    ""    %%     I%[    ""  
%%           %%  ,adPPPPP%%  %%%%[      `%b   d%'           %%  %PP"""""""   `"Y%ba,     %%      `"Y%ba,   
%%           %%  %%,    ,%%  %%`"Yba,    `%b,d%'            %%  "%b,   ,aa  aa    ]%I    %%,    aa    ]%I  
%%           %%  `"%bbdP"Y%  %%   `Y%a     Y%%'             %%   `"Ybbd%"'  `"YbbdP"'    "Y%%%  `"YbbdP"'  
                                           d%'                                                             
                                          d%'                                                              
"#;
