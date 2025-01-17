use clap::Parser;
use context::repo::RepoUrlParts;
use std::{fs, io, path::PathBuf};
use xcresult::XCResult;

#[derive(Debug, Parser)]
pub struct Cli {
    /// Repository URL
    #[arg(long)]
    pub repo_url: Option<String>,
    /// Organization URL slug
    #[arg(long)]
    pub org_url_slug: Option<String>,
    /// `.xcresult` directory to parse
    #[arg(required = true)]
    pub xcresult: String,
    /// JUnit XML output file path, defaults to stdout
    #[arg(long)]
    pub output_file_path: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let Cli {
        xcresult: path,
        repo_url,
        org_url_slug,
        output_file_path,
    } = Cli::parse();
    let repo_url_parts = repo_url
        .and_then(|repo_url| RepoUrlParts::from_url(&repo_url).ok())
        .unwrap_or_default();
    let xcresult = XCResult::new(path, &repo_url_parts, org_url_slug.unwrap_or_default())?;
    let mut junits = xcresult.generate_junits()?;
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
