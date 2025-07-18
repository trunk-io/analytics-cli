use std::{
    env,
    sync::{mpsc::Sender, Arc},
    thread::JoinHandle,
};

use clap::{Parser, Subcommand};
use clap_verbosity_flag::{log::LevelFilter, InfoLevel, Verbosity};
use display::{
    message::{send_message, DisplayMessage},
    render::spin_up_renderer,
};
use sentry::ClientInitGuard;
use tracing_subscriber::{filter::FilterFn, prelude::*};
use trunk_analytics_cli::{
    context::gather_debug_props,
    error_report::ErrorReport,
    test_command::{run_test, TestArgs},
    upload_command::{run_upload, UploadArgs, UploadRunResult},
    validate_command::{run_validate, ValidateArgs, ValidateRunResult},
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
            Commands::Test(args) => Some(args.token()),
            Commands::Upload(args) => Some(args.token.clone()),
            Commands::Validate(..) => None,
        };

        token.map_or(
            format!("{:?}", env::args().collect::<Vec<String>>()),
            |token| gather_debug_props(env::args().collect::<Vec<String>>(), token).command_line,
        )
    }

    pub fn command_name(&self) -> &str {
        match &self.command {
            Commands::Test(..) => "test",
            Commands::Upload(..) => "upload",
            Commands::Validate(..) => "validate",
        }
    }

    pub fn org_url_slug(&self) -> String {
        match &self.command {
            Commands::Test(args) => args.org_url_slug(),
            Commands::Upload(args) => args.org_url_slug.clone(),
            Commands::Validate(..) => String::from("not used"),
        }
    }

    pub fn repo_root(&self) -> String {
        let explicit_root = match &self.command {
            Commands::Test(args) => args.repo_root(),
            Commands::Upload(args) => args.repo_root.clone(),
            Commands::Validate(..) => None,
        };
        explicit_root
            .or(std::env::current_dir()
                .iter()
                .flat_map(|path_buf| path_buf.clone().into_os_string().into_string().into_iter())
                .next())
            .unwrap_or(String::from("not set"))
    }
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run a test command and upload data to Trunk Flaky Tests
    Test(TestArgs),
    /// Upload data to Trunk Flaky Tests
    Upload(UploadArgs),
    /// Validate that your test runner output is suitable for Trunk Flaky Tests (does not call to servers)
    Validate(ValidateArgs),
}

fn close_out_and_exit(
    exit_code: i32,
    guard: ClientInitGuard,
    render_sender: Sender<DisplayMessage>,
    render_handle: JoinHandle<()>,
) -> ! {
    guard.flush(None);
    drop(render_sender);
    let _ = render_handle.join();
    std::process::exit(exit_code)
}

// "the Sentry client must be initialized before starting an async runtime or spawning threads"
// https://docs.sentry.io/platforms/rust/#async-main-function
fn main() -> anyhow::Result<()> {
    let release_name = format!("analytics-cli@{}", env!("CARGO_PKG_VERSION"));
    let guard = third_party::sentry::init(release_name.into(), None);

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let cli = Cli::parse();
            let org_url_slug = cli.org_url_slug();
            let log_level_filter = cli.verbose.log_level_filter();
            setup_logger(
                log_level_filter,
                cli.command_name(),
                cli.org_url_slug(),
                cli.repo_root(),
            )?;

            tracing::info!(
                command = cli.debug_props(),
                "Trunk Flaky Test running command"
            );
            let (render_handle, render_sender) = spin_up_renderer();

            match run(cli, render_sender.clone()).await {
                Ok(RunResult::Upload(run_result)) => {
                    let result_ptr = Arc::new(run_result);
                    send_message(
                        DisplayMessage::Final(result_ptr.clone(), String::from("upload display")),
                        &render_sender,
                    );
                    let exit_code = result_ptr
                        .error_report
                        .as_ref()
                        .map(|e| e.context.exit_code)
                        .unwrap_or(result_ptr.quarantine_context.exit_code);
                    close_out_and_exit(exit_code, guard, render_sender, render_handle)
                }
                Ok(RunResult::Test(run_result)) => {
                    let result_ptr = Arc::new(run_result);
                    send_message(
                        DisplayMessage::Final(result_ptr.clone(), String::from("test display")),
                        &render_sender,
                    );
                    close_out_and_exit(
                        result_ptr.quarantine_context.exit_code,
                        guard,
                        render_sender,
                        render_handle,
                    )
                }
                Ok(RunResult::Validate(run_result)) => {
                    let result_ptr = Arc::new(run_result);
                    send_message(
                        DisplayMessage::Final(result_ptr.clone(), String::from("validate display")),
                        &render_sender,
                    );
                    close_out_and_exit(result_ptr.exit_code(), guard, render_sender, render_handle)
                }
                Err(error) => {
                    let error_report = ErrorReport::new(error, org_url_slug, None);
                    let exit_code = error_report.context.exit_code;
                    send_message(
                        DisplayMessage::Final(
                            Arc::new(error_report),
                            String::from("error display"),
                        ),
                        &render_sender,
                    );
                    close_out_and_exit(exit_code, guard, render_sender, render_handle)
                }
            }
        })
}

enum RunResult {
    Upload(UploadRunResult),
    Test(UploadRunResult),
    Validate(ValidateRunResult),
}

async fn run(cli: Cli, render_sender: Sender<DisplayMessage>) -> anyhow::Result<RunResult> {
    tracing::info!(
        "Starting trunk flakytests {} (git={}) rustc={}",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_SHA"),
        env!("VERGEN_RUSTC_SEMVER")
    );
    match cli.command {
        Commands::Upload(upload_args) => {
            let result = run_upload(upload_args, None, None, Some(render_sender)).await?;
            Ok(RunResult::Upload(result))
        }
        Commands::Test(test_args) => {
            let result = run_test(test_args, render_sender).await?;
            Ok(RunResult::Test(result))
        }
        Commands::Validate(validate_args) => {
            let result = run_validate(validate_args).await?;
            Ok(RunResult::Validate(result))
        }
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

fn setup_logger(
    log_level_filter: LevelFilter,
    command_name: &str,
    org_url_slug: String,
    repo_root: String,
) -> anyhow::Result<()> {
    let command_string = String::from(command_name);
    let sentry_layer = sentry_tracing::layer().event_mapper(move |event, context| {
        // trunk-ignore(clippy/match_ref_pats)
        match event.metadata().level() {
            &tracing::Level::ERROR => {
                let mut event = sentry_tracing::event_from_event(event, context);
                event
                    .tags
                    .insert(String::from("command_name"), command_string.clone());
                event
                    .tags
                    .insert(String::from("org_url_slug"), org_url_slug.clone());
                event
                    .tags
                    .insert(String::from("repo_root"), repo_root.clone());
                sentry_tracing::EventMapping::Event(event)
            }
            &tracing::Level::WARN => sentry_tracing::EventMapping::Breadcrumb(
                sentry_tracing::breadcrumb_from_event(event, context),
            ),
            &tracing::Level::INFO => sentry_tracing::EventMapping::Breadcrumb(
                sentry_tracing::breadcrumb_from_event(event, context),
            ),
            &tracing::Level::DEBUG => sentry_tracing::EventMapping::Breadcrumb(
                sentry_tracing::breadcrumb_from_event(event, context),
            ),
            _ => sentry_tracing::EventMapping::Ignore,
        }
    });

    // make console layer toggle based on vebosity
    let console_layer = tracing_subscriber::fmt::Layer::new()
        .with_target(true)
        .with_level(true)
        .with_writer(std::io::stdout.with_max_level(to_trace_filter(log_level_filter)))
        .with_filter(FilterFn::new(move |metadata| {
            !metadata
                .fields()
                .iter()
                .any(|field| field.name() == "hidden_in_console")
                && to_trace_filter(log_level_filter) > tracing::Level::INFO
        }));

    tracing_subscriber::registry()
        .with(console_layer)
        .with(sentry_layer)
        .init();
    Ok(())
}
