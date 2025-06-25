use std::{
    env,
    process::{Command, Stdio},
    sync::{mpsc::Sender, Arc},
    time::SystemTime,
};

use clap::Args;
use constants::EXIT_FAILURE;
use display::{
    end_output::EndOutput,
    message::{send_message, DisplayMessage},
};
use superconsole::{
    style::{Attribute, Stylize},
    Line, Span,
};

use crate::{
    context::{gather_debug_props, gather_initial_test_context},
    upload_command::{run_upload, UploadArgs, UploadRunResult},
};

enum RunOutput {
    Title,
}
impl EndOutput for RunOutput {
    fn output(&self) -> anyhow::Result<Vec<Line>> {
        match self {
            RunOutput::Title => Ok(vec![
                Line::from_iter([Span::new_styled(
                    String::from("📒 Test command outputs").attribute(Attribute::Bold),
                )?]),
                Line::default(),
            ]),
        }
    }
}

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
    pub command_stdout: String,
    pub command_stderr: String,
}

fn lines_of(output: String) -> impl Iterator<Item = String> {
    output
        .lines()
        .flat_map(|lf_line| {
            lf_line
                .split('\r')
                .map(|crlf_line| crlf_line.replace('\t', "    "))
        })
        .collect::<Vec<String>>()
        .into_iter()
}

impl EndOutput for TestRunResult {
    fn output(&self) -> anyhow::Result<Vec<Line>> {
        let mut output: Vec<Line> = Vec::new();

        output.extend(vec![
            Line::from_iter([Span::new_styled(
                String::from("📒 Test command outputs").attribute(Attribute::Bold),
            )?]),
            Line::default(),
        ]);

        for stderr_line in lines_of(self.command_stderr.clone()) {
            output.push(Line::from_iter([Span::new_unstyled_lossy(stderr_line)]));
        }

        output.push(Line::default());

        for stdout_line in lines_of(self.command_stdout.clone()) {
            output.push(Line::from_iter([Span::new_unstyled_lossy(stdout_line)]));
        }

        Ok(output)
    }
}

pub async fn run_test(
    TestArgs {
        upload_args,
        command,
    }: TestArgs,
    render_sender: Sender<DisplayMessage>,
) -> anyhow::Result<UploadRunResult> {
    let token = upload_args.token.clone();
    let mut test_run_result = run_test_command(&command, render_sender.clone()).await?;
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
        None,
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

pub async fn run_test_command<T: AsRef<str>>(
    command: &[T],
    render_sender: Sender<DisplayMessage>,
) -> anyhow::Result<TestRunResult> {
    let exec_start = SystemTime::now();
    let title_ptr = Arc::new(RunOutput::Title);
    send_message(
        DisplayMessage::Final(title_ptr, String::from("test command title")),
        &render_sender,
    );
    let child = Command::new(command.first().map(|s| s.as_ref()).unwrap_or_default())
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
    let exit_result = child.wait_with_output().map_err(|e| {
        tracing::warn!("Error waiting for execution: {}", e);
        e
    });
    let (exit_code, stdout, stderr) = exit_result
        .map(|result| {
            (
                result.status.code().unwrap_or(EXIT_FAILURE),
                String::from_utf8(result.stdout)
                    .unwrap_or_else(|_| String::from("Error: Command had non-utf-8 output!")),
                String::from_utf8(result.stderr)
                    .unwrap_or_else(|_| String::from("Error: Command had non-utf-8 error output!")),
            )
        })
        .unwrap_or((EXIT_FAILURE, String::from(""), String::from("")));
    tracing::info!("Command exit code: {}", exit_code);

    Ok(TestRunResult {
        exit_code,
        exec_start: Some(exec_start),
        command: command
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>()
            .join(" "),
        command_stdout: stdout,
        command_stderr: stderr,
    })
}
