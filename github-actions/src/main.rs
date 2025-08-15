use clap::Parser;

#[derive(Parser)]
#[command(
    name = "github-actions",
    about = "Extract external ID from GitHub Actions worker log files",
    version,
    long_about = "This tool extracts the external ID from GitHub Actions worker log files. It should be run inside a GitHub Actions environment."
)]
struct Cli {
    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize tracing subscriber with appropriate log level
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::WARN)
            .init();
    }

    // Extract GitHub Actions external ID
    match github_actions::extract_github_external_id()? {
        Some(external_id) => {
            tracing::info!("Extracted GitHub Actions external ID: {}", external_id);
            std::process::exit(0);
        }
        None => {
            tracing::warn!("No GitHub Actions external ID found");
            std::process::exit(1);
        }
    }
}
