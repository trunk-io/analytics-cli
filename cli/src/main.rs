mod shutdown;

use std::{
    env,
    path::{Path, PathBuf},
    sync::{Arc, mpsc::Sender},
    thread::JoinHandle,
};

use clap::{Args, Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity, log::LevelFilter};
use display::{
    message::{DisplayMessage, send_message},
    render::spin_up_renderer,
};
use sentry::ClientInitGuard;
use tracing_subscriber::{filter::FilterFn, prelude::*};
use trunk_analytics_cli::{
    context::gather_debug_props,
    error_report::ErrorReport,
    exit_code::ExitCode,
    summary_output::{
        ErrorSummary, UploadCommand, UploadReport, ValidateReport, github,
        json::{self, Report},
        text::{SummaryFileText, TextOptions, UploadText},
    },
    test_command::{TestArgs, run_test},
    upload_command::{RunUploadOptions, UploadArgs, UploadRunResult, run_upload},
    validate_command::{ValidateArgs, ValidateRunResult, run_validate},
};

#[derive(Debug, Parser)]
#[command(
    version = std::env!("CARGO_PKG_VERSION"),
    about = "Trunk Analytics CLI",
    after_long_help = ExitCode::help(),
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
            Commands::Test(args) => args.token(),
            Commands::Upload(args) => args.token.clone(),
            Commands::Validate(..)
            | Commands::SummaryOutputSchema
            | Commands::GithubAnnotations(..) => None,
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
            Commands::SummaryOutputSchema => "summary-output-schema",
            Commands::GithubAnnotations(..) => "github-annotations",
        }
    }

    /// The org slug is required on `upload` and `test` because it is what attributes a run to an
    /// organization in our telemetry: it tags every error forwarded to Sentry, is recorded on the
    /// bundle as `base_props.org`, and scopes the upload metrics we report for the run. Without it
    /// a run can't be tied back to a customer. `validate` runs locally, uploads nothing, and never
    /// accepts the flag, hence the placeholder.
    pub fn org_url_slug(&self) -> String {
        match &self.command {
            Commands::Test(args) => args.org_url_slug(),
            Commands::Upload(args) => args.org_url_slug.clone(),
            Commands::Validate(..)
            | Commands::SummaryOutputSchema
            | Commands::GithubAnnotations(..) => String::from("not used"),
        }
    }

    pub fn summary_output_file(&self) -> Option<PathBuf> {
        match &self.command {
            Commands::Test(args) => args.summary_output_file(),
            Commands::Upload(args) => args.summary_output_file.clone(),
            Commands::Validate(..)
            | Commands::SummaryOutputSchema
            | Commands::GithubAnnotations(..) => None,
        }
    }

    pub fn repo_root(&self) -> String {
        let explicit_root = match &self.command {
            Commands::Test(args) => args.repo_root(),
            Commands::Upload(args) => args.repo_root.clone(),
            Commands::Validate(..)
            | Commands::SummaryOutputSchema
            | Commands::GithubAnnotations(..) => None,
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
    /// Run a test command and upload results to Trunk Flaky Tests. Automatically detects and reports flaky tests.
    #[command(after_long_help = ExitCode::help())]
    Test(TestArgs),
    /// Upload test results to Trunk Flaky Tests. Use this when you've already run tests and have result files.
    #[command(after_long_help = ExitCode::help())]
    Upload(UploadArgs),
    /// Validate test report files for compatibility with Trunk Flaky Tests. Runs locally without uploading data.
    #[command(after_long_help = ExitCode::help())]
    Validate(ValidateArgs),
    /// Print the JSON Schema of the summary that `--summary-output-file` writes.
    SummaryOutputSchema,
    /// Print GitHub Actions annotations for why a run failed, from its `--summary-output-file`.
    /// Exits 0 only when it printed one. Used by the analytics-uploader action.
    #[command(hide = true)]
    GithubAnnotations(GithubAnnotationsArgs),
}

#[derive(Debug, Args)]
struct GithubAnnotationsArgs {
    /// The summary that `upload` or `test` wrote to `--summary-output-file`.
    #[arg(long)]
    summary_file: PathBuf,
}

fn close_out_and_exit(
    exit_code: ExitCode,
    guard: ClientInitGuard,
    render_sender: Sender<DisplayMessage>,
    render_handle: JoinHandle<()>,
) -> ! {
    guard.flush(None);
    drop(render_sender);
    let _ = render_handle.join();
    std::process::exit(exit_code.code())
}

// "the Sentry client must be initialized before starting an async runtime or spawning threads"
// https://docs.sentry.io/platforms/rust/#async-main-function
fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // Before Sentry and logging start, so stdout carries only the workflow commands.
    if let Commands::GithubAnnotations(args) = &cli.command {
        std::process::exit(github::run(&args.summary_file).code());
    }

    let release_name = format!("analytics-cli@{}", env!("CARGO_PKG_VERSION"));
    let guard = third_party::sentry::init(release_name.into(), None);

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let org_url_slug = cli.org_url_slug();
            let summary_output_file = cli.summary_output_file();
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

            // Race the work against termination signals so a CI cancellation aborts in-flight
            // operations immediately rather than waiting for slow retries/uploads to finish.
            let outcome = tokio::select! {
                biased;
                result = run(cli, render_sender.clone()) => MainOutcome::Finished(result),
                exit_code = shutdown::shutdown_signal() => MainOutcome::Interrupted(exit_code),
            };

            match outcome {
                MainOutcome::Interrupted(exit_code) => {
                    tracing::warn!(
                        "Received termination signal; aborting in-flight work and shutting down."
                    );
                    close_out_and_exit(exit_code, guard, render_sender, render_handle)
                }
                MainOutcome::Finished(Ok(RunResult::Upload(run_result))) => {
                    let text = Arc::new(UploadText {
                        report: UploadReport::new(&run_result, UploadCommand::Upload),
                        options: TextOptions::from(&run_result),
                    });
                    send_message(
                        DisplayMessage::Final(text.clone(), String::from("upload display")),
                        &render_sender,
                    );
                    write_summary_file(
                        summary_output_file.as_deref(),
                        Report::Upload(&text.report),
                        &render_sender,
                    );
                    close_out_and_exit(
                        text.report.cli_exit_code(),
                        guard,
                        render_sender,
                        render_handle,
                    )
                }
                MainOutcome::Finished(Ok(RunResult::Test(run_result))) => {
                    let text = Arc::new(UploadText {
                        report: UploadReport::new(&run_result, UploadCommand::Test),
                        options: TextOptions::from(&run_result),
                    });
                    send_message(
                        DisplayMessage::Final(text.clone(), String::from("test display")),
                        &render_sender,
                    );
                    write_summary_file(
                        summary_output_file.as_deref(),
                        Report::Upload(&text.report),
                        &render_sender,
                    );
                    close_out_and_exit(
                        text.report.cli_exit_code(),
                        guard,
                        render_sender,
                        render_handle,
                    )
                }
                MainOutcome::Finished(Ok(RunResult::Validate(run_result))) => {
                    let report = ValidateReport::new(&run_result);
                    let exit_code = report.exit_code;
                    send_message(
                        DisplayMessage::Final(Arc::new(report), String::from("validate display")),
                        &render_sender,
                    );
                    close_out_and_exit(exit_code, guard, render_sender, render_handle)
                }
                MainOutcome::Finished(Ok(RunResult::SummaryOutputSchema)) => {
                    println!("{}", serde_json::to_string_pretty(&json::Report::schema())?);
                    close_out_and_exit(ExitCode::Success, guard, render_sender, render_handle)
                }
                MainOutcome::Finished(Err(error)) => {
                    let error = Arc::new(ErrorSummary::new(&ErrorReport::new(
                        error,
                        org_url_slug,
                        None,
                    )));
                    let exit_code = error.exit_code.unwrap_or(ExitCode::Success);
                    send_message(
                        DisplayMessage::Final(error.clone(), String::from("error display")),
                        &render_sender,
                    );
                    write_summary_file(
                        summary_output_file.as_deref(),
                        Report::Error(&error),
                        &render_sender,
                    );
                    close_out_and_exit(exit_code, guard, render_sender, render_handle)
                }
            }
        })
}

fn write_summary_file(path: Option<&Path>, report: Report, render_sender: &Sender<DisplayMessage>) {
    let Some(path) = path else {
        return;
    };
    let text = SummaryFileText::new(path, report.write_file(path));
    send_message(
        DisplayMessage::Final(Arc::new(text), String::from("summary file display")),
        render_sender,
    );
}

/// Outcome of racing the command against termination signals in `main`.
enum MainOutcome {
    /// The command ran to completion (successfully or with an error).
    Finished(anyhow::Result<RunResult>),
    /// A termination signal was received first; the payload is the exit code to use.
    Interrupted(ExitCode),
}

enum RunResult {
    Upload(UploadRunResult),
    Test(UploadRunResult),
    Validate(ValidateRunResult),
    SummaryOutputSchema,
}

async fn run(cli: Cli, render_sender: Sender<DisplayMessage>) -> anyhow::Result<RunResult> {
    tracing::info!(
        "Starting trunk-analytics-cli {} (git={}) rustc={}",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_SHA"),
        env!("VERGEN_RUSTC_SEMVER")
    );
    match cli.command {
        Commands::Upload(upload_args) => {
            let result = run_upload(
                upload_args,
                RunUploadOptions {
                    render_sender: Some(render_sender),
                    ..Default::default()
                },
            )
            .await?;
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
        Commands::SummaryOutputSchema => Ok(RunResult::SummaryOutputSchema),
        Commands::GithubAnnotations(..) => {
            anyhow::bail!("github-annotations runs before the async runtime starts")
        }
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
    // logs go to stderr so stdout is left for the command's output
    let trace_filter = to_trace_filter(log_level_filter);
    let console_layer = tracing_subscriber::fmt::Layer::new()
        .with_target(true)
        .with_level(true)
        .with_writer(std::io::stderr)
        .with_filter(FilterFn::new(move |metadata| {
            !metadata
                .fields()
                .iter()
                .any(|field| field.name() == "hidden_in_console")
                && trace_filter > tracing::Level::INFO
                && *metadata.level() <= trace_filter
        }));

    tracing_subscriber::registry()
        .with(console_layer)
        .with(sentry_layer)
        .init();
    Ok(())
}
