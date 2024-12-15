use clap::{ArgAction, Args};
#[cfg(target_os = "macos")]
use std::io::Write;
use std::{
    env,
    io::BufReader,
    time::{SystemTime, UNIX_EPOCH},
};
#[cfg(target_os = "macos")]
use xcresult::XCResult;

use api::BundleUploadStatus;
use bundle::{
    parse_custom_tags, BundleMeta, BundleMetaBaseProps, BundleMetaDebugProps, BundleMetaJunitProps,
    BundlerUtil, FileSet, QuarantineBulkTestStatus, QuarantineRunResult, META_VERSION,
};
use codeowners::CodeOwners;
use constants::{EXIT_FAILURE, EXIT_SUCCESS};
#[cfg(target_os = "macos")]
use context::repo::RepoUrlParts;
use context::{
    bazel_bep::parser::{BazelBepParser, BepParseResult},
    junit::parser::JunitParser,
    repo::BundleRepo,
};

use crate::{
    api_client::ApiClient,
    print::print_bep_results,
    runner::{build_filesets, extract_failed_tests, run_quarantine},
    scanner::EnvScanner,
};

#[derive(Args, Clone, Debug)]
pub struct UploadArgs {
    #[arg(
        long,
        required_unless_present_any = [junit_require(), "bazel_bep_path"],
        conflicts_with = "bazel_bep_path",
        value_delimiter = ',',
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        help = "Comma-separated list of glob paths to junit files."
    )]
    pub junit_paths: Vec<String>,
    #[arg(
        long,
        required_unless_present_any = [junit_require(), "junit_paths"],
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

pub async fn run_upload(
    upload_args: UploadArgs,
    test_command: Option<String>,
    quarantine_results: Option<QuarantineRunResult>,
    codeowners: Option<CodeOwners>,
    exec_start: Option<SystemTime>,
) -> anyhow::Result<i32> {
    let UploadArgs {
        mut junit_paths,
        #[cfg(target_os = "macos")]
        xcresult_path,
        bazel_bep_path,
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

    let mut bep_result: Option<BepParseResult> = None;
    if let Some(bazel_bep_path) = bazel_bep_path {
        let mut parser = BazelBepParser::new(bazel_bep_path);
        let bep_parse_result = parser.parse()?;
        print_bep_results(&bep_parse_result);
        junit_paths = bep_parse_result.uncached_xml_files();
        bep_result = Some(bep_parse_result);
    }

    let tags = parse_custom_tags(&tags)?;
    #[cfg(target_os = "macos")]
    let junit_temp_dir = tempfile::tempdir()?;
    #[cfg(target_os = "macos")]
    {
        let temp_paths =
            handle_xcresult(&junit_temp_dir, xcresult_path, &repo.repo, &org_url_slug)?;
        junit_paths = [junit_paths.as_slice(), temp_paths.as_slice()].concat();
        if junit_paths.is_empty() && !allow_empty_test_results {
            return Err(anyhow::anyhow!(
                "No tests found in the provided XCResult path."
            ));
        } else if junit_paths.is_empty() && allow_empty_test_results {
            log::warn!("No tests found in the provided XCResult path.");
        }
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
            run_quarantine(
                &api_client,
                &api::GetQuarantineBulkTestStatusRequest {
                    repo: repo.repo.clone(),
                    org_url_slug: org_url_slug.clone(),
                    test_identifiers: failures.clone(),
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
            test_command,
            quarantined_tests: resolved_quarantine_results.quarantine_results.to_vec(),
            os_info: Some(os_info),
            codeowners,
        },
        junit_props: BundleMetaJunitProps {
            num_files,
            num_tests,
        },
        debug_props: BundleMetaDebugProps { command_line },
        bundle_upload_id_v2: upload.id_v2,
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
    let bundle = BundlerUtil::new(meta, bep_result);
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

#[cfg(target_os = "macos")]
fn handle_xcresult(
    junit_temp_dir: &tempfile::TempDir,
    xcresult_path: Option<String>,
    repo: &RepoUrlParts,
    org_url_slug: &str,
) -> Result<Vec<String>, anyhow::Error> {
    let mut temp_paths = Vec::new();
    if let Some(xcresult_path) = xcresult_path {
        let xcresult = XCResult::new(xcresult_path, repo, org_url_slug.to_string());
        let junits = xcresult?
            .generate_junits()
            .map_err(|e| anyhow::anyhow!("Failed to generate junit files from xcresult: {}", e))?;
        for (i, junit) in junits.iter().enumerate() {
            let mut junit_writer: Vec<u8> = Vec::new();
            junit.serialize(&mut junit_writer)?;
            let junit_temp_path = junit_temp_dir
                .path()
                .join(format!("xcresult_junit_{}.xml", i));
            let mut junit_temp = std::fs::File::create(&junit_temp_path)?;
            junit_temp
                .write_all(&junit_writer)
                .map_err(|e| anyhow::anyhow!("Failed to write junit file: {}", e))?;
            let junit_temp_path_str = junit_temp_path.to_str();
            if let Some(junit_temp_path_string) = junit_temp_path_str {
                temp_paths.push(junit_temp_path_string.to_string());
            } else {
                return Err(anyhow::anyhow!(
                    "Failed to convert junit temp path to string."
                ));
            }
        }
    }
    Ok(temp_paths)
}

fn junit_require() -> &'static str {
    if cfg!(target_os = "macos") {
        "xcresult_path"
    } else {
        "junit_paths"
    }
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
