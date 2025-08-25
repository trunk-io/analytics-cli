use std::{fs, path::PathBuf};

use bundle::bin_parse;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    version,
    about = "A tool to convert bundle proto bins into JSON for debugging."
)]
pub struct Cli {
    /// Protobuf bin file to convert into JSON
    #[arg(required = true)]
    pub proto_bin_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let Cli {
        proto_bin_file: path,
    } = Cli::parse();
    let bin = fs::read(path)?;
    let test_report = bin_parse(&bin)?;
    let json = serde_json::to_string_pretty(&test_report)?;
    println!("{}", json);
    Ok(())
}
