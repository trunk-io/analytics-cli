use std::{fs, io, path::PathBuf, time::Duration};

use clap::Parser;
use context::repo::RepoUrlParts;
use tracing_subscriber::prelude::*;
use xcresult::{test_locations::Limits, xcresult::XCResult};

#[derive(Debug, Parser)]
pub struct Cli {
    /// Organization URL slug
    #[arg(long)]
    pub org_url_slug: Option<String>,
    /// Repository URL, e.g. `https://github.com/trunk-io/analytics-cli`
    #[arg(long)]
    pub repo_url: Option<String>,
    /// `.xcresult` directory to parse
    #[arg(required = true)]
    pub xcresult: String,
    /// JUnit XML output file path, defaults to stdout
    #[arg(long)]
    pub output_file_path: Option<PathBuf>,
    #[arg(long, required = false)]
    pub use_experimental_failure_summary: bool,
    #[arg(
        long,
        env = constants::TRUNK_USE_EXPERIMENTAL_XCRESULT_TEST_LOCATIONS_ENV,
        help = "Take each test's file from where a language server says it is declared in --repo-root, rather than from the failure that surfaced it. Reads the bundle with no legacy `xcresulttool get object` calls.",
        action = clap::ArgAction::Set,
        required = false,
        require_equals = true,
        num_args = 0..=1,
        default_value = "false",
        default_missing_value = "true",
        conflicts_with = "use_experimental_failure_summary"
    )]
    pub use_experimental_xcresult_test_locations: bool,
    #[arg(
        long,
        help = "Checkout to resolve test declarations in, defaults to the working directory.",
        requires = "use_experimental_xcresult_test_locations"
    )]
    pub repo_root: Option<PathBuf>,
    /// Most source files to parse. Ranked so the likeliest declarations come first, and
    /// parsing stops early once every test resolves.
    #[arg(
        long,
        env = constants::TRUNK_XCRESULT_TEST_LOCATIONS_MAX_FILES_ENV,
        default_value_t = Limits::default().max_files,
    )]
    pub xcresult_test_locations_max_files: usize,
    /// Seconds to spend per language server. The clang server answers far slower per file
    /// than the Swift one, so an Objective-C heavy repo wants this raised.
    #[arg(
        long,
        env = constants::TRUNK_XCRESULT_TEST_LOCATIONS_BUDGET_SECS_ENV,
        default_value_t = Limits::default().budget.as_secs(),
    )]
    pub xcresult_test_locations_budget_secs: u64,
    /// Seconds to wait for a single language server reply before giving up on it.
    #[arg(
        long,
        env = constants::TRUNK_XCRESULT_TEST_LOCATIONS_REQUEST_TIMEOUT_SECS_ENV,
        default_value_t = Limits::default().request_timeout.as_secs(),
    )]
    pub xcresult_test_locations_request_timeout_secs: u64,
    /// How many times to replace a language server that stops answering with a fresh one.
    #[arg(
        long,
        env = constants::TRUNK_XCRESULT_TEST_LOCATIONS_RETRIES_ENV,
        default_value_t = Limits::default().retries,
    )]
    pub xcresult_test_locations_retries: usize,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .with(tracing::metadata::LevelFilter::INFO)
        .init();
    let Cli {
        xcresult: path,
        org_url_slug,
        repo_url,
        output_file_path,
        use_experimental_failure_summary,
        use_experimental_xcresult_test_locations,
        repo_root,
        xcresult_test_locations_max_files,
        xcresult_test_locations_budget_secs,
        xcresult_test_locations_request_timeout_secs,
        xcresult_test_locations_retries,
    } = Cli::parse();
    let repo_url_parts = repo_url
        .and_then(|repo_url| RepoUrlParts::from_url(&repo_url).ok())
        .unwrap_or_default();
    let org_url_slug = org_url_slug.unwrap_or_default();
    let repo_full_name = repo_url_parts.repo_full_name();
    let xcresult = if use_experimental_xcresult_test_locations {
        XCResult::new_with_declaration_locations(
            path,
            org_url_slug,
            repo_full_name,
            repo_root.unwrap_or_else(|| PathBuf::from(".")),
            Limits {
                max_files: xcresult_test_locations_max_files,
                budget: Duration::from_secs(xcresult_test_locations_budget_secs),
                request_timeout: Duration::from_secs(xcresult_test_locations_request_timeout_secs),
                retries: xcresult_test_locations_retries,
            },
        )?
    } else {
        XCResult::new(
            path,
            org_url_slug,
            repo_full_name,
            use_experimental_failure_summary,
        )?
    };
    let mut junits = xcresult.generate_junits();
    let junit_count_and_first_junit = (junits.len(), junits.pop());
    let junit = if let (1, Some(junit)) = junit_count_and_first_junit {
        junit
    } else {
        return Err(anyhow::anyhow!(
            "Expected 1 JUnit report, found {}",
            junit_count_and_first_junit.0
        ));
    };
    let writer: Box<dyn io::Write> = if let Some(f) = output_file_path {
        Box::new(fs::File::create(f)?)
    } else {
        Box::new(io::stdout())
    };
    junit.serialize(writer)?;
    Ok(())
}
