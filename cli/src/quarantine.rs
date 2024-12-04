use clap::Args;
use std::time::SystemTime;

use crate::junit_utils::junit_require;
#[cfg(target_os = "macos")]
use crate::junit_utils::junitify_xcresult;
use bundle::{QuarantineBulkTestStatus, QuarantineRunResult};
use codeowners::CodeOwners;
use constants::{EXIT_FAILURE, EXIT_SUCCESS};
use context::repo::BundleRepo;

use crate::{
    api_client::ApiClient,
    runner::{build_filesets, extract_failed_tests, run_quarantine_upload},
};

#[derive(Args, Clone, Debug)]
pub struct QuarantineArgs {
    #[arg(
        long,
        required_unless_present = junit_require(),
        value_delimiter = ',',
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        help = "Comma-separated list of glob paths to junit files."
    )]
    pub junit_paths: Vec<String>,
    #[cfg(target_os = "macos")]
    #[arg(long, required = false, help = "Path of xcresult directory")]
    pub xcresult_path: Option<String>,
    #[arg(long, help = "Organization url slug.")]
    pub org_url_slug: String,
    #[arg(
        long,
        required = true,
        env = "TRUNK_API_TOKEN",
        help = "Organization token. Defaults to TRUNK_API_TOKEN env var."
    )]
    pub token: String,
    #[arg(long, help = "Path to repository root. Defaults to current directory.")]
    pub repo_root: Option<String>,
    #[arg(long, help = "Value to override URL of repository.")]
    pub repo_url: Option<String>,
    #[arg(long, help = "Value to override SHA of repository head.")]
    pub repo_head_sha: Option<String>,
    #[arg(long, help = "Value to override branch of repository head.")]
    pub repo_head_branch: Option<String>,
    #[arg(long, help = "Value to override commit epoch of repository head.")]
    pub repo_head_commit_epoch: Option<String>,
    #[arg(long, help = "Value to tag team owner of upload.")]
    pub team: Option<String>,
    #[arg(long, help = "Value to override CODEOWNERS file or directory path.")]
    pub codeowners_path: Option<String>,
}

pub async fn run_quarantine(
    quarantine_args: QuarantineArgs,
    quarantine_results: Option<QuarantineRunResult>,
    codeowners: Option<CodeOwners>,
    exec_start: Option<SystemTime>,
) -> anyhow::Result<i32> {
    let QuarantineArgs {
        #[cfg(target_os = "macos")]
        mut junit_paths,
        #[cfg(target_os = "linux")]
        junit_paths,
        #[cfg(target_os = "macos")]
        xcresult_path,
        org_url_slug,
        token,
        repo_root,
        repo_url,
        repo_head_sha,
        repo_head_branch,
        repo_head_commit_epoch,
        team,
        codeowners_path,
    } = quarantine_args;

    let repo = BundleRepo::new(
        repo_root,
        repo_url,
        repo_head_sha,
        repo_head_branch,
        repo_head_commit_epoch,
    )?;

    let api_client = ApiClient::new(token)?;

    let codeowners =
        codeowners.or_else(|| CodeOwners::find_file(&repo.repo_root, &codeowners_path));

    #[cfg(target_os = "macos")]
    {
        let junitified = junitify_xcresult(
            xcresult_path,
            base_junit_paths,
            repo,
            org_url_slug,
            allow_empty_test_results,
        );
        if junitified.is_err() {
            return junitified;
        }
        junit_paths = junitified.unwrap_or(junit_paths);
    }

    let (file_sets, _file_counter) = build_filesets(
        &repo.repo_root,
        &junit_paths,
        team.clone(),
        &codeowners,
        exec_start,
    )?;

    let failures = extract_failed_tests(&repo, &org_url_slug, &file_sets).await;

    // Run the quarantine step and update the exit code.
    let exit_code = if failures.is_empty() {
        EXIT_SUCCESS
    } else {
        EXIT_FAILURE
    };

    let quarantine_run_results = if quarantine_results.is_none() {
        Some(
            run_quarantine_upload(
                &api_client,
                &api::GetQuarantineBulkTestStatusRequest {
                    repo: repo.repo.clone(),
                    org_url_slug: org_url_slug.clone(),
                },
                failures,
                exit_code,
            )
            .await,
        )
    } else {
        quarantine_results
    };

    let (exit_code, _resolved_quarantine_results) = if let Some(r) = quarantine_run_results.as_ref()
    {
        (r.exit_code, r.quarantine_status.clone())
    } else {
        (
            EXIT_SUCCESS,
            QuarantineBulkTestStatus {
                group_is_quarantined: false,
                quarantine_results: Vec::new(),
            },
        )
    };

    Ok(exit_code)
}
