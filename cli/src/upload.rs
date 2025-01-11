#[cfg(target_os = "macos")]
use std::io::Write;
use std::{
    env,
    io::BufReader,
    time::{SystemTime, UNIX_EPOCH},
};

use api::BundleUploadStatus;
use bundle::{
    parse_custom_tags, BundleMeta, BundleMetaBaseProps, BundleMetaDebugProps, BundleMetaJunitProps,
    BundlerUtil, FileSet, FileSetBuilder, QuarantineBulkTestStatus, QuarantineRunResult, Test,
    META_VERSION,
};
use clap::{ArgAction, Args};
use codeowners::CodeOwners;
use constants::EXIT_SUCCESS;
#[cfg(target_os = "macos")]
use context::repo::RepoUrlParts;
use context::{
    bazel_bep::parser::{BazelBepParser, BepParseResult},
    junit::{junit_path::JunitReportFileWithStatus, parser::JunitParser},
    repo::BundleRepo,
};
use tempfile::TempDir;
#[cfg(target_os = "macos")]
use xcresult::XCResult;

use crate::{
    api_client::ApiClient,
    print::print_bep_results,
    quarantine::{run_quarantine, FailedTestsExtractor},
    scanner::EnvScanner,
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
    test_command: Option<String>,
    quarantine_run_result: Option<QuarantineRunResult>,
    codeowners: Option<CodeOwners>,
    exec_start: Option<SystemTime>,
) -> anyhow::Result<i32> {
    let UploadArgs {
        org_url_slug,
        token,
        print_files,
        dry_run,
        use_quarantining,
        ..
    } = upload_args.clone();

    let api_client = ApiClient::new(&token)?;

    let (
        mut meta,
        file_set_builder,
        bep_result,
        // directory is removed on drop
        _junit_path_wrappers_temp_dir,
    ) = gather_context(upload_args, test_command, codeowners, exec_start).await?;

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

    let (exit_code, quarantined_tests) = extract_exit_code_and_quarantined_tests(
        use_quarantining,
        &api_client,
        &meta.base_props,
        quarantine_run_result,
        &file_set_builder,
    )
    .await;
    meta.base_props.quarantined_tests = quarantined_tests;

    api_client
        .create_trunk_repo(&api::CreateRepoRequest {
            repo: meta.base_props.repo.repo.clone(),
            org_url_slug: org_url_slug.clone(),
            remote_urls: vec![meta.base_props.repo.repo_url.clone()],
        })
        .await?;

    let upload = api_client
        .create_bundle_upload_intent(&api::CreateBundleUploadRequest {
            repo: meta.base_props.repo.repo.clone(),
            org_url_slug: org_url_slug.clone(),
            client_version: format!("trunk-analytics-cli {}", meta.base_props.cli_version),
        })
        .await?;
    meta.base_props.bundle_upload_id.clone_from(&upload.id);
    meta.bundle_upload_id_v2 = upload.id_v2;

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

    Ok(exit_code)
}

async fn gather_context(
    upload_args: UploadArgs,
    test_command: Option<String>,
    codeowners: Option<CodeOwners>,
    exec_start: Option<SystemTime>,
) -> anyhow::Result<(
    BundleMeta,
    FileSetBuilder,
    Option<BepParseResult>,
    Option<TempDir>,
)> {
    let UploadArgs {
        junit_paths,
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
        allow_empty_test_results,
        team,
        codeowners_path,
        ..
    } = upload_args;

    let repo = BundleRepo::new(
        repo_root,
        repo_url,
        repo_head_sha,
        repo_head_branch,
        repo_head_commit_epoch,
    )?;

    let (junit_path_wrappers, bep_result, junit_path_wrappers_temp_dir) =
        coalesce_junit_path_wrappers(
            junit_paths,
            bazel_bep_path,
            #[cfg(target_os = "macos")]
            xcresult_path,
            #[cfg(target_os = "macos")]
            &repo.repo,
            #[cfg(target_os = "macos")]
            &org_url_slug,
            #[cfg(target_os = "macos")]
            allow_empty_test_results,
        )?;

    let codeowners =
        codeowners.or_else(|| CodeOwners::find_file(&repo.repo_root, &codeowners_path));

    let file_set_builder = FileSetBuilder::build_file_sets(
        &repo.repo_root,
        &junit_path_wrappers,
        team.clone(),
        &codeowners,
        exec_start,
    )?;

    if !allow_empty_test_results && file_set_builder.no_files_found() {
        return Err(anyhow::anyhow!("No JUnit files found to upload."));
    }

    log::info!("Total files pack and upload: {}", file_set_builder.count());
    if file_set_builder.no_files_found() {
        log::warn!(
            "No JUnit files found to pack and upload using globs: {:?}",
            junit_path_wrappers
                .iter()
                .map(|j| &j.junit_path)
                .collect::<Vec<_>>()
        );
    }

    let meta = BundleMeta {
        junit_props: BundleMetaJunitProps {
            num_files: file_set_builder.count(),
            num_tests: parse_num_tests(file_set_builder.file_sets()),
        },
        debug_props: BundleMetaDebugProps {
            command_line: env::args()
                .collect::<Vec<String>>()
                .join(" ")
                .replace(&token, "***"),
        },
        bundle_upload_id_v2: String::with_capacity(0),
        base_props: BundleMetaBaseProps {
            version: META_VERSION.to_string(),
            org: org_url_slug,
            repo,
            cli_version: format!(
                "cargo={} git={} rustc={}",
                env!("CARGO_PKG_VERSION"),
                env!("VERGEN_GIT_SHA"),
                env!("VERGEN_RUSTC_SEMVER")
            ),
            bundle_upload_id: String::with_capacity(0),
            tags: parse_custom_tags(&tags)?,
            file_sets: file_set_builder.file_sets().to_vec(),
            envs: EnvScanner::scan_env(),
            upload_time_epoch: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            test_command,
            quarantined_tests: Vec::with_capacity(0),
            os_info: Some(env::consts::OS.to_string()),
            codeowners,
        },
    };

    Ok((
        meta,
        file_set_builder,
        bep_result,
        junit_path_wrappers_temp_dir,
    ))
}

fn coalesce_junit_path_wrappers(
    junit_paths: Vec<String>,
    bazel_bep_path: Option<String>,
    #[cfg(target_os = "macos")] xcresult_path: Option<String>,
    #[cfg(target_os = "macos")] repo: &RepoUrlParts,
    #[cfg(target_os = "macos")] org_url_slug: &str,
    #[cfg(target_os = "macos")] allow_empty_test_results: bool,
) -> anyhow::Result<(
    Vec<JunitReportFileWithStatus>,
    Option<BepParseResult>,
    Option<TempDir>,
)> {
    let mut junit_path_wrappers = junit_paths
        .into_iter()
        .map(JunitReportFileWithStatus::from)
        .collect();

    let mut bep_result: Option<BepParseResult> = None;
    if let Some(bazel_bep_path) = bazel_bep_path {
        let mut parser = BazelBepParser::new(bazel_bep_path);
        let bep_parse_result = parser.parse()?;
        print_bep_results(&bep_parse_result);
        junit_path_wrappers = bep_parse_result.uncached_xml_files();
        bep_result = Some(bep_parse_result);
    }

    let mut _junit_path_wrappers_temp_dir = None;
    #[cfg(target_os = "macos")]
    {
        let temp_dir = tempfile::tempdir()?;
        let temp_paths = handle_xcresult(&temp_dir, xcresult_path, repo, org_url_slug)?;
        _junit_path_wrappers_temp_dir = Some(temp_dir);
        junit_path_wrappers = [junit_path_wrappers.as_slice(), temp_paths.as_slice()].concat();
        if junit_path_wrappers.is_empty() {
            if allow_empty_test_results {
                log::warn!("No tests found in the provided XCResult path.");
            } else {
                return Err(anyhow::anyhow!(
                    "No tests found in the provided XCResult path."
                ));
            }
        }
    }

    Ok((
        junit_path_wrappers,
        bep_result,
        _junit_path_wrappers_temp_dir,
    ))
}

async fn extract_exit_code_and_quarantined_tests(
    use_quarantining: bool,
    api_client: &ApiClient,
    meta_base_props: &BundleMetaBaseProps,
    quarantine_run_result: Option<QuarantineRunResult>,
    file_set_builder: &FileSetBuilder,
) -> (i32, Vec<Test>) {
    // Run the quarantine step and update the exit code.
    let failed_tests_extractor = FailedTestsExtractor::new(
        &meta_base_props.repo.repo,
        &meta_base_props.org,
        file_set_builder.file_sets(),
    );
    let QuarantineRunResult {
        exit_code,
        quarantine_status:
            QuarantineBulkTestStatus {
                quarantine_results: quarantined_tests,
                ..
            },
    } = if !use_quarantining {
        QuarantineRunResult {
            exit_code: failed_tests_extractor.exit_code(),
            ..Default::default()
        }
    } else if let Some(quarantine_run_result) = quarantine_run_result {
        quarantine_run_result
    } else {
        run_quarantine(
            api_client,
            &api::GetQuarantineBulkTestStatusRequest {
                repo: meta_base_props.repo.repo.clone(),
                org_url_slug: meta_base_props.org.clone(),
                test_identifiers: failed_tests_extractor.failed_tests().to_vec(),
            },
            file_set_builder,
            Some(failed_tests_extractor),
            None,
        )
        .await
    };

    (exit_code, quarantined_tests)
}

#[cfg(target_os = "macos")]
fn handle_xcresult(
    junit_temp_dir: &tempfile::TempDir,
    xcresult_path: Option<String>,
    repo: &RepoUrlParts,
    org_url_slug: &str,
) -> Result<Vec<JunitReportFileWithStatus>, anyhow::Error> {
    let mut temp_paths = Vec::new();
    if let Some(xcresult_path) = xcresult_path {
        let xcresult = XCResult::new(xcresult_path, repo, org_url_slug);
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
                temp_paths.push(JunitReportFileWithStatus {
                    junit_path: junit_temp_path_string.to_string(),
                    status: None,
                });
            } else {
                return Err(anyhow::anyhow!(
                    "Failed to convert junit temp path to string."
                ));
            }
        }
    }
    Ok(temp_paths)
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
