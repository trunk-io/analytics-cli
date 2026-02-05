use std::{fs, io::BufReader, path::PathBuf};

use chrono::{DateTime, FixedOffset, Utc};
use clap::{Parser, Subcommand};
use context::junit;

#[derive(Debug, Parser)]
#[command(version, about = "Utilities for working with context data.")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Subcommand)]
enum Command {
    Junit {
        /// JUnit XML file to parse and validate
        junit_xml_file: PathBuf,
        /// Reference timestamp for validation
        #[arg(long)]
        reference_timestamp: Option<DateTime<FixedOffset>>,
    },
}

fn main() -> anyhow::Result<()> {
    let Cli { command } = Cli::parse();
    match command {
        Command::Junit {
            junit_xml_file,
            reference_timestamp,
        } => {
            let file = fs::File::open(junit_xml_file)?;

            let mut junit_parser = junit::parser::JunitParser::new();
            junit_parser.parse(BufReader::new(file))?;

            let mut test_reports = junit_parser.into_reports();
            if test_reports.len() > 1 {
                return Err(anyhow::anyhow!("Multiple test reports found"));
            }

            let test_report = test_reports
                .pop()
                .ok_or_else(|| anyhow::anyhow!("No test report found"))?;
            {
                println!("Test report:");
                println!("{:#?}", test_report);
            }

            let validation = junit::validator::validate(
                &test_report.into(),
                &None,
                reference_timestamp.unwrap_or_else(|| Utc::now().fixed_offset()),
            );
            {
                println!("Validation:");
                println!("{:#?}", validation);
            }
        }
    }
    Ok(())
}
