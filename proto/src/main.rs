use std::{fs, path::PathBuf};

use clap::Parser;
use prost::Message;
use proto::test_context::test_run::{TestReport, TestResult};

#[derive(Debug, Parser)]
pub struct Cli {
    /// Protobuf bin file to parse
    #[arg(required = true)]
    pub proto_bin_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let Cli {
        proto_bin_file: path,
    } = Cli::parse();
    let bin = fs::read(&path)?;
    if let Ok(test_report) = TestReport::decode(bin.as_slice()) {
        println!("{:#?}", test_report);
    } else {
        let test_result = TestResult::decode(bin.as_slice())
            .map_err(|err| anyhow::anyhow!("Failed to decode {:#?}: {}", &path, err))?;
        println!("{:#?}", test_result);
    }
    Ok(())
}
