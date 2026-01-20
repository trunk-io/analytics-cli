#[cfg(target_os = "macos")]
use std::io::Write;
use std::{collections::BTreeMap, io::Read};
use std::{
    collections::HashMap,
    env,
    io::BufReader,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use api::{client::ApiClient, message::CreateBundleUploadResponse};
use bundle::{
    BundleMeta, BundleMetaBaseProps, BundleMetaDebugProps, BundleMetaJunitProps, BundledFile,
    FileSet, FileSetBuilder, FileSetType, INTERNAL_BIN_FILENAME, META_VERSION,
    QuarantineBulkTestStatus, bin_parse,
};
use codeowners::CodeOwners;
use constants::ENVS_TO_GET;
use context::{
    bazel_bep::{
        binary_parser::BazelBepBinParser,
        common::{BepParseResult, map_test_status_to_build_result},
        parser::BazelBepParser,
    },
    junit::{
        bindings::BindingsReport,
        junit_path::JunitReportFileWithTestRunnerReport,
        parser::JunitParser,
        validator::{JunitReportValidation, validate},
    },
    repo::{BundleRepo, RepoUrlParts},
};
use github_actions::extract_github_external_id;
use lazy_static::lazy_static;
use prost::Message;
use proto::test_context::test_run::{
    BazelBuildInformation, TestBuildResult, TestReport, TestResult, UploaderMetadata,
};
use regex::Regex;
use tempfile::TempDir;
#[cfg(target_os = "macos")]
use xcresult::xcresult::XCResult;

use crate::error_report::InterruptingError;
use crate::{
    context_quarantine::{
        FailedTestsExtractor, QuarantineContext, QuarantineFetchStatus, gather_quarantine_context,
    },
    print::print_bep_results,
    test_command::TestRunResult,
    upload_command::UploadArgs,
};

pub struct PreTestContext {
    pub meta: BundleMeta,
    pub junit_path_wrappers: Vec<JunitReportFileWithTestRunnerReport>,
    pub bep_result: Option<BepParseResult>,
    pub junit_path_wrappers_temp_dir: Option<TempDir>,
}

lazy_static! {
    static ref COMMAND_REGEX: Regex = Regex::new(r"--token[=]?").unwrap();
}

// This function is used to gather debug properties for the bundle meta.
// It will trigger EXC_BAD_ACCESS on arm64-darwin builds when compiled under cdylib
pub fn gather_debug_props(args: Vec<String>, token: String) -> BundleMetaDebugProps {
    BundleMetaDebugProps {
        command_line: COMMAND_REGEX
            .replace(&args.join(" "), "")
            .replace(&token, "")
            .trim()
            .to_string(),
    }
}

fn capture_env_vars() -> HashMap<String, String> {
    let mut envs: HashMap<String, String> = ENVS_TO_GET
        .iter()
        .filter_map(|&env_var| {
            env::var(env_var)
                .map(|env_var_value| (env_var.to_string(), env_var_value))
                .ok()
        })
        .collect();

    match extract_github_external_id() {
        Ok(Some(external_id)) => {
            tracing::info!("Extracted GitHub Actions external ID: {}", external_id);
            // Set the environment variable for upstream use
            envs.insert("GITHUB_EXTERNAL_ID".to_string(), external_id);
        }
        Ok(None) => {
            tracing::debug!("Not running in GitHub Actions or no external ID found");
        }
        Err(e) => {
            tracing::error!("Failed to extract GitHub Actions external ID: {}", e);
        }
    };

    envs
}

pub fn gather_initial_test_context(
    upload_args: UploadArgs,
    debug_props: BundleMetaDebugProps,
) -> anyhow::Result<PreTestContext> {
    let UploadArgs {
        junit_paths,
        #[cfg(target_os = "macos")]
        xcresult_path,
        bazel_bep_path,
        test_reports,
        org_url_slug,
        repo_root,
        repo_url,
        repo_head_sha,
        repo_head_branch,
        repo_head_commit_epoch,
        allow_empty_test_results,
        repo_head_author_name,
        #[cfg(target_os = "macos")]
        use_experimental_failure_summary,
        ..
    } = upload_args;

    let repo = BundleRepo::new(
        repo_root,
        repo_url,
        repo_head_sha,
        repo_head_branch,
        repo_head_commit_epoch,
        repo_head_author_name,
        upload_args.use_uncloned_repo,
    )?;
    tracing::debug!("Found repo state: {:?}", repo);

    let (junit_path_wrappers, bep_result, junit_path_wrappers_temp_dir) =
        coalesce_junit_path_wrappers(
            junit_paths,
            bazel_bep_path,
            #[cfg(target_os = "macos")]
            xcresult_path,
            #[cfg(target_os = "macos")]
            &repo.repo,
            #[cfg(target_os = "macos")]
            org_url_slug.clone(),
            #[cfg(target_os = "macos")]
            use_experimental_failure_summary,
            test_reports,
            allow_empty_test_results,
        )?;

    let envs = capture_env_vars();

    let meta = BundleMeta {
        junit_props: BundleMetaJunitProps::default(),
        debug_props,
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
            tags: vec![],
            file_sets: Vec::with_capacity(0),
            envs,
            upload_time_epoch: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            test_command: None,
            quarantined_tests: Vec::with_capacity(0),
            os_info: Some(env::consts::OS.to_string()),
            codeowners: None,
            use_uncloned_repo: Some(upload_args.use_uncloned_repo),
        },
        failed_tests: Vec::with_capacity(0),
        variant: upload_args.variant.as_ref().map(|v| {
            if v.len() > 64 {
                tracing::warn!(
                    "Variant '{}' exceeds 64 character limit and will be truncated",
                    v
                );
                v[..64].to_string()
            } else {
                v.clone()
            }
        }),
        internal_bundled_file: None,
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
    junit_path_wrappers: Vec<JunitReportFileWithTestRunnerReport>,
    codeowners_path: &Option<U>,
    allow_empty_test_results: bool,
    test_run_result: &Option<TestRunResult>,
) -> anyhow::Result<FileSetBuilder> {
    let mut file_set_builder = FileSetBuilder::build_file_sets(
        &meta.base_props.repo.repo_root,
        &junit_path_wrappers,
        codeowners_path,
        test_run_result.as_ref().and_then(|r| r.exec_start),
    )?;

    if !allow_empty_test_results && file_set_builder.no_files_found() {
        return Err(anyhow::anyhow!(InterruptingError::new(
            "No test output files found to upload."
        )));
    }

    tracing::info!("Total files pack and upload: {}", file_set_builder.count());
    if file_set_builder.no_files_found() {
        tracing::warn!(
            "No test output files found to pack and upload using globs: {:?}",
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

fn parse_and_optionally_validate_junit_file(
    junit_path: &str,
    show_warnings: bool,
    test_runner_report: Option<context::junit::junit_path::TestRunnerReport>,
    junit_validations: &mut BTreeMap<String, anyhow::Result<JunitReportValidation>>,
) -> Option<JunitParser> {
    let mut junit_parser = JunitParser::new();

    let file_contents = match std::fs::read_to_string(junit_path) {
        Ok(contents) => contents,
        Err(e) => {
            if show_warnings {
                junit_validations.insert(junit_path.to_string(), Err(e.into()));
            }
            return None;
        }
    };

    let parsed_results = junit_parser.parse(BufReader::new(file_contents.as_bytes()));
    if let Err(e) = parsed_results {
        if show_warnings {
            tracing::warn!("Failed to parse JUnit file {:?}: {:?}", junit_path, e);
            junit_validations.insert(junit_path.to_string(), Err(e));
        }
        return None;
    }

    if show_warnings {
        let reports = junit_parser.reports();
        let reference_timestamp = chrono::Utc::now().fixed_offset();
        for report in reports {
            let bindings_report = BindingsReport::from(report.clone());
            let validation = validate(&bindings_report, &test_runner_report, reference_timestamp);
            junit_validations.insert(
                junit_path.to_string(),
                Ok(JunitReportValidation::from(validation)),
            );
        }
    }

    Some(junit_parser)
}

fn create_test_result(
    test_case_runs: Vec<proto::test_context::test_run::TestCaseRun>,
    variant: Option<String>,
    build_information: Option<proto::test_context::test_run::test_result::TestBuildInformation>,
) -> TestResult {
    TestResult {
        test_case_runs,
        // trunk-ignore(clippy/deprecated)
        uploader_metadata: Some(UploaderMetadata {
            variant: variant.unwrap_or_default(),
            ..Default::default()
        }),
        test_build_information: build_information,
    }
}

fn write_test_report_to_file(
    test_results: Vec<TestResult>,
    variant: Option<String>,
    temp_dir: &TempDir,
) -> anyhow::Result<BundledFile> {
    let test_report = TestReport {
        uploader_metadata: Some(UploaderMetadata {
            variant: variant.unwrap_or_default(),
            ..Default::default()
        }),
        test_results,
    };

    let mut buf = Vec::new();
    prost::Message::encode(&test_report, &mut buf)?;
    let test_report_path = temp_dir.path().join(INTERNAL_BIN_FILENAME);
    std::fs::write(&test_report_path, buf)?;

    Ok(BundledFile {
        original_path: test_report_path.to_string_lossy().to_string(),
        original_path_rel: None,
        owners: vec![],
        path: INTERNAL_BIN_FILENAME.to_string(),
        // last_modified_epoch_ns does not serialize so the compiler complains it does not exist
        ..Default::default()
    })
}

fn get_build_result_from_bep(bep_result: &BepParseResult, label: &str) -> i32 {
    bep_result
        .test_results
        .iter()
        .find(|test_result| test_result.label == label)
        .and_then(|test_result| test_result.build_status)
        .map(|status| map_test_status_to_build_result(status) as i32)
        .unwrap_or(TestBuildResult::Unspecified as i32)
}

pub fn generate_internal_file_from_bep(
    bep_result: &BepParseResult,
    temp_dir: &TempDir,
    codeowners: Option<&CodeOwners>,
    show_warnings: bool,
    variant: Option<String>,
    org_slug: &String,
    repo: &RepoUrlParts,
    quarantined_test_ids: &[String],
) -> anyhow::Result<(
    BundledFile,
    BTreeMap<String, anyhow::Result<JunitReportValidation>>,
)> {
    let mut junit_validations = BTreeMap::new();
    let labels_map = bep_result.uncached_labels();
    let mut test_results = vec![];

    for (label, file_with_test_runner_report) in labels_map.into_iter() {
        let mut test_case_runs = Vec::new();

        let bep_test_result = bep_result
            .test_results
            .iter()
            .find(|test_result| test_result.label == label);

        if let Some(bep_result) = bep_test_result {
            for xml_file in bep_result.xml_files.iter() {
                if xml_file.file.ends_with(".xml") {
                    let Some(junit_parser) = parse_and_optionally_validate_junit_file(
                        &xml_file.file,
                        show_warnings,
                        None,
                        &mut junit_validations,
                    ) else {
                        continue;
                    };

                    let mut xml_test_case_runs = junit_parser.into_test_case_runs(
                        codeowners,
                        org_slug,
                        repo,
                        quarantined_test_ids,
                        variant.as_deref().unwrap_or(""),
                    );
                    for test_case_run in &mut xml_test_case_runs {
                        test_case_run.attempt_number = xml_file.attempt;
                    }
                    test_case_runs.extend(xml_test_case_runs);
                }
            }
        } else {
            for JunitReportFileWithTestRunnerReport {
                junit_path,
                test_runner_report,
            } in file_with_test_runner_report
            {
                if junit_path.ends_with(".xml") {
                    let Some(junit_parser) = parse_and_optionally_validate_junit_file(
                        &junit_path,
                        show_warnings,
                        test_runner_report,
                        &mut junit_validations,
                    ) else {
                        continue;
                    };

                    test_case_runs.extend(junit_parser.into_test_case_runs(
                        codeowners,
                        org_slug,
                        repo,
                        quarantined_test_ids,
                        variant.as_deref().unwrap_or(""),
                    ));
                }
            }
        }

        let build_result = get_build_result_from_bep(bep_result, &label);

        test_results.push(create_test_result(
            test_case_runs,
            variant.clone(),
            Some(proto::test_context::test_run::test_result::TestBuildInformation::BazelBuildInformation(
                BazelBuildInformation {
                    label,
                    result: build_result,
                }
            )),
        ));
    }

    let bundled_file = write_test_report_to_file(test_results, variant, temp_dir)?;

    Ok((bundled_file, junit_validations))
}

pub fn generate_internal_file(
    file_sets: &[FileSet],
    temp_dir: &TempDir,
    codeowners: Option<&CodeOwners>,
    show_warnings: bool,
    variant: Option<String>,
    org_slug: &String,
    repo: &RepoUrlParts,
    quarantined_test_ids: &[String],
) -> anyhow::Result<(
    BundledFile,
    BTreeMap<String, anyhow::Result<JunitReportValidation>>,
)> {
    let mut junit_validations = BTreeMap::new();
    let mut test_case_runs = Vec::new();

    for file_set in file_sets {
        if file_set.file_set_type == FileSetType::Internal {
            if file_set.files.is_empty() {
                return Err(anyhow::anyhow!("Internal file set is empty"));
            }
            if file_set.files.len() > 1 {
                return Err(anyhow::anyhow!(
                    "Internal file set contains more than one file"
                ));
            }
            // Internal file set, we should just use that directly and assume it's valid
            return Ok((file_set.files[0].clone(), BTreeMap::new()));
        } else {
            for file in &file_set.files {
                if file.original_path.ends_with(".xml") {
                    let test_runner_report = file_set.test_runner_report.clone().map(|t| t.into());

                    let Some(junit_parser) = parse_and_optionally_validate_junit_file(
                        &file.original_path,
                        show_warnings,
                        test_runner_report,
                        &mut junit_validations,
                    ) else {
                        continue;
                    };

                    test_case_runs.extend(junit_parser.into_test_case_runs(
                        codeowners,
                        org_slug,
                        repo,
                        quarantined_test_ids,
                        variant.as_deref().unwrap_or(""),
                    ));
                }
            }
        }
    }

    let test_results = vec![create_test_result(
        test_case_runs,
        variant.clone(),
        None, // No build information for non-BEP files
    )];

    let bundled_file = write_test_report_to_file(test_results, variant, temp_dir)?;

    Ok((bundled_file, junit_validations))
}

pub fn fall_back_to_binary_parse(
    json_parse_result: anyhow::Result<BepParseResult>,
    bazel_bep_path: &String,
) -> anyhow::Result<BepParseResult> {
    let mut binary_parser = BazelBepBinParser::new(bazel_bep_path);
    match json_parse_result {
        anyhow::Result::Ok(result) if !result.errors.is_empty() => {
            let binary_result = binary_parser.parse();
            match binary_result {
                anyhow::Result::Ok(result) if result.errors.is_empty() => {
                    anyhow::Result::Ok(result)
                }
                _ => anyhow::Result::Ok(result),
            }
        }
        anyhow::Result::Err(json_error) => {
            let binary_result = binary_parser.parse();
            match binary_result {
                anyhow::Result::Ok(result) => anyhow::Result::Ok(result),
                _ => anyhow::Result::Err(json_error),
            }
        }
        just_json => just_json,
    }
}

fn parse_as_bep(dir: String) -> anyhow::Result<BepParseResult> {
    let mut parser = BazelBepParser::new(&dir);
    let result = fall_back_to_binary_parse(parser.parse(), &dir);
    if let anyhow::Result::Ok(ref ok_result) = result {
        print_bep_results(ok_result);
    }
    result
}

fn coalesce_junit_path_wrappers(
    junit_paths: Vec<String>,
    bazel_bep_path: Option<String>,
    #[cfg(target_os = "macos")] xcresult_path: Option<String>,
    #[cfg(target_os = "macos")] repo: &RepoUrlParts,
    #[cfg(target_os = "macos")] org_url_slug: String,
    #[cfg(target_os = "macos")] use_experimental_failure_summary: bool,
    test_reports: Vec<String>,
    allow_empty_test_results: bool,
) -> anyhow::Result<(
    Vec<JunitReportFileWithTestRunnerReport>,
    Option<BepParseResult>,
    Option<TempDir>,
)> {
    let mut junit_path_wrappers = junit_paths
        .into_iter()
        .map(JunitReportFileWithTestRunnerReport::from)
        .collect();

    let mut bep_result: Option<BepParseResult> = None;
    if let Some(bazel_bep_path) = bazel_bep_path {
        let mut parser = BazelBepParser::new(&bazel_bep_path);
        let result = fall_back_to_binary_parse(parser.parse(), &bazel_bep_path);
        let bep_parse_result = match result {
            anyhow::Result::Ok(result) => result,
            anyhow::Result::Err(e) => {
                if allow_empty_test_results {
                    tracing::warn!(
                        "Failed to parse Bazel BEP file at {}: {}",
                        bazel_bep_path,
                        e
                    );
                    tracing::warn!(
                        "Allow empty test results enabled - continuing without test results."
                    );
                    return Ok((junit_path_wrappers, None, None));
                }
                return Err(anyhow::anyhow!(
                    "Failed to parse Bazel BEP file at {}: {}",
                    bazel_bep_path,
                    e
                ));
            }
        };
        print_bep_results(&bep_parse_result);
        junit_path_wrappers = bep_parse_result.uncached_xml_files();
        bep_result = Some(bep_parse_result);
    }

    let mut _junit_path_wrappers_temp_dir = None;
    #[cfg(target_os = "macos")]
    if xcresult_path.is_some() {
        let temp_dir = tempfile::tempdir()?;
        let temp_paths = handle_xcresult(
            &temp_dir,
            xcresult_path,
            repo,
            &org_url_slug,
            use_experimental_failure_summary,
        )?;
        _junit_path_wrappers_temp_dir = Some(temp_dir);
        junit_path_wrappers = [junit_path_wrappers.as_slice(), temp_paths.as_slice()].concat();
        if junit_path_wrappers.is_empty() {
            if allow_empty_test_results {
                tracing::warn!("No tests found in the provided XCResult path.");
            } else {
                return Err(anyhow::anyhow!(
                    "No tests found in the provided XCResult path."
                ));
            }
        }
    }

    if !test_reports.is_empty() {
        for test_report in test_reports {
            if let Ok(bazel_result) = parse_as_bep(test_report.clone()) {
                if bep_result.is_some() {
                    return Err(anyhow::anyhow!(
                        "Was given multiple bazel BEP files (can only support one)"
                    ));
                }
                bep_result = Some(bazel_result.clone());
                junit_path_wrappers = [
                    junit_path_wrappers.as_slice(),
                    bazel_result.uncached_xml_files().as_slice(),
                ]
                .concat();
            } else if let Some(_temp_dir) = parse_as_xcresult(
                #[cfg(target_os = "macos")]
                &test_report,
                #[cfg(target_os = "macos")]
                repo,
                #[cfg(target_os = "macos")]
                &org_url_slug,
                #[cfg(target_os = "macos")]
                use_experimental_failure_summary,
            ) {
                #[cfg(target_os = "macos")]
                {
                    if _junit_path_wrappers_temp_dir.is_some() {
                        return Err(anyhow::anyhow!(
                            "Was given multiple XCResult files (can only support one)"
                        ));
                    }
                    _junit_path_wrappers_temp_dir = Some(_temp_dir);
                }
            } else {
                junit_path_wrappers.push(JunitReportFileWithTestRunnerReport::from(test_report));
            }
        }
    }

    Ok((
        junit_path_wrappers,
        bep_result,
        _junit_path_wrappers_temp_dir,
    ))
}

fn parse_as_xcresult(
    #[cfg(target_os = "macos")] test_report: &String,
    #[cfg(target_os = "macos")] repo: &RepoUrlParts,
    #[cfg(target_os = "macos")] org_url_slug: &String,
    #[cfg(target_os = "macos")] use_experimental_failure_summary: bool,
) -> Option<tempfile::TempDir> {
    #[cfg(target_os = "macos")]
    {
        let temp_dir = tempfile::tempdir().ok()?;
        let temp_paths = handle_xcresult(
            &temp_dir,
            Some(test_report.clone()),
            repo,
            &org_url_slug,
            use_experimental_failure_summary,
        );
        if temp_paths.is_ok() {
            return Some(temp_dir);
        } else {
            return None;
        }
    }
    #[cfg(not(target_os = "macos"))]
    None
}

pub async fn gather_exit_code_and_quarantined_tests_context(
    meta: &mut BundleMeta,
    disable_quarantining: bool,
    api_client: &ApiClient,
    file_set_builder: &FileSetBuilder,
    default_exit_code: Option<i32>,
) -> anyhow::Result<QuarantineContext> {
    // Run the quarantine step and update the exit code.
    let failed_tests_extractor = FailedTestsExtractor::new(
        &meta.base_props.repo.repo,
        &meta.base_props.org,
        file_set_builder.file_sets(),
        &meta.variant.clone().unwrap_or(String::from("")),
    );
    let quarantine_context = if disable_quarantining {
        // use the exit code of the test run result if exists
        if let Some(exit_code) = default_exit_code {
            QuarantineContext {
                exit_code,
                quarantine_status: QuarantineBulkTestStatus {
                    quarantine_results: failed_tests_extractor
                        .failed_tests()
                        .iter()
                        .filter_map(|test| {
                            if test.is_quarantined {
                                Some(test.clone())
                            } else {
                                None
                            }
                        })
                        .collect(),
                    ..Default::default()
                },
                failures: failed_tests_extractor.failed_tests().into(),
                repo: RepoUrlParts::default(),
                org_url_slug: String::default(),
                fetch_status: QuarantineFetchStatus::FetchSkipped,
            }
        } else {
            // default to success if no test run result (i.e. `upload`)
            QuarantineContext::skip_fetch(failed_tests_extractor.failed_tests().to_vec())
        }
    } else {
        gather_quarantine_context(
            api_client,
            &api::message::GetQuarantineConfigRequest {
                repo: meta.base_props.repo.repo.clone(),
                org_url_slug: meta.base_props.org.clone(),
                test_identifiers: failed_tests_extractor.failed_tests().to_vec(),
                remote_urls: vec![meta.base_props.repo.repo_url.clone()],
            },
            file_set_builder,
            Some(failed_tests_extractor),
            default_exit_code,
            &meta.variant.clone().unwrap_or(String::from("")),
        )
        .await?
    };
    Ok(quarantine_context)
}

pub async fn gather_upload_id_context(
    meta: &mut BundleMeta,
    api_client: &ApiClient,
    dry_run: bool,
) -> anyhow::Result<CreateBundleUploadResponse> {
    if dry_run {
        return Err(anyhow::anyhow!("Dry run mode enabled, unable to upload"));
    }
    let external_id = meta.base_props.envs.get("GITHUB_EXTERNAL_ID").cloned();
    let upload = api_client
        .create_bundle_upload(&api::message::CreateBundleUploadRequest {
            repo: meta.base_props.repo.repo.clone(),
            org_url_slug: meta.base_props.org.clone(),
            client_version: format!("trunk-analytics-cli {}", meta.base_props.cli_version),
            remote_urls: vec![meta.base_props.repo.repo_url.clone()],
            external_id,
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
    org_url_slug: &String,
    use_experimental_failure_summary: bool,
) -> Result<Vec<JunitReportFileWithTestRunnerReport>, anyhow::Error> {
    let mut temp_paths = Vec::new();
    if let Some(xcresult_path) = xcresult_path {
        let xcresult = XCResult::new(
            xcresult_path,
            org_url_slug.clone(),
            repo.repo_full_name(),
            use_experimental_failure_summary,
        )?;
        let junits = xcresult.generate_junits();
        if junits.is_empty() {
            return Err(anyhow::anyhow!(
                "Failed to generate test result files from xcresult."
            ));
        }
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
                temp_paths.push(JunitReportFileWithTestRunnerReport::from(
                    junit_temp_path_string.to_string(),
                ));
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
    let bundled_files = file_sets
        .iter()
        .flat_map(|file_set| &file_set.files)
        .filter_map(|bundled_file| {
            let path = std::path::Path::new(&bundled_file.original_path);
            let file = std::fs::File::open(path);
            if let Err(ref e) = file {
                tracing::warn!(
                    "Could not open file {}: {}",
                    bundled_file.get_print_path(),
                    e
                );
            }
            file.ok().map(|f| (f, bundled_file))
        });

    let junit_num_tests = bundled_files
        .clone()
        .filter_map(|(file, bundled_file)| {
            let file_buf_reader = BufReader::new(file);
            let mut junit_parser = JunitParser::new();
            // skip non xml files
            if !bundled_file.original_path.ends_with(".xml") {
                return None;
            }
            if let Err(e) = junit_parser.parse(file_buf_reader) {
                tracing::warn!(
                    "Encountered error while parsing file {}: {}",
                    bundled_file.get_print_path(),
                    e
                );
                return None;
            }
            Some(junit_parser)
        })
        .flat_map(|junit_parser| junit_parser.into_reports())
        .fold(0, |num_tests, report| num_tests + report.tests);

    let internal_num_tests = bundled_files
        .filter_map(|(file, bundled_file)| {
            let mut file_buf_reader = BufReader::new(file);
            // skip non bin files
            if !bundled_file.original_path.ends_with(".bin") {
                return None;
            }
            let mut buffer = Vec::new();
            let result = file_buf_reader.read_to_end(&mut buffer);
            if let Err(ref e) = result {
                tracing::warn!(
                    "Encountered error while reading file {}: {}",
                    bundled_file.get_print_path(),
                    e
                );
                return None;
            }
            let test_report = bin_parse(&buffer);
            if let Ok(test_report) = test_report {
                let num_tests = test_report
                    .test_results
                    .iter()
                    .fold(0, |acc, test_result| acc + test_result.test_case_runs.len());
                Some(num_tests)
            } else {
                None
            }
        })
        .sum::<usize>();
    junit_num_tests + internal_num_tests
}

#[cfg(test)]
mod tests {

    use bundle::BundleMetaDebugProps;
    #[cfg(target_os = "macos")]
    use context::repo::RepoUrlParts;

    use crate::context::{coalesce_junit_path_wrappers, gather_initial_test_context};
    use crate::upload_command::UploadArgs;

    #[test]
    fn test_variant_truncation() {
        let mut upload_args = UploadArgs::new(
            "test-token".to_string(),
            "test-org".to_string(),
            vec![],
            None,
            true,
        );

        // Test case 1: Variant under 64 characters
        upload_args.variant = Some("short-variant".to_string());
        let context = gather_initial_test_context(
            upload_args.clone(),
            BundleMetaDebugProps {
                command_line: "test".to_string(),
            },
        )
        .unwrap();
        assert_eq!(context.meta.variant, Some("short-variant".to_string()));

        // Test case 2: Variant exactly 64 characters
        let long_variant = "a".repeat(64);
        upload_args.variant = Some(long_variant.clone());
        let context = gather_initial_test_context(
            upload_args.clone(),
            BundleMetaDebugProps {
                command_line: "test".to_string(),
            },
        )
        .unwrap();
        assert_eq!(context.meta.variant, Some(long_variant));

        // Test case 3: Variant over 64 characters
        let very_long_variant = "a".repeat(100);
        upload_args.variant = Some(very_long_variant.clone());
        let context = gather_initial_test_context(
            upload_args,
            BundleMetaDebugProps {
                command_line: "test".to_string(),
            },
        )
        .unwrap();
        assert_eq!(
            context.meta.variant,
            Some(very_long_variant[..64].to_string())
        );
    }

    #[test]
    fn test_coalesce_junit_path_wrappers() {
        #[cfg(target_os = "macos")]
        let repo = RepoUrlParts {
            host: "github.com".to_string(),
            owner: "trunk-io".to_string(),
            name: "analytics-cli".to_string(),
        };
        let result_err = coalesce_junit_path_wrappers(
            vec!["test".into()],
            Some("test".into()),
            #[cfg(target_os = "macos")]
            Some("test".into()),
            #[cfg(target_os = "macos")]
            &repo,
            #[cfg(target_os = "macos")]
            "test".into(),
            #[cfg(target_os = "macos")]
            false,
            Vec::new(),
            false,
        );
        assert!(result_err.is_err());
        let result_ok = coalesce_junit_path_wrappers(
            vec!["test".into()],
            Some("test".into()),
            #[cfg(target_os = "macos")]
            Some("test".into()),
            #[cfg(target_os = "macos")]
            &repo,
            #[cfg(target_os = "macos")]
            "test".into(),
            #[cfg(target_os = "macos")]
            false,
            Vec::new(),
            true,
        );
        assert!(result_ok.is_ok());
        let result = result_ok.unwrap();
        assert_eq!(result.0.len(), 1);
        let junit_result = &result.0[0];
        assert_eq!(junit_result.junit_path, "test");
        assert!(result.1.is_none());
        assert!(result.2.is_none());
    }

    #[test]
    fn test_coalesce_junit_path_wrappers_with_test_reports() {
        #[cfg(target_os = "macos")]
        let repo = RepoUrlParts {
            host: "github.com".to_string(),
            owner: "trunk-io".to_string(),
            name: "analytics-cli".to_string(),
        };
        let result_ok = coalesce_junit_path_wrappers(
            Vec::new(),
            None,
            #[cfg(target_os = "macos")]
            None,
            #[cfg(target_os = "macos")]
            &repo,
            #[cfg(target_os = "macos")]
            "test".into(),
            #[cfg(target_os = "macos")]
            false,
            vec!["test".into()],
            true,
        );
        assert!(result_ok.is_ok());
        let result = result_ok.unwrap();
        assert_eq!(result.0.len(), 1);
        let junit_result = &result.0[0];
        assert_eq!(junit_result.junit_path, "test");
        assert!(result.1.is_none());
        assert!(result.2.is_none());
    }

    #[test]
    fn test_gather_debug_props() {
        let args: Vec<String> = vec!["trunk flakytests".into(), "test".into(), "--token".into()];
        let debug_props = super::gather_debug_props(args, "token".to_string());
        assert_eq!(debug_props.command_line, "trunk flakytests test");
        let args: Vec<String> = vec![
            "trunk flakytests".into(),
            "test".into(),
            "--token=token".into(),
        ];
        let debug_props = super::gather_debug_props(args, "token".to_string());
        assert_eq!(debug_props.command_line, "trunk flakytests test");

        let args: Vec<String> = vec![
            "trunk flakytests".into(),
            "test".into(),
            "--token token".into(),
        ];
        let debug_props = super::gather_debug_props(args, "token".to_string());
        assert_eq!(debug_props.command_line, "trunk flakytests test");

        let args: Vec<String> = vec!["trunk flakytests".into(), "test".into()];
        let debug_props = super::gather_debug_props(args, "token".to_string());
        assert_eq!(debug_props.command_line, "trunk flakytests test");

        let args: Vec<String> = vec!["trunk flakytests".into(), "token".into()];
        let debug_props = super::gather_debug_props(args, "token".to_string());
        assert_eq!(debug_props.command_line, "trunk flakytests");
    }
}
