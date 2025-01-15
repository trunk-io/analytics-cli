use api::BundleUploadStatus;
use bundle::{BundleMeta, BundlerUtil};
use clap::{ArgAction, Args};
use constants::EXIT_SUCCESS;
use context::bazel_bep::parser::BepParseResult;

use crate::{
    api_client::ApiClient,
    context::{
        gather_exit_code_and_quarantined_tests_context, gather_post_test_context,
        gather_pre_test_context, gather_upload_id_context, PreTestContext,
    },
    test::TestRunResult,
};

#[cfg(target_os = "macos")]
const JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG: &str = "xcresult_path";
#[cfg(not(target_os = "macos"))]
const JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG: &str = "junit_paths";

#[derive(Args, Clone, Debug, Default)]
pub struct UploadArgs {
    #[arg(
        long,
        required_unless_present_any = [JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG, "bazel_bep_path"],
        conflicts_with = "bazel_bep_path",
        value_delimiter = ',',
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        help = "Comma-separated list of glob paths to junit files."
    )]
    pub junit_paths: Vec<String>,
    #[arg(
        long,
        required_unless_present_any = [JUNIT_GLOB_REQUIRED_UNLESS_PRESENT_ARG, "junit_paths"],
        help = "Path to bazel build event protocol JSON file."
    )]
    pub bazel_bep_path: Option<String>,
    #[cfg(target_os = "macos")]
    #[arg(long,
        required_unless_present_any = ["junit_paths", "bazel_bep_path"],
        conflicts_with_all = ["junit_paths", "bazel_bep_path"],
        required = false, help = "Path of xcresult directory"
    )]
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
    #[arg(
        long,
        value_delimiter = ',',
        help = "Comma separated list of custom tag=value pairs."
    )]
    pub tags: Vec<String>,
    #[arg(long, help = "Print files which will be uploaded to stdout.")]
    pub print_files: bool,
    #[arg(long, help = "Run metrics CLI without uploading to API.")]
    pub dry_run: bool,
    #[arg(long, help = "Value to tag team owner of upload.")]
    pub team: Option<String>,
    #[arg(long, help = "Value to override CODEOWNERS file or directory path.")]
    pub codeowners_path: Option<String>,
    #[arg(
        long,
        help = "Run commands with the quarantining step.",
        action = ArgAction::Set,
        required = false,
        require_equals = true,
        num_args = 0..=1,
        default_value = "true",
        default_missing_value = "true",
    )]
    pub use_quarantining: bool,
    #[arg(
        long,
        alias = "allow-missing-junit-files",
        help = "Do not fail if test results are not found.",
        action = ArgAction::Set,
        required = false,
        require_equals = true,
        num_args = 0..=1,
        default_value = "true",
        default_missing_value = "true",
    )]
    pub allow_empty_test_results: bool,
}

impl UploadArgs {
    pub fn new(
        token: String,
        org_url_slug: String,
        junit_paths: Vec<String>,
        repo_root: String,
    ) -> Self {
        Self {
            junit_paths,
            org_url_slug,
            token,
            repo_root: Some(repo_root),
            allow_empty_test_results: true,
            ..Default::default()
        }
    }
}

pub async fn run_upload(
    upload_args: UploadArgs,
    pre_test_context: Option<PreTestContext>,
    test_run_result: Option<TestRunResult>,
) -> anyhow::Result<i32> {
    let api_client = ApiClient::new(&upload_args.token)?;

    let PreTestContext {
        mut meta,
        junit_path_wrappers,
        bep_result,
        // directory is removed on drop
        junit_path_wrappers_temp_dir: _junit_path_wrappers_temp_dir,
    } = if let Some(pre_test_context) = pre_test_context {
        pre_test_context
    } else {
        gather_pre_test_context(upload_args.clone())?
    };

    let file_set_builder = gather_post_test_context(
        &mut meta,
        junit_path_wrappers,
        &upload_args.team,
        &upload_args.codeowners_path,
        upload_args.allow_empty_test_results,
        &test_run_result,
    )?;

    if upload_args.print_files {
        println!("Files to upload:");
        for file_set in &meta.base_props.file_sets {
            println!(
                "  File set ({:?}): {}",
                file_set.file_set_type, file_set.glob
            );
            for file in &file_set.files {
                println!("    {}", file.original_path);
            }
        }
    }

    let exit_code = gather_exit_code_and_quarantined_tests_context(
        &mut meta,
        upload_args.use_quarantining,
        &api_client,
        &file_set_builder,
        &test_run_result,
    )
    .await;

    upload_bundle(
        meta,
        &api_client,
        bep_result,
        upload_args.dry_run,
        exit_code,
    )
    .await?;

    Ok(exit_code)
}

async fn upload_bundle(
    mut meta: BundleMeta,
    api_client: &ApiClient,
    bep_result: Option<BepParseResult>,
    dry_run: bool,
    exit_code: i32,
) -> anyhow::Result<()> {
    api_client
        .create_trunk_repo(&api::CreateRepoRequest {
            repo: meta.base_props.repo.repo.clone(),
            org_url_slug: meta.base_props.org.clone(),
            remote_urls: vec![meta.base_props.repo.repo_url.clone()],
        })
        .await?;

    let upload = gather_upload_id_context(&mut meta, api_client).await?;

    let (
        bundle_temp_file,
        // directory is removed on drop
        _bundle_temp_dir,
    ) = BundlerUtil::new(meta, bep_result).make_tarball_in_temp_dir()?;
    log::info!("Flushed temporary tarball to {:?}", bundle_temp_file);

    if dry_run {
        if let Err(e) = api_client
            .update_bundle_upload_status(&api::UpdateBundleUploadRequest {
                id: upload.id.clone(),
                upload_status: BundleUploadStatus::DryRun,
            })
            .await
        {
            log::warn!("{}", e);
        } else {
            log::debug!("Updated bundle upload status to DRY_RUN");
        }
        log::info!("Dry run, skipping upload.");
    } else {
        api_client
            .put_bundle_to_s3(&upload.url, &bundle_temp_file)
            .await?;

        if let Err(e) = api_client
            .update_bundle_upload_status(&api::UpdateBundleUploadRequest {
                id: upload.id.clone(),
                upload_status: BundleUploadStatus::UploadComplete,
            })
            .await
        {
            log::warn!("{}", e)
        } else {
            log::debug!(
                "Updated bundle upload status to {:#?}",
                BundleUploadStatus::UploadComplete
            )
        }

        if exit_code == EXIT_SUCCESS {
            log::info!("Done");
        } else {
            log::info!(
                "Upload successful; returning unsuccessful exit code of test run: {}",
                exit_code
            )
        }
    }

    Ok(())
}
