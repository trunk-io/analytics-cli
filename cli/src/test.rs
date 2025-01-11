use std::{
    process::{Command, Stdio},
    time::SystemTime,
};

use bundle::{FileSetBuilder, RunResult};
use clap::Args;
use codeowners::CodeOwners;
use constants::EXIT_FAILURE;
use context::{
    bazel_bep::parser::BazelBepParser, junit::junit_path::JunitReportFileWithStatus,
    repo::BundleRepo,
};

use crate::{
    api_client::ApiClient,
    print::print_bep_results,
    quarantine::{run_quarantine, FailedTestsExtractor},
    upload::{run_upload, UploadArgs},
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

pub async fn run_test(test_args: TestArgs) -> anyhow::Result<i32> {
    let TestArgs {
        command,
        upload_args,
    } = test_args;
    let UploadArgs {
        junit_paths,
        bazel_bep_path,
        org_url_slug,
        token,
        repo_root,
        repo_url,
        repo_head_sha,
        repo_head_branch,
        repo_head_commit_epoch,
        use_quarantining,
        team,
        codeowners_path,
        ..
    } = &upload_args;

    let repo = BundleRepo::new(
        repo_root.clone(),
        repo_url.clone(),
        repo_head_sha.clone(),
        repo_head_branch.clone(),
        repo_head_commit_epoch.clone(),
    )?;

    if junit_paths.is_empty() && bazel_bep_path.is_none() {
        return Err(anyhow::anyhow!("No junit paths provided."));
    }

    let api_client = ApiClient::new(String::from(token))?;

    let codeowners = CodeOwners::find_file(&repo.repo_root, codeowners_path);
    let junit_spec = if !junit_paths.is_empty() {
        JunitSpec::Paths(junit_paths.clone())
    } else {
        JunitSpec::BazelBep(bazel_bep_path.as_deref().unwrap_or_default().to_string())
    };

    log::info!("running command: {:?}", command);
    let run_result = run_test_command(
        &repo,
        command.first().unwrap(),
        command.iter().skip(1).collect(),
        junit_spec,
        team.clone(),
        &codeowners,
    )
    .await
    .unwrap_or_else(|e| {
        log::error!("Test command failed to run: {}", e);
        RunResult {
            exit_code: EXIT_FAILURE,
            ..Default::default()
        }
    });

    let run_exit_code = run_result.exit_code;
    let failed_tests_extractor = FailedTestsExtractor::new(
        &repo.repo,
        org_url_slug,
        run_result.file_set_builder.file_sets(),
    );

    let quarantine_run_result = if *use_quarantining {
        Some(
            run_quarantine(
                &api_client,
                &api::GetQuarantineBulkTestStatusRequest {
                    repo: repo.repo,
                    org_url_slug: org_url_slug.clone(),
                    test_identifiers: failed_tests_extractor.failed_tests().to_vec(),
                },
                &run_result.file_set_builder,
                Some(failed_tests_extractor),
                Some(run_exit_code),
            )
            .await,
        )
    } else {
        None
    };

    let exit_code = quarantine_run_result
        .as_ref()
        .map(|r| r.exit_code)
        .unwrap_or(run_exit_code);

    let exec_start = run_result.exec_start;
    if let Err(e) = run_upload(
        upload_args,
        Some(command.join(" ")),
        None, // don't re-run quarantine checks
        codeowners,
        exec_start,
    )
    .await
    {
        log::error!("Error uploading test results: {:?}", e)
    };

    Ok(exit_code)
}

pub enum JunitSpec {
    Paths(Vec<String>),
    BazelBep(String),
}

pub async fn run_test_command(
    repo: &BundleRepo,
    command: &String,
    args: Vec<&String>,
    junit_spec: JunitSpec,
    team: Option<String>,
    codeowners: &Option<CodeOwners>,
) -> anyhow::Result<RunResult> {
    let start = SystemTime::now();
    let exit_code = run_test_and_get_exit_code(command, args)?;
    log::info!("Command exit code: {}", exit_code);

    let output_paths = match junit_spec {
        JunitSpec::Paths(paths) => paths
            .into_iter()
            .map(JunitReportFileWithStatus::from)
            .collect(),
        JunitSpec::BazelBep(bep_path) => {
            let mut parser = BazelBepParser::new(bep_path);
            let bep_result = parser.parse()?;
            print_bep_results(&bep_result);
            bep_result.uncached_xml_files()
        }
    };

    let file_set_builder = FileSetBuilder::build_file_sets(
        &repo.repo_root,
        &output_paths,
        team,
        codeowners,
        Some(start),
    )?;

    Ok(RunResult {
        exit_code,
        file_set_builder,
        exec_start: Some(start),
    })
}

fn run_test_and_get_exit_code(command: &String, args: Vec<&String>) -> anyhow::Result<i32> {
    let mut child = Command::new(command)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    let result = child
        .wait()
        .map_or_else(
            |e| {
                log::error!("Error waiting for execution: {}", e);
                None
            },
            |exit_status| exit_status.code(),
        )
        .unwrap_or(EXIT_FAILURE);

    Ok(result)
}
