use std::{fs::File, path::PathBuf, time::Instant};

use clap::{Parser, ValueEnum};
use codeowners::{FromReader, GitHubOwners, GitLabOwners, Owners, associate_codeowners};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
#[value(rename_all = "lower")]
pub enum CodeownersType {
    GitHub,
    GitLab,
}

#[derive(Debug, Parser)]
pub struct Cli {
    /// How to parse CODEOWNERS
    #[arg(long)]
    pub codeowners_type: CodeownersType,
    /// Path to CODEOWNERS file to parse
    #[arg(long)]
    pub codeowners_path: PathBuf,
    /// Test case path to check against CODEOWNERS
    #[arg(long)]
    pub test_case_path: String,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let Cli {
        codeowners_type,
        codeowners_path,
        test_case_path,
    } = Cli::parse();

    let parse_started_at = Instant::now();
    let owners = match codeowners_type {
        CodeownersType::GitHub => File::open(&codeowners_path)
            .map_err(anyhow::Error::from)
            .and_then(|file| GitHubOwners::from_reader(&file).map(Owners::GitHubOwners))?,
        CodeownersType::GitLab => File::open(&codeowners_path)
            .map_err(anyhow::Error::from)
            .and_then(|file| GitLabOwners::from_reader(&file).map(Owners::GitLabOwners))?,
    };
    tracing::info!(
        parse_ms = parse_started_at.elapsed().as_millis(),
        codeowners_type = ?codeowners_type,
        "Completed CODEOWNERS parse"
    );

    let associated_owners = associate_codeowners(&owners, &test_case_path);
    tracing::info!(
        owner_count = associated_owners.len(),
        test_case_path = %test_case_path,
        "Completed CODEOWNERS association"
    );

    if associated_owners.is_empty() {
        eprintln!("No owners found for {}", test_case_path);
        std::process::exit(1);
    } else {
        println!("Owners found for {}:", test_case_path);
        for owner in associated_owners {
            println!("{}", owner);
        }
    }

    Ok(())
}
