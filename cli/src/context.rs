#[cfg(target_os = "macos")]
use std::io::Write;
use std::{
    collections::HashMap,
    env,
    io::BufReader,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use api::CreateBundleUploadResponse;
use bundle::{
    parse_custom_tags, BundleMeta, BundleMetaBaseProps, BundleMetaDebugProps, BundleMetaJunitProps,
    FileSet, FileSetBuilder, QuarantineBulkTestStatus, META_VERSION,
};
use constants::ENVS_TO_GET;
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
    context_quarantine::{gather_quarantine_context, FailedTestsExtractor, QuarantineContext},
    print::print_bep_results,
    test_command::TestRunResult,
    upload_command::UploadArgs,
};

pub struct PreTestContext {
    pub meta: BundleMeta,
    pub junit_path_wrappers: Vec<JunitReportFileWithStatus>,
    pub bep_result: Option<BepParseResult>,
    pub junit_path_wrappers_temp_dir: Option<TempDir>,
}

pub fn gather_pre_test_context(upload_args: UploadArgs) -> anyhow::Result<PreTestContext> {
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
        #[cfg(target_os = "macos")]
        allow_empty_test_results,
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

    let envs: HashMap<String, String> = ENVS_TO_GET
        .iter()
        .filter_map(|&env_var| {
            env::var(env_var)
                .map(|env_var_value| (env_var.to_string(), env_var_value))
                .ok()
        })
        .collect();

    let meta = BundleMeta {
        junit_props: BundleMetaJunitProps::default(),
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
            file_sets: Vec::with_capacity(0),
            envs,
            upload_time_epoch: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            test_command: None,
            quarantined_tests: Vec::with_capacity(0),
            os_info: Some(env::consts::OS.to_string()),
            codeowners: None,
        },
    };

    Ok(PreTestContext {
        meta,
        junit_path_wrappers,
        bep_result,
        junit_path_wrappers_temp_dir,
    })
}

pub fn gather_post_test_context<U: AsRef<Path>>(
    meta: &mut BundleMeta,
    junit_path_wrappers: Vec<JunitReportFileWithStatus>,
    team: &Option<String>,
    codeowners_path: &Option<U>,
    allow_empty_test_results: bool,
    test_run_result: &Option<TestRunResult>,
) -> anyhow::Result<FileSetBuilder> {
    let mut file_set_builder = FileSetBuilder::build_file_sets(
        &meta.base_props.repo.repo_root,
        &junit_path_wrappers,
        team,
        codeowners_path,
        test_run_result.as_ref().map(|r| r.exec_start),
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

    meta.junit_props = BundleMetaJunitProps {
        num_files: file_set_builder.count(),
        num_tests: parse_num_tests(file_set_builder.file_sets()),
    };
    meta.base_props.file_sets = file_set_builder.file_sets().to_vec();
    meta.base_props.codeowners = file_set_builder.take_codeowners();
    meta.base_props.test_command = test_run_result.as_ref().map(|r| r.command.clone());

    Ok(file_set_builder)
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

pub async fn gather_exit_code_and_quarantined_tests_context(
    meta: &mut BundleMeta,
    use_quarantining: bool,
    api_client: &ApiClient,
    file_set_builder: &FileSetBuilder,
    test_run_result: &Option<TestRunResult>,
) -> i32 {
    // Run the quarantine step and update the exit code.
    let failed_tests_extractor = FailedTestsExtractor::new(
        &meta.base_props.repo.repo,
        &meta.base_props.org,
        file_set_builder.file_sets(),
    );
    let QuarantineContext {
        exit_code,
        quarantine_status:
            QuarantineBulkTestStatus {
                quarantine_results: quarantined_tests,
                ..
            },
    } = if !use_quarantining {
        QuarantineContext {
            exit_code: test_run_result
                .as_ref()
                .map(|t| t.exit_code)
                .unwrap_or_else(|| failed_tests_extractor.exit_code()),
            ..Default::default()
        }
    } else {
        gather_quarantine_context(
            api_client,
            &api::GetQuarantineBulkTestStatusRequest {
                repo: meta.base_props.repo.repo.clone(),
                org_url_slug: meta.base_props.org.clone(),
                test_identifiers: failed_tests_extractor.failed_tests().to_vec(),
            },
            file_set_builder,
            Some(failed_tests_extractor),
            test_run_result.as_ref().map(|t| t.exit_code),
        )
        .await
    };

    meta.base_props.quarantined_tests = quarantined_tests;

    exit_code
}

pub async fn gather_upload_id_context(
    meta: &mut BundleMeta,
    api_client: &ApiClient,
) -> anyhow::Result<CreateBundleUploadResponse> {
    let upload = api_client
        .create_bundle_upload_intent(&api::CreateBundleUploadRequest {
            repo: meta.base_props.repo.repo.clone(),
            org_url_slug: meta.base_props.org.clone(),
            client_version: format!("trunk-analytics-cli {}", meta.base_props.cli_version),
        })
        .await?;
    meta.base_props.bundle_upload_id.clone_from(&upload.id);
    meta.bundle_upload_id_v2.clone_from(&upload.id_v2);
    Ok(upload)
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