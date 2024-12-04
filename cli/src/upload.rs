use clap::Args;
use std::{
    env,
    io::BufReader,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::junit_utils::junit_require;
#[cfg(target_os = "macos")]
use crate::junit_utils::junitify_xcresult;
use api::BundleUploadStatus;
use bundle::{
    parse_custom_tags, BundleMeta, BundleMetaBaseProps, BundleMetaDebugProps, BundleMetaJunitProps,
    BundlerUtil, FileSet, QuarantineBulkTestStatus, QuarantineRunResult, META_VERSION,
};
use codeowners::CodeOwners;
use constants::{EXIT_FAILURE, EXIT_SUCCESS};
use context::{junit::parser::JunitParser, repo::BundleRepo};

use crate::{
    api_client::ApiClient,
    runner::{build_filesets, extract_failed_tests, run_quarantine_upload},
    scanner::EnvScanner,
};

#[derive(Args, Clone, Debug)]
pub struct UploadArgs {
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
        default_value = "true"
    )]
    pub use_quarantining: bool,
    #[arg(
        long,
        alias = "allow-missing-junit-files",
        help = "Do not fail if test results are not found.",
        default_value = "true"
    )]
    pub allow_empty_test_results: bool,
}

pub async fn run_upload(
    upload_args: UploadArgs,
    test_command: Option<String>,
    quarantine_results: Option<QuarantineRunResult>,
    codeowners: Option<CodeOwners>,
    exec_start: Option<SystemTime>,
) -> anyhow::Result<i32> {
    let UploadArgs {
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
        tags,
        print_files,
        dry_run,
        use_quarantining,
        allow_empty_test_results,
        team,
        codeowners_path,
    } = upload_args;

    let repo = BundleRepo::new(
        repo_root,
        repo_url,
        repo_head_sha,
        repo_head_branch,
        repo_head_commit_epoch,
    )?;

    let command_line = env::args()
        .collect::<Vec<String>>()
        .join(" ")
        .replace(&token, "***");
    let api_client = ApiClient::new(token)?;

    let codeowners =
        codeowners.or_else(|| CodeOwners::find_file(&repo.repo_root, &codeowners_path));

    let tags = parse_custom_tags(&tags)?;

    #[cfg(target_os = "macos")]
    {
        let junitified = junitify_xcresult(
            xcresult_path,
            junit_paths,
            repo,
            org_url_slug,
            allow_empty_test_results,
        );
        if junitified.is_err() {
            return junitified;
        }
        junit_paths = junitified.unwrap_or(junit_paths);
    }

    let (file_sets, file_counter) = build_filesets(
        &repo.repo_root,
        &junit_paths,
        team.clone(),
        &codeowners,
        exec_start,
    )?;

    if !allow_empty_test_results && (file_counter.get_count() == 0 || file_sets.is_empty()) {
        return Err(anyhow::anyhow!("No JUnit files found to upload."));
    }

    let failures = extract_failed_tests(&repo, &org_url_slug, &file_sets).await;

    // Run the quarantine step and update the exit code.
    let exit_code = if failures.is_empty() {
        EXIT_SUCCESS
    } else {
        EXIT_FAILURE
    };
    let quarantine_run_results = if use_quarantining && quarantine_results.is_none() {
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

    let (exit_code, resolved_quarantine_results) = if let Some(r) = quarantine_run_results.as_ref()
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

    let num_files = file_sets.iter().fold(0, |mut num_files, file_set| {
        num_files += file_set.files.len();
        num_files
    });
    let num_tests = parse_num_tests(&file_sets);

    let envs = EnvScanner::scan_env();
    let os_info: String = env::consts::OS.to_string();

    api_client
        .create_trunk_repo(&api::CreateRepoRequest {
            repo: repo.repo.clone(),
            org_url_slug: org_url_slug.clone(),
            remote_urls: vec![repo.repo_url.clone()],
        })
        .await?;

    let cli_version = format!(
        "cargo={} git={} rustc={}",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_SHA"),
        env!("VERGEN_RUSTC_SEMVER")
    );
    let client_version = format!("trunk-analytics-cli {}", cli_version);
    let upload = api_client
        .create_bundle_upload_intent(&api::CreateBundleUploadRequest {
            repo: repo.repo.clone(),
            org_url_slug: org_url_slug.clone(),
            client_version,
        })
        .await?;

    let meta = BundleMeta {
        base_props: BundleMetaBaseProps {
            version: META_VERSION.to_string(),
            org: org_url_slug,
            repo,
            cli_version,
            bundle_upload_id: upload.id.clone(),
            tags,
            file_sets,
            envs,
            upload_time_epoch: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            quarantined_tests: resolved_quarantine_results.quarantine_results.to_vec(),
            test_command,
            os_info: Some(os_info),
            codeowners,
        },
        junit_props: BundleMetaJunitProps {
            num_files,
            num_tests,
        },
        debug_props: BundleMetaDebugProps { command_line },
    };

    log::info!("Total files pack and upload: {}", file_counter.get_count());
    if file_counter.get_count() == 0 {
        log::warn!(
            "No JUnit files found to pack and upload using globs: {:?}",
            junit_paths
        );
    }

    if print_files {
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

    let bundle_temp_dir = tempfile::tempdir()?;
    let bundle_time_file = bundle_temp_dir.path().join("bundle.tar.zstd");
    let bundle = BundlerUtil::new(meta);
    bundle.make_tarball(&bundle_time_file)?;
    log::info!("Flushed temporary tarball to {:?}", bundle_time_file);

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
        return Ok(exit_code);
    }

    api_client
        .put_bundle_to_s3(&upload.url, &bundle_time_file)
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
    Ok(exit_code)
}

fn parse_num_tests(file_sets: &[FileSet]) -> usize {
    file_sets
        .iter()
        .flat_map(|file_set| &file_set.files)
        .filter_map(|bundled_file| {
            let path = std::path::Path::new(&bundled_file.original_path);
            let file = std::fs::File::open(path);
            if let Err(ref e) = file {
                log::warn!(
                    "Could not open file {}: {}",
                    bundled_file.get_print_path(),
                    e
                );
            }
            file.ok().map(|f| (f, bundled_file))
        })
        .filter_map(|(file, bundled_file)| {
            let file_buf_reader = BufReader::new(file);
            let mut junit_parser = JunitParser::new();
            if let Err(e) = junit_parser.parse(file_buf_reader) {
                log::warn!(
                    "Encountered error while parsing file {}: {}",
                    bundled_file.get_print_path(),
                    e
                );
                return None;
            }
            Some(junit_parser)
        })
        .flat_map(|junit_parser| junit_parser.into_reports())
        .fold(0, |num_tests, report| num_tests + report.tests)
}
