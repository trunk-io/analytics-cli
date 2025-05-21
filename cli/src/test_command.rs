use std::{
    env,
    process::{Command, Stdio},
    time::SystemTime,
};

use clap::Args;
use constants::EXIT_FAILURE;
use serde_yaml;

use crate::{
    context::{gather_debug_props, gather_initial_test_context},
    error_report::{log_error, Context},
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
        if self.upload_args.token.is_none() {
            // read from  ~/.cache/trunk/user.yaml
            let dir = dirs::home_dir();
            if dir.is_none() {
                return "".to_string();
            }
            let dir = dir.unwrap();
            let mut path = dir;
            path.push(".cache");
            path.push("trunk");
            path.push("user.yaml");
            if let Ok(f) = std::fs::File::open(path) {
                let d: Result<serde_yaml::Value, serde_yaml::Error> = serde_yaml::from_reader(f);
                if d.is_err() {
                    return "".to_string();
                }
                return d.unwrap()["trunk_user"]["tokens"]["access_token"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
            }
        }
        self.upload_args.token.clone().unwrap()
    }

    pub fn org_url_slug(&self) -> String {
        self.upload_args.org_url_slug.clone()
    }

    pub fn repo_root(&self) -> Option<String> {
        self.upload_args.repo_root.clone()
    }

    pub fn hide_banner(&self) -> bool {
        self.upload_args.hide_banner
    }
}

#[derive(Debug, Clone)]
pub struct TestRunResult {
    pub command: String,
    pub exec_start: Option<SystemTime>,
    pub exit_code: i32,
}

pub async fn run_test(test_args: TestArgs) -> anyhow::Result<i32> {
    let token = test_args.token();
    let mut test_run_result = run_test_command(&test_args.command).await?;
    let test_context = gather_initial_test_context(
        test_args.upload_args.clone(),
        gather_debug_props(env::args().collect::<Vec<String>>(), token),
    )?;

    let test_run_result_exit_code = test_run_result.exit_code;
    // remove exec start because it filters out test files and we want to
    // trust bazel-bep to provide the required test files
    if test_args.upload_args.bazel_bep_path.is_some() {
        test_run_result.exec_start = None;
    }

    let org_url_slug = test_args.upload_args.org_url_slug.clone();
    let upload_run_result = run_upload(
        test_args.upload_args,
        Some(test_context),
        Some(test_run_result),
    )
    .await;

    upload_run_result
        .and_then(
            |UploadRunResult {
                 exit_code,
                 upload_bundle_error,
             }| {
                if let Some(e) = upload_bundle_error {
                    return Err(e);
                }
                Ok(exit_code)
            },
        )
        .or_else(|e| {
            println!("error: {}", e);
            log_error(
                &e,
                Context {
                    base_message: Some("Error uploading test results".into()),
                    org_url_slug,
                },
            );
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
