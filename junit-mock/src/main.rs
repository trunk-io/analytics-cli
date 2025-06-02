use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use junit_mock::JunitMock;

#[derive(Debug, Parser)]
pub struct Cli {
    /// Directory to output JUnit XML files
    #[arg(required = true)]
    pub directory: PathBuf,

    #[command(flatten)]
    pub options: junit_mock::Options,
}

fn main() -> Result<()> {
    let Cli { directory, options } = Cli::try_parse()?;

    let mut jm = JunitMock::new(options);
    println!("Using seed `{}` to generate random data.", jm.get_seed());

    let tmp_dir: Option<String> = None;
    let reports = jm.generate_reports(&tmp_dir);

    jm.write_reports_to_file(directory, &reports)?;

    Ok(())
}
