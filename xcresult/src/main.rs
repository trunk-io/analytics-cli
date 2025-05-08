use std::{fs, io, path::PathBuf};

use clap::Parser;
use context::repo::RepoUrlParts;
use tracing_subscriber::prelude::*;
use xcresult::xcresult::XCResult;

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
    } = Cli::parse();
    let repo_url_parts = repo_url
        .and_then(|repo_url| RepoUrlParts::from_url(&repo_url).ok())
        .unwrap_or_default();
    let xcresult = XCResult::new(
        path,
        org_url_slug.unwrap_or_default(),
        repo_url_parts.repo_full_name(),
        true,
    )?;
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
