use std::{
    process::{Command, Stdio},
    time::SystemTime,
};

use clap::Args;
use constants::EXIT_FAILURE;

use crate::{
    context::gather_pre_test_context,
    upload_command::{run_upload, UploadArgs},
};

#[derive(Args, Clone, Debug)]
pub struct TestArgs {
    #[command(flatten)]
    upload_args: UploadArgs,
    #[arg(
        required = true,
        allow_hyphen_values = true,
        trailing_var_arg = true,
        help = "Test command to invoke."
    )]
    command: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TestRunResult {
    pub command: String,
    pub exec_start: SystemTime,
    pub exit_code: i32,
}

pub async fn run_test(
    TestArgs {
        upload_args,
        command,
    }: TestArgs,
) -> anyhow::Result<i32> {
    let pre_test_context = gather_pre_test_context(upload_args.clone())?;

    log::info!("running command: {:?}", command);
    let test_run_result = run_test_command(&command).await?;
    let test_run_result_exit_code = test_run_result.exit_code;

    let upload_run_result =
        run_upload(upload_args, Some(pre_test_context), Some(test_run_result)).await;

    upload_run_result.or_else(|e| {
        log::error!("Error uploading test results: {:?}", e);
        Ok(test_run_result_exit_code)
    })
}

pub async fn run_test_command<T: AsRef<str>>(command: &[T]) -> anyhow::Result<TestRunResult> {
    let exec_start = SystemTime::now();
    let mut child = Command::new(command.first().map(|s| s.as_ref()).unwrap_or_default())
        .args(
            command
                .iter()
                .skip(1)
                .map(|s| s.as_ref())
                .collect::<Vec<_>>(),
        )
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;
    let exit_code = child
        .wait()
        .map_or_else(
            |e| {
                log::error!("Error waiting for execution: {}", e);
                None
            },
            |exit_status| exit_status.code(),
        )
        .unwrap_or(EXIT_FAILURE);
    log::info!("Command exit code: {}", exit_code);

    Ok(TestRunResult {
        exit_code,
        exec_start,
        command: command
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>()
            .join(" "),
    })
}