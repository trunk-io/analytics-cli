use std::{
    env,
    process::{Command, Stdio},
    time::SystemTime,
};

use clap::Args;
use constants::EXIT_FAILURE;

use crate::{
    context::{gather_debug_props, gather_initial_test_context},
    upload_command::{run_upload, UploadArgs, UploadRunResult},
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

impl TestArgs {
    pub fn token(&self) -> String {
        self.upload_args.token.clone()
    }

    pub fn org_url_slug(&self) -> String {
        self.upload_args.org_url_slug.clone()
    }

    pub fn repo_root(&self) -> Option<String> {
        self.upload_args.repo_root.clone()
    }
}

#[derive(Debug, Clone)]
pub struct TestRunResult {
    pub command: String,
    pub exec_start: Option<SystemTime>,
    pub exit_code: i32,
}

pub async fn run_test(
    TestArgs {
        upload_args,
        command,
    }: TestArgs,
) -> anyhow::Result<UploadRunResult> {
    let token = upload_args.token.clone();
    let mut test_run_result = run_test_command(&command).await?;
    let test_context = gather_initial_test_context(
        upload_args.clone(),
        gather_debug_props(env::args().collect::<Vec<String>>(), token),
    )?;

    let test_run_result_exit_code = test_run_result.exit_code;
    // remove exec start because it filters out test files and we want to
    // trust bazel-bep to provide the required test files
    if upload_args.bazel_bep_path.is_some() {
        test_run_result.exec_start = None;
    }

    let upload_run_result = run_upload(
        upload_args,
        Some(test_context),
        Some(test_run_result.clone()),
    )
    .await;
    match upload_run_result {
        Ok(upload_run_result) => {
            tracing::info!(
                "Test command '{}' executed with exit code {}",
                test_run_result.command,
                test_run_result_exit_code.to_string()
            );
            Ok(upload_run_result)
        }
        Err(e) => Err(e),
    }
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
                tracing::warn!("Error waiting for execution: {}", e);
                None
            },
            |exit_status| exit_status.code(),
        )
        .unwrap_or(EXIT_FAILURE);
    tracing::info!("Command exit code: {}", exit_code);

    Ok(TestRunResult {
        exit_code,
        exec_start: Some(exec_start),
        command: command
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>()
            .join(" "),
    })
}
