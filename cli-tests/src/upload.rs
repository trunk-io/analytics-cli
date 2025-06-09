use std::io::Write;
use std::sync::{Arc, Mutex};
use std::{fs, io::BufReader};

use api::message::{
    CreateBundleUploadRequest, CreateBundleUploadResponse, GetQuarantineConfigRequest,
    GetQuarantineConfigResponse,
};
use assert_matches::assert_matches;
use axum::{extract::State, http::StatusCode, Json};
use bundle::{BundleMeta, FileSetType, INTERNAL_BIN_FILENAME};
use codeowners::CodeOwners;
use constants::EXIT_FAILURE;
use context::{
    bazel_bep::parser::BazelBepParser,
    junit::{junit_path::JunitReportStatus, parser::JunitParser},
    repo::{BundleRepo, RepoUrlParts as Repo},
};
use lazy_static::lazy_static;
use predicates::prelude::*;
use pretty_assertions::assert_eq;
use prost::Message;
use tempfile::{tempdir, TempDir};
use test_utils::{
    inputs::get_test_file_path,
    mock_server::{MockServerBuilder, RequestPayload, SharedMockServerState},
};

use crate::command_builder::CommandBuilder;
use crate::utils::{
    generate_mock_bazel_bep, generate_mock_codeowners, generate_mock_git_repo,
    generate_mock_valid_junit_xmls,
};

// NOTE: must be multi threaded to start a mock server
#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle() {
    let temp_dir = TempDir::with_prefix("not-hidden").unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(temp_dir.path());
    generate_mock_codeowners(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let command_builder = CommandBuilder::upload(temp_dir.path(), state.host.clone());

    let assert = command_builder
        .command()
        .assert()
        // should fail due to quarantine and succeed without quarantining
        .failure();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 4);
    let mut requests_iter = requests.into_iter();

    let quarantine_request = requests_iter.next().unwrap();
    assert_matches!(quarantine_request, RequestPayload::GetQuarantineBulkTestStatus(req) => {
        assert_eq!(req.repo.host, "github.com");
        assert_eq!(req.repo.owner, "trunk-io");
        assert_eq!(req.repo.name, "analytics-cli");
        assert_eq!(req.org_url_slug, "test-org");
        assert!(
            !req.test_identifiers.is_empty(),
            "test_identifiers should not be empty"
        );
        for test in &req.test_identifiers {
            assert!(!test.name.is_empty(), "Test name should not be empty");
            assert!(
                !test.parent_name.is_empty(),
                "Parent name should not be empty"
            );
            assert!(test.id.len() == 36, "Test ID should be a valid UUID");
        }
    });

    let upload_request = assert_matches!(requests_iter.next().unwrap(), RequestPayload::CreateBundleUpload(ur) => ur);
    assert_eq!(
        upload_request.repo,
        Repo {
            host: String::from("github.com"),
            owner: String::from("trunk-io"),
            name: String::from("analytics-cli"),
        }
    );
    assert_eq!(upload_request.org_url_slug, "test-org");
    assert!(upload_request
        .client_version
        .starts_with("trunk-analytics-cli cargo="));
    assert!(upload_request.client_version.contains(" git="));
    assert!(upload_request.client_version.contains(" rustc="));

    let tar_extract_directory =
        assert_matches!(requests_iter.next().unwrap(), RequestPayload::S3Upload(d) => d);

    let file = fs::File::open(tar_extract_directory.join("meta.json")).unwrap();
    let reader = BufReader::new(file);
    let bundle_meta: BundleMeta = serde_json::from_reader(reader).unwrap();
    let base_props = bundle_meta.base_props;
    let junit_props = bundle_meta.junit_props;
    let debug_props = bundle_meta.debug_props;

    assert_eq!(base_props.org, "test-org");
    assert_eq!(
        base_props.repo.repo,
        Repo {
            host: String::from("github.com"),
            owner: String::from("trunk-io"),
            name: String::from("analytics-cli"),
        }
    );
    assert_eq!(
        base_props.repo.repo_url,
        "https://github.com/trunk-io/analytics-cli.git"
    );
    assert!(!base_props.repo.repo_head_sha.is_empty());
    let repo_head_sha_short = base_props.repo.repo_head_sha_short.unwrap();
    assert!(!repo_head_sha_short.is_empty());
    assert!(&repo_head_sha_short.len() < &base_props.repo.repo_head_sha.len());
    assert!(base_props
        .repo
        .repo_head_sha
        .starts_with(&repo_head_sha_short));
    assert_eq!(base_props.repo.repo_head_branch, "refs/heads/trunk/test");
    assert_eq!(base_props.repo.repo_head_author_name, "Your Name");
    assert_eq!(
        base_props.repo.repo_head_author_email,
        "your.email@example.com"
    );
    assert_eq!(base_props.bundle_upload_id, "test-bundle-upload-id");
    assert_eq!(base_props.tags, &[]);
    assert_eq!(base_props.file_sets.len(), 1);
    assert_eq!(junit_props.num_files, 1);
    assert_eq!(junit_props.num_tests, 500);
    assert_eq!(base_props.envs.get("CI"), Some(&String::from("1")));
    assert_eq!(
        base_props.envs.get("GITHUB_JOB"),
        Some(&String::from("test-job"))
    );
    let time_since_upload = chrono::Utc::now()
        - chrono::DateTime::from_timestamp(base_props.upload_time_epoch as i64, 0).unwrap();
    more_asserts::assert_lt!(time_since_upload.num_minutes(), 5);
    assert_eq!(base_props.test_command, None);
    assert!(base_props.os_info.is_some());
    assert!(base_props.quarantined_tests.is_empty());
    assert_eq!(
        base_props.codeowners,
        Some(CodeOwners {
            path: temp_dir.as_ref().join("CODEOWNERS").canonicalize().unwrap(),
            owners: None,
        })
    );

    let file_set = base_props.file_sets.first().unwrap();
    assert_eq!(file_set.file_set_type, FileSetType::Junit);
    assert_eq!(file_set.glob, "./*");
    assert_eq!(file_set.files.len(), 1);

    let bundled_file = file_set.files.first().unwrap();
    assert_eq!(bundled_file.path, "junit/0");
    assert!(
        fs::File::open(tar_extract_directory.join(&bundled_file.path))
            .unwrap()
            .metadata()
            .unwrap()
            .is_file()
    );
    assert_eq!(bundled_file.owners, ["@user", "@user2"]);
    assert_eq!(bundled_file.team, None);

    // Verify internal bundled file contents
    let internal_bundled_file = bundle_meta.internal_bundled_file.as_ref().unwrap();
    assert_eq!(internal_bundled_file.path, INTERNAL_BIN_FILENAME);
    assert_eq!(internal_bundled_file.owners.len(), 0);
    assert_eq!(internal_bundled_file.team, None);

    let bin = fs::read(tar_extract_directory.join(&internal_bundled_file.path)).unwrap();
    let report = proto::test_context::test_run::TestResult::decode(&*bin).unwrap();

    assert_eq!(report.test_case_runs.len(), 500);
    let test_case_run = &report.test_case_runs[0];
    assert!(test_case_run.id.is_empty());
    assert!(!test_case_run.name.is_empty());
    assert!(!test_case_run.classname.is_empty());
    assert!(!test_case_run.file.is_empty());
    assert!(!test_case_run.parent_name.is_empty());
    assert_eq!(test_case_run.line, 0);
    assert_eq!(test_case_run.attempt_number, 0);
    assert!(test_case_run.started_at.is_some());
    assert!(test_case_run.finished_at.is_some());
    assert!(!test_case_run.is_quarantined);
    assert_eq!(test_case_run.codeowners.len(), 2);
    assert_eq!(test_case_run.codeowners[0].name, "@user");
    assert_eq!(test_case_run.codeowners[1].name, "@user2");

    assert!(debug_props.command_line.ends_with(
        &command_builder
            .build_args()
            .join(" ")
            .replace("test-token", "")
            .replace("--token", "")
            .trim()
    ));

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_using_bep() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    let bep_path = generate_mock_bazel_bep(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .bazel_bep_path(bep_path.to_str().unwrap())
        .command()
        .assert()
        .success();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 3);
    assert_matches!(requests[0], RequestPayload::CreateBundleUpload(_));
    let tar_extract_directory = assert_matches!(&requests[1], RequestPayload::S3Upload(d) => d);
    assert_matches!(requests[2], RequestPayload::TelemetryUploadMetrics(_));

    let meta_json = fs::File::open(tar_extract_directory.join("meta.json")).unwrap();
    let bundle_meta: BundleMeta = serde_json::from_reader(meta_json).unwrap();

    assert!(!bundle_meta.base_props.file_sets.is_empty());
    bundle_meta
        .base_props
        .file_sets
        .iter()
        .for_each(|file_set| {
            assert_eq!(file_set.resolved_status, Some(JunitReportStatus::Passed));
            assert_eq!(file_set.file_set_type, FileSetType::Junit);
            let mut junit_parser = JunitParser::new();
            file_set.files.iter().for_each(|file| {
                let junit_file = fs::File::open(tar_extract_directory.join(&file.path)).unwrap();
                assert!(junit_parser.parse(BufReader::new(junit_file)).is_ok());
                assert!(junit_parser.issues().is_empty());
            });
        });

    let mut bazel_bep_parser = BazelBepParser::new(tar_extract_directory.join("bazel_bep.json"));
    let parse_result = bazel_bep_parser.parse().ok().unwrap();
    assert!(parse_result.errors.is_empty());
    assert_eq!(parse_result.xml_file_counts(), (1, 0));

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_success_status_code() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    let test_bep_path = get_test_file_path("test_fixtures/bep_retries");
    let uri_fail = format!(
        "file://{}",
        get_test_file_path("../cli/test_fixtures/junit1_fail.xml")
    );
    let uri_pass = format!(
        "file://{}",
        get_test_file_path("../cli/test_fixtures/junit0_pass.xml")
    );

    let bep_content = fs::read_to_string(&test_bep_path)
        .unwrap()
        .replace("${URI_FAIL}", &uri_fail)
        .replace("${URI_PASS}", &uri_pass);
    let bep_path = temp_dir.path().join("bep.json");
    fs::write(&bep_path, bep_content).unwrap();

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .bazel_bep_path(bep_path.to_str().unwrap())
        .command()
        .assert()
        .code(0)
        .success();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 3);
    assert_matches!(requests[0], RequestPayload::CreateBundleUpload(_));
    let tar_extract_directory = assert_matches!(&requests[1], RequestPayload::S3Upload(d) => d);
    assert_matches!(requests[2], RequestPayload::TelemetryUploadMetrics(_));

    let mut bazel_bep_parser = BazelBepParser::new(tar_extract_directory.join("bazel_bep.json"));
    let parse_result = bazel_bep_parser.parse().ok().unwrap();
    assert_eq!(parse_result.test_results.len(), 2);

    let meta_json = fs::File::open(tar_extract_directory.join("meta.json")).unwrap();
    let bundle_meta: BundleMeta = serde_json::from_reader(meta_json).unwrap();

    assert!(!bundle_meta.base_props.file_sets.is_empty());
    bundle_meta
        .base_props
        .file_sets
        .iter()
        .for_each(|file_set| {
            assert_eq!(file_set.file_set_type, FileSetType::Junit);
            let mut junit_parser = JunitParser::new();
            file_set.files.iter().for_each(|file| {
                let junit_file = fs::File::open(tar_extract_directory.join(&file.path)).unwrap();
                assert!(junit_parser.parse(BufReader::new(junit_file)).is_ok());
                assert!(junit_parser.issues().is_empty());
            });
            assert_eq!(file_set.resolved_status, Some(JunitReportStatus::Flaky));
        });

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn falls_back_to_binary_file() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    let test_bep_path = get_test_file_path("test_fixtures/bep_binary_file.bin");

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .bazel_bep_path(test_bep_path.as_str())
        // verbose output to see the tracing log
        .verbose(true)
        .command()
        .assert()
        .success();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 3);
    assert_matches!(requests[0], RequestPayload::CreateBundleUpload(_));
    let tar_extract_directory = assert_matches!(&requests[1], RequestPayload::S3Upload(d) => d);
    assert_matches!(requests[2], RequestPayload::TelemetryUploadMetrics(_));

    let mut bazel_bep_parser = BazelBepParser::new(tar_extract_directory.join("bazel_bep.json"));
    let parse_result = bazel_bep_parser.parse().ok().unwrap();
    assert_eq!(parse_result.test_results.len(), 8);

    let meta_json = fs::File::open(tar_extract_directory.join("meta.json")).unwrap();
    assert!(serde_json::from_reader::<fs::File, BundleMeta>(meta_json).is_ok());

    assert.stdout(predicate::str::contains(
        "Attempting to parse bep file as binary",
    ));
}

// same test as upload_bundle_success_status_code but with a previous exit code set
#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_success_preceding_failure() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    let test_bep_path = get_test_file_path("test_fixtures/bep_retries");
    let uri_fail = format!(
        "file://{}",
        get_test_file_path("../cli/test_fixtures/junit1_fail.xml")
    );
    let uri_pass = format!(
        "file://{}",
        get_test_file_path("../cli/test_fixtures/junit0_pass.xml")
    );

    let bep_content = fs::read_to_string(&test_bep_path)
        .unwrap()
        .replace("${URI_FAIL}", &uri_fail)
        .replace("${URI_PASS}", &uri_pass);
    let bep_path = temp_dir.path().join("bep.json");
    fs::write(&bep_path, bep_content).unwrap();

    let state = MockServerBuilder::new().spawn_mock_server().await;
    let test_process_exit_code = 127;

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .bazel_bep_path(bep_path.to_str().unwrap())
        .test_process_exit_code(test_process_exit_code)
        .command()
        .assert()
        .code(test_process_exit_code)
        .failure();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 3);
    assert_matches!(requests[0], RequestPayload::CreateBundleUpload(_));
    let tar_extract_directory = assert_matches!(&requests[1], RequestPayload::S3Upload(d) => d);
    assert_matches!(requests[2], RequestPayload::TelemetryUploadMetrics(_));

    let mut bazel_bep_parser = BazelBepParser::new(tar_extract_directory.join("bazel_bep.json"));
    let parse_result = bazel_bep_parser.parse().ok().unwrap();
    assert_eq!(parse_result.test_results.len(), 2);

    let meta_json = fs::File::open(tar_extract_directory.join("meta.json")).unwrap();
    let bundle_meta: BundleMeta = serde_json::from_reader(meta_json).unwrap();

    assert!(!bundle_meta.base_props.file_sets.is_empty());
    bundle_meta
        .base_props
        .file_sets
        .iter()
        .for_each(|file_set| {
            assert_eq!(file_set.file_set_type, FileSetType::Junit);
            let mut junit_parser = JunitParser::new();
            file_set.files.iter().for_each(|file| {
                let junit_file = fs::File::open(tar_extract_directory.join(&file.path)).unwrap();
                assert!(junit_parser.parse(BufReader::new(junit_file)).is_ok());
                assert!(junit_parser.issues().is_empty());
            });
            assert_eq!(file_set.resolved_status, Some(JunitReportStatus::Flaky));
        });

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_empty_junit_paths() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .junit_paths("")
        .command()
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "error: a value is required for '--junit-paths <JUNIT_PATHS>' but none was supplied",
        ));

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_no_files_allow_missing_junit_files() {
    enum Flag {
        Long,
        LongWithEquals,
        Alias,
        AliasWithEquals,
        Default,
        Off,
        OffWithEquals,
        OffAlias,
        OffAliasWithEquals,
    }

    for flag in [
        Flag::Long,
        Flag::LongWithEquals,
        Flag::Alias,
        Flag::AliasWithEquals,
        Flag::Default,
        Flag::Off,
        Flag::OffWithEquals,
        Flag::OffAlias,
        Flag::OffAliasWithEquals,
    ] {
        let temp_dir = tempdir().unwrap();
        generate_mock_git_repo(&temp_dir);

        let state = MockServerBuilder::new().spawn_mock_server().await;

        let mut command = CommandBuilder::upload(temp_dir.path(), state.host.clone()).command();

        match flag {
            Flag::Long => {
                command.arg("--allow-empty-test-results");
            }
            Flag::LongWithEquals => {
                command.arg("--allow-empty-test-results=true");
            }
            Flag::Alias => {
                command.arg("--allow-missing-junit-files");
            }
            Flag::AliasWithEquals => {
                command.arg("--allow-missing-junit-files=true");
            }
            Flag::Default => (),
            Flag::Off => {
                command.arg("--allow-empty-test-results");
                command.arg("false");
            }
            Flag::OffWithEquals => {
                command.arg("--allow-empty-test-results=false");
            }
            Flag::OffAlias => {
                command.arg("--allow-missing-junit-files");
                command.arg("false");
            }
            Flag::OffAliasWithEquals => {
                command.arg("--allow-missing-junit-files=false");
            }
        };

        let mut assert = command.assert();

        assert = if matches!(
            flag,
            Flag::Off | Flag::OffWithEquals | Flag::OffAlias | Flag::OffAliasWithEquals
        ) {
            assert.failure()
        } else {
            assert.success()
        };

        let predicate_fn = predicate::str::contains("unexpected argument");

        // `=` is required to set the flag to `false`
        assert = if matches!(flag, Flag::Off | Flag::OffAlias) {
            assert.stderr(predicate_fn)
        } else {
            assert.stderr(predicate_fn.not())
        };

        // HINT: View CLI output with `cargo test -- --nocapture`
        println!("{assert}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_invalid_repo_root() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .repo_root("../")
        .command()
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "\"../\" does not appear to be a git repository",
        ));
    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 0);

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_invalid_repo_root_explicit() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;
    // make a child directory to upload from
    let child_path = temp_dir.path().join("child_dir");
    fs::create_dir(&child_path).unwrap();

    let assert = CommandBuilder::upload(&child_path, state.host.clone())
        .repo_root(child_path.to_str().unwrap())
        .command()
        .assert()
        .failure()
        .stderr(predicate::str::contains(format!(
            "\"{}\" does not appear to be a git repository",
            child_path.to_str().unwrap()
        )));
    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 0);

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_valid_repo_root_implicit() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;
    // make a child directory to upload from
    let child_path = temp_dir.path().join("child_dir");
    fs::create_dir(&child_path).unwrap();

    let assert = CommandBuilder::upload(&child_path, state.host.clone())
        .command()
        .assert()
        .success();

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_when_server_down() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);

    let assert = CommandBuilder::upload(temp_dir.path(), String::from("https://localhost:10"))
        .command()
        .assert()
        .success();

    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_with_no_junit_files_no_quarantine_successful_upload() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .command()
        .assert()
        .code(0)
        .success()
        .stderr(predicate::str::contains(
            "No tests were found in the provided test results",
        ));

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn quarantines_tests_regardless_of_upload() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let mut mock_server_builder = MockServerBuilder::new();

    lazy_static! {
        static ref QUARANTINE_CONFIG_RESPONSE: Arc<Mutex<QuarantineConfigResponse>> =
            Arc::new(Mutex::new(QuarantineConfigResponse::None));
    }
    #[derive(Debug, Clone, Copy)]
    enum QuarantineConfigResponse {
        Disabled,
        None,
        Some,
        All,
    }
    mock_server_builder.set_get_quarantining_config_handler(
        |Json(get_quarantine_bulk_test_status_request): Json<GetQuarantineConfigRequest>| {
            let mut test_ids = get_quarantine_bulk_test_status_request
                .test_identifiers
                .into_iter()
                .map(|t| t.id)
                .collect::<Vec<_>>();
            let quarantine_config_response = *QUARANTINE_CONFIG_RESPONSE.lock().unwrap();
            let quarantined_tests = match quarantine_config_response {
                QuarantineConfigResponse::Disabled => Vec::new(),
                QuarantineConfigResponse::None => Vec::new(),
                QuarantineConfigResponse::Some => test_ids.split_off(1),
                QuarantineConfigResponse::All => test_ids,
            };
            let is_disabled = matches!(
                quarantine_config_response,
                QuarantineConfigResponse::Disabled
            );
            async move {
                Json(GetQuarantineConfigResponse {
                    is_disabled,
                    quarantined_tests,
                })
            }
        },
    );
    #[derive(Debug, Clone, Copy)]
    enum CreateBundleResponse {
        Error,
        Success,
    }
    lazy_static! {
        static ref CREATE_BUNDLE_RESPONSE: Arc<Mutex<CreateBundleResponse>> =
            Arc::new(Mutex::new(CreateBundleResponse::Error));
    }
    mock_server_builder.set_create_bundle_handler(
        |State(state): State<SharedMockServerState>, _: Json<CreateBundleUploadRequest>| {
            let create_bundle_response = *CREATE_BUNDLE_RESPONSE.lock().unwrap();
            let result = match create_bundle_response {
                CreateBundleResponse::Error => Err(String::from("Server is down")),
                CreateBundleResponse::Success => {
                    let host = &state.host;
                    Ok(Json(CreateBundleUploadResponse {
                        id: String::from("test-bundle-upload-id"),
                        id_v2: String::from("test-bundle-upload-id-v2"),
                        url: format!("{host}/s3upload"),
                        key: String::from("unused"),
                    }))
                }
            };
            async { result }
        },
    );
    let state = mock_server_builder.spawn_mock_server().await;

    let mut command = CommandBuilder::upload(temp_dir.path(), state.host.clone()).command();

    // First run won't quarantine any tests
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::None;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Error;
    command.assert().failure();

    // Second run quarantines all, but 1 test
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::Some;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Error;
    command.assert().failure();

    // Third run will not quarantine all tests because of upload failure
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::All;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Error;
    command.assert().failure();

    // Fourth run will quarantine all tests, and upload them
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::All;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Success;
    command.assert().success();

    // Fifth run will run with quarantining disabled, but will fail to upload
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::Disabled;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Error;
    command.assert().failure();

    // Sixth run will run with quarantining disabled, and will succeed with upload
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::Disabled;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Success;
    command.assert().success();
}

#[tokio::test(flavor = "multi_thread")]
async fn is_ok_on_unauthorized() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let mut mock_server_builder = MockServerBuilder::new();

    mock_server_builder.set_get_quarantining_config_handler(
        |Json(_): Json<GetQuarantineConfigRequest>| async {
            Err::<Json<GetQuarantineConfigResponse>, StatusCode>(StatusCode::UNAUTHORIZED)
        },
    );

    mock_server_builder.set_create_bundle_handler(
        |State(_): State<SharedMockServerState>, _: Json<CreateBundleUploadRequest>| async {
            Err::<Json<CreateBundleUploadResponse>, StatusCode>(StatusCode::UNAUTHORIZED)
        },
    );
    let state = mock_server_builder.spawn_mock_server().await;

    let mut command = CommandBuilder::upload(temp_dir.path(), state.host.clone()).command();

    command
        .assert()
        .failure()
        .stdout(predicate::str::contains("error: ").not());
}

#[tokio::test(flavor = "multi_thread")]
async fn is_ok_on_forbidden() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let mut mock_server_builder = MockServerBuilder::new();

    mock_server_builder.set_get_quarantining_config_handler(
        |Json(_): Json<GetQuarantineConfigRequest>| async {
            Err::<Json<GetQuarantineConfigResponse>, StatusCode>(StatusCode::FORBIDDEN)
        },
    );

    mock_server_builder.set_create_bundle_handler(
        |State(_): State<SharedMockServerState>, _: Json<CreateBundleUploadRequest>| async {
            Err::<Json<CreateBundleUploadResponse>, StatusCode>(StatusCode::FORBIDDEN)
        },
    );
    let state = mock_server_builder.spawn_mock_server().await;

    let mut command = CommandBuilder::upload(temp_dir.path(), state.host.clone()).command();

    command
        .assert()
        .failure()
        .stdout(predicate::str::contains("error: ").not());
}

#[tokio::test(flavor = "multi_thread")]
async fn is_not_ok_on_bad_request() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let mut mock_server_builder = MockServerBuilder::new();

    mock_server_builder.set_get_quarantining_config_handler(
        |Json(_): Json<GetQuarantineConfigRequest>| async {
            Err::<Json<GetQuarantineConfigResponse>, StatusCode>(StatusCode::BAD_REQUEST)
        },
    );

    mock_server_builder.set_create_bundle_handler(
        |State(_): State<SharedMockServerState>, _: Json<CreateBundleUploadRequest>| async {
            Err::<Json<CreateBundleUploadResponse>, StatusCode>(StatusCode::BAD_REQUEST)
        },
    );
    let state = mock_server_builder.spawn_mock_server().await;

    let mut command = CommandBuilder::upload(temp_dir.path(), state.host.clone()).command();

    command
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[tokio::test(flavor = "multi_thread")]
async fn telemetry_upload_metrics_on_upload_failure() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);

    let mut mock_server_builder = MockServerBuilder::new();
    mock_server_builder.set_create_bundle_handler(
        |State(_): State<SharedMockServerState>, _: Json<CreateBundleUploadRequest>| async {
            Err::<Json<CreateBundleUploadResponse>, StatusCode>(StatusCode::BAD_REQUEST)
        },
    );
    let state = mock_server_builder.spawn_mock_server().await;

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .disable_quarantining(true)
        .command()
        .assert()
        .failure();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);

    let telemetry_request =
        assert_matches!(requests.last().unwrap(), RequestPayload::TelemetryUploadMetrics(ur) => ur);
    let telemetry_request_repo = telemetry_request.repo.clone().unwrap();
    assert!(telemetry_request.failed);
    assert_eq!(telemetry_request_repo.host, "github.com");
    assert_eq!(telemetry_request_repo.owner, "trunk-io");
    assert_eq!(telemetry_request_repo.name, "analytics-cli");
    assert_eq!(telemetry_request.failure_reason, "400_bad_request");

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn telemetry_upload_metrics_on_upload_success() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);

    let mock_server_builder = MockServerBuilder::new();
    let state = mock_server_builder.spawn_mock_server().await;

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .disable_quarantining(true)
        .command()
        .assert()
        .success();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 3);

    let telemetry_request =
        assert_matches!(requests.last().unwrap(), RequestPayload::TelemetryUploadMetrics(ur) => ur);
    let telemetry_request_repo = telemetry_request.repo.clone().unwrap();
    assert!(!telemetry_request.failed);
    assert_eq!(telemetry_request_repo.host, "github.com");
    assert_eq!(telemetry_request_repo.owner, "trunk-io");
    assert_eq!(telemetry_request_repo.name, "analytics-cli");
    assert_eq!(telemetry_request.failure_reason, "");

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn telemetry_does_not_impact_return() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);

    let mut mock_server_builder = MockServerBuilder::new();
    mock_server_builder.set_telemetry_upload_metrics_handler(
        |State(_state): State<SharedMockServerState>, _: String| async { String::from("Err") },
    );
    let state = mock_server_builder.spawn_mock_server().await;

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .disable_quarantining(true)
        .command()
        .assert()
        .success();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 2);
    assert_matches!(requests[0], RequestPayload::CreateBundleUpload(_));
    assert_matches!(requests[1], RequestPayload::S3Upload(_));

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_variant_propagation() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .variant("test-variant")
        .command()
        .assert()
        .failure();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 4);
    let mut requests_iter = requests.into_iter();

    assert_matches!(
        requests_iter.next().unwrap(),
        RequestPayload::GetQuarantineBulkTestStatus(_)
    );

    assert_matches!(
        requests_iter.next().unwrap(),
        RequestPayload::CreateBundleUpload(_)
    );

    let tar_extract_directory =
        assert_matches!(requests_iter.next().unwrap(), RequestPayload::S3Upload(d) => d);

    let file = fs::File::open(tar_extract_directory.join("meta.json")).unwrap();
    let reader = BufReader::new(file);
    let bundle_meta: BundleMeta = serde_json::from_reader(reader).unwrap();
    assert_eq!(bundle_meta.variant, Some("test-variant".to_string()));

    assert_matches!(
        requests_iter.next().unwrap(),
        RequestPayload::TelemetryUploadMetrics(_)
    );

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_can_upload_with_uncloned_repo() {
    let temp_dir = tempdir().unwrap();
    generate_mock_codeowners(&temp_dir);

    let test_bep_path = get_test_file_path("test_fixtures/bep_retries");
    let uri_fail = format!(
        "file://{}",
        get_test_file_path("../cli/test_fixtures/junit0_fail.xml")
    );
    let uri_pass = format!(
        "file://{}",
        get_test_file_path("../cli/test_fixtures/junit0_pass.xml")
    );

    let bep_content = fs::read_to_string(&test_bep_path)
        .unwrap()
        .replace("${URI_FAIL}", &uri_fail)
        .replace("${URI_PASS}", &uri_pass);
    let bep_path = temp_dir.path().join("bep.json");
    fs::write(&bep_path, bep_content).unwrap();

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let repo_url = "https://github.com/my-org/my-repo";
    let sha = "1234567890abcde";
    let branch = "my-branch";
    let epoch: i64 = 12341432;
    let author_name = "my-gh-username";

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .bazel_bep_path(bep_path.to_str().unwrap())
        .use_uncloned_repo(true)
        .repo_url(repo_url)
        .repo_head_sha(sha)
        .repo_head_branch(branch)
        .repo_head_commit_epoch(epoch.to_string().as_str())
        .repo_head_author_name(author_name)
        .command()
        .assert()
        .success();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 3);
    let mut requests_iter = requests.into_iter();

    assert_matches!(
        requests_iter.next().unwrap(),
        RequestPayload::CreateBundleUpload(_)
    );

    let tar_extract_directory =
        assert_matches!(requests_iter.next().unwrap(), RequestPayload::S3Upload(d) => d);

    let file = fs::File::open(tar_extract_directory.join("meta.json")).unwrap();
    let reader = BufReader::new(file);
    let bundle_meta: BundleMeta = serde_json::from_reader(reader).unwrap();

    let expected_repo_root = String::from(
        fs::canonicalize(temp_dir.path())
            .expect("Could not canonicalize temp dir")
            .as_os_str()
            .to_str()
            .unwrap(),
    );
    let expected = BundleRepo {
        repo: Repo {
            host: String::from("github.com"),
            owner: String::from("my-org"),
            name: String::from("my-repo"),
        },
        repo_root: expected_repo_root,
        repo_url: String::from(repo_url),
        repo_head_sha: String::from(sha),
        repo_head_sha_short: Some(String::from("1234567")),
        repo_head_branch: String::from(branch),
        repo_head_commit_epoch: epoch,
        repo_head_commit_message: String::from(""),
        repo_head_author_name: String::from(author_name),
        repo_head_author_email: String::from(""),
    };
    assert_eq!(bundle_meta.base_props.repo, expected);

    assert_matches!(
        requests_iter.next().unwrap(),
        RequestPayload::TelemetryUploadMetrics(_)
    );

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_uncloned_repo_requires_manual_settings() {
    let temp_dir = tempdir().unwrap();
    generate_mock_codeowners(&temp_dir);

    let test_bep_path = get_test_file_path("test_fixtures/bep_retries");
    let uri_fail = format!(
        "file://{}",
        get_test_file_path("../cli/test_fixtures/junit0_fail.xml")
    );
    let uri_pass = format!(
        "file://{}",
        get_test_file_path("../cli/test_fixtures/junit0_pass.xml")
    );

    let bep_content = fs::read_to_string(&test_bep_path)
        .unwrap()
        .replace("${URI_FAIL}", &uri_fail)
        .replace("${URI_PASS}", &uri_pass);
    let bep_path = temp_dir.path().join("bep.json");
    fs::write(&bep_path, bep_content).unwrap();

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .bazel_bep_path(bep_path.to_str().unwrap())
        .use_uncloned_repo(true)
        .command()
        .assert()
        .code(predicate::eq(2))
        .failure();

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_uncloned_repo_conflicts_with_repo_root() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let test_bep_path = get_test_file_path("test_fixtures/bep_retries");
    let uri_fail = format!(
        "file://{}",
        get_test_file_path("../cli/test_fixtures/junit0_fail.xml")
    );
    let uri_pass = format!(
        "file://{}",
        get_test_file_path("../cli/test_fixtures/junit0_pass.xml")
    );

    let bep_content = fs::read_to_string(&test_bep_path)
        .unwrap()
        .replace("${URI_FAIL}", &uri_fail)
        .replace("${URI_PASS}", &uri_pass);
    let bep_path = temp_dir.path().join("bep.json");
    fs::write(&bep_path, bep_content).unwrap();

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let repo_url = "https://github.com/my-org/my-repo";
    let sha = "1234567890abcde";
    let branch = "my-branch";
    let epoch: i64 = 12341432;
    let author_name = "my-gh-username";

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .bazel_bep_path(bep_path.to_str().unwrap())
        .use_uncloned_repo(true)
        .repo_root("./")
        .repo_url(repo_url)
        .repo_head_sha(sha)
        .repo_head_branch(branch)
        .repo_head_commit_epoch(epoch.to_string().as_str())
        .repo_head_author_name(author_name)
        .command()
        .assert()
        .code(predicate::eq(2))
        .failure();

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_can_use_manual_overrides_on_cloned_repo() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let test_bep_path = get_test_file_path("test_fixtures/bep_retries");
    let uri_fail = format!(
        "file://{}",
        get_test_file_path("../cli/test_fixtures/junit0_fail.xml")
    );
    let uri_pass = format!(
        "file://{}",
        get_test_file_path("../cli/test_fixtures/junit0_pass.xml")
    );

    let bep_content = fs::read_to_string(&test_bep_path)
        .unwrap()
        .replace("${URI_FAIL}", &uri_fail)
        .replace("${URI_PASS}", &uri_pass);
    let bep_path = temp_dir.path().join("bep.json");
    fs::write(&bep_path, bep_content).unwrap();

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let repo_url = "https://github.com/my-org/my-repo";
    let sha = "1234567890abcde";
    let branch = "my-branch";
    let epoch: i64 = 12341432;
    let author_name = "my-gh-username";

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .bazel_bep_path(bep_path.to_str().unwrap())
        .repo_url(repo_url)
        .repo_head_sha(sha)
        .repo_head_branch(branch)
        .repo_head_commit_epoch(epoch.to_string().as_str())
        .repo_head_author_name(author_name)
        .command()
        .assert()
        .success();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 3);
    let mut requests_iter = requests.into_iter();

    assert_matches!(
        requests_iter.next().unwrap(),
        RequestPayload::CreateBundleUpload(_)
    );

    let tar_extract_directory =
        assert_matches!(requests_iter.next().unwrap(), RequestPayload::S3Upload(d) => d);

    let file = fs::File::open(tar_extract_directory.join("meta.json")).unwrap();
    let reader = BufReader::new(file);
    let bundle_meta: BundleMeta = serde_json::from_reader(reader).unwrap();

    let expected_repo_root = String::from(
        fs::canonicalize(temp_dir.path())
            .expect("Could not canonicalize temp dir")
            .as_os_str()
            .to_str()
            .unwrap(),
    );
    let expected = BundleRepo {
        repo: Repo {
            host: String::from("github.com"),
            owner: String::from("my-org"),
            name: String::from("my-repo"),
        },
        repo_root: expected_repo_root,
        repo_url: String::from(repo_url),
        repo_head_sha: String::from(sha),
        repo_head_sha_short: Some(String::from("1234567")),
        repo_head_branch: String::from(branch),
        repo_head_commit_epoch: epoch,
        repo_head_commit_message: String::from("Initial commit"),
        repo_head_author_name: String::from(author_name),
        repo_head_author_email: String::from(""),
    };
    assert_eq!(bundle_meta.base_props.repo, expected);

    // Fourth request should be telemetry
    assert_matches!(
        requests_iter.next().unwrap(),
        RequestPayload::TelemetryUploadMetrics(_)
    );

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[derive(Debug, Clone, Copy)]
enum QuarantineConfigResponse {
    Disabled,
    None,
    Some,
    All,
}

#[derive(Debug, Clone, Copy)]
enum CreateBundleResponse {
    Error,
    Success,
}

#[tokio::test(flavor = "multi_thread")]
async fn do_not_quarantine_tests_when_quarantine_disabled_set() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let mut mock_server_builder = MockServerBuilder::new();

    lazy_static! {
        static ref QUARANTINE_CONFIG_RESPONSE: Arc<Mutex<QuarantineConfigResponse>> =
            Arc::new(Mutex::new(QuarantineConfigResponse::None));
    }
    mock_server_builder.set_get_quarantining_config_handler(
        |Json(get_quarantine_bulk_test_status_request): Json<GetQuarantineConfigRequest>| {
            let mut test_ids = get_quarantine_bulk_test_status_request
                .test_identifiers
                .into_iter()
                .map(|t| t.id)
                .collect::<Vec<_>>();
            let quarantine_config_response = *QUARANTINE_CONFIG_RESPONSE.lock().unwrap();
            let quarantined_tests = match quarantine_config_response {
                QuarantineConfigResponse::Disabled => Vec::new(),
                QuarantineConfigResponse::None => Vec::new(),
                QuarantineConfigResponse::Some => test_ids.split_off(1),
                QuarantineConfigResponse::All => test_ids,
            };
            let is_disabled = matches!(
                quarantine_config_response,
                QuarantineConfigResponse::Disabled
            );
            async move {
                Json(GetQuarantineConfigResponse {
                    is_disabled,
                    quarantined_tests,
                })
            }
        },
    );
    lazy_static! {
        static ref CREATE_BUNDLE_RESPONSE: Arc<Mutex<CreateBundleResponse>> =
            Arc::new(Mutex::new(CreateBundleResponse::Error));
    }
    mock_server_builder.set_create_bundle_handler(
        |State(state): State<SharedMockServerState>, _: Json<CreateBundleUploadRequest>| {
            let create_bundle_response = *CREATE_BUNDLE_RESPONSE.lock().unwrap();
            let result = match create_bundle_response {
                CreateBundleResponse::Error => Err(String::from("Server is down")),
                CreateBundleResponse::Success => {
                    let host = &state.host;
                    Ok(Json(CreateBundleUploadResponse {
                        id: String::from("test-bundle-upload-id"),
                        id_v2: String::from("test-bundle-upload-id-v2"),
                        url: format!("{host}/s3upload"),
                        key: String::from("unused"),
                    }))
                }
            };
            async { result }
        },
    );
    let state = mock_server_builder.spawn_mock_server().await;

    let mut command = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .disable_quarantining(true)
        .test_process_exit_code(1)
        .command();

    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Success;

    // there is a provided exit code, so all of the options below will default to failure
    // First run won't quarantine any tests
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::None;
    command.assert().failure();

    // Second run won't quarantine even when config generates 1 quarantined test
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::Some;
    command.assert().failure();

    // Third run won't quarantine even when config generates all tests quarantined
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::All;
    command.assert().failure();

    // Fourth run won't quarantine with quarantining disabled in the app
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::Disabled;
    command.assert().failure();

    // repeat the test with quarantining disabled with use-quarantining
    let mut command = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .use_quarantining(false)
        .test_process_exit_code(1)
        .command();

    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Success;

    // there is a provided exit code, so all of the options below will default to failure
    // First run won't quarantine any tests
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::None;
    command.assert().failure();

    // Second run won't quarantine even when config generates 1 quarantined test
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::Some;
    command.assert().failure();

    // Third run won't quarantine even when config generates all tests quarantined
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::All;
    command.assert().failure();

    // Fourth run won't quarantine with quarantining disabled in the app
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::Disabled;
    command.assert().failure();
}

#[tokio::test(flavor = "multi_thread")]
async fn uses_software_exit_code_if_upload_fails() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);

    let junit_location = temp_dir.path().join("junit.xml");
    let mut junit_file = fs::File::create(junit_location).unwrap();
    write!(junit_file, r#"
        <?xml version="1.0" encoding="UTF-8" ?>
        <testsuites name="vitest tests" tests="1" failures="0" errors="0" time="1.128069555">
            <testsuite name="src/constants/products-parser-server.test.ts" timestamp="2025-05-27T15:31:07.510Z" hostname="christian-cloudtop" tests="10" failures="0" errors="0" skipped="0" time="0.007118101">
                <testcase classname="src/constants/products-parser-server.test.ts" name="Product Parsers &gt; Server-side parsers &gt; has parsers for all products" time="0.001408508">
                    <failure>
                        Test failed
                    </failure>
                </testcase>
            </testsuite>
        </testsuites">
    "#).unwrap();

    let mut mock_server_builder = MockServerBuilder::new();
    mock_server_builder.set_s3_upload_handler(
        |_: State<SharedMockServerState>, _: Json<CreateBundleUploadRequest>| async {
            Err::<String, String>(String::from("Upload is broke"))
        },
    );
    let state = mock_server_builder.spawn_mock_server().await;

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .disable_quarantining(false)
        .use_quarantining(true)
        .junit_paths("junit.xml")
        .command()
        .assert()
        .code(predicate::eq(exitcode::SOFTWARE));

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn uses_failure_exit_code_if_unquarantined_tests_fail() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);

    let junit_location = temp_dir.path().join("junit.xml");
    let mut junit_file = fs::File::create(junit_location).unwrap();
    write!(junit_file, r#"
        <?xml version="1.0" encoding="UTF-8" ?>
        <testsuites name="vitest tests" tests="1" failures="0" errors="0" time="1.128069555">
            <testsuite name="src/constants/products-parser-server.test.ts" timestamp="2025-05-27T15:31:07.510Z" hostname="christian-cloudtop" tests="10" failures="0" errors="0" skipped="0" time="0.007118101">
                <testcase classname="src/constants/products-parser-server.test.ts" name="Product Parsers &gt; Server-side parsers &gt; has parsers for all products" time="0.001408508">
                    <failure>
                        Test failed
                    </failure>
                </testcase>
            </testsuite>
        </testsuites>
    "#).unwrap();

    let mock_server_builder = MockServerBuilder::new();
    let state = mock_server_builder.spawn_mock_server().await;

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .disable_quarantining(false)
        .use_quarantining(true)
        .junit_paths("junit.xml")
        .command()
        .assert()
        .code(predicate::eq(EXIT_FAILURE));

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn uses_passed_exit_code_if_unquarantined_tests_fail() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);

    let junit_location = temp_dir.path().join("junit.xml");
    let mut junit_file = fs::File::create(junit_location).unwrap();
    write!(junit_file, r#"
        <?xml version="1.0" encoding="UTF-8" ?>
        <testsuites name="vitest tests" tests="1" failures="0" errors="0" time="1.128069555">
            <testsuite name="src/constants/products-parser-server.test.ts" timestamp="2025-05-27T15:31:07.510Z" hostname="christian-cloudtop" tests="10" failures="0" errors="0" skipped="0" time="0.007118101">
                <testcase classname="src/constants/products-parser-server.test.ts" name="Product Parsers &gt; Server-side parsers &gt; has parsers for all products" time="0.001408508">
                    <failure>
                        Test failed
                    </failure>
                </testcase>
            </testsuite>
        </testsuites">
    "#).unwrap();

    let mock_server_builder = MockServerBuilder::new();
    let state = mock_server_builder.spawn_mock_server().await;

    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .disable_quarantining(false)
        .use_quarantining(true)
        .junit_paths("junit.xml")
        .test_process_exit_code(123)
        .command()
        .assert()
        .code(predicate::eq(123));

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn uploaded_file_contains_updated_test_files() {
    let temp_dir = TempDir::with_prefix("not_hidden").unwrap();
    generate_mock_git_repo(&temp_dir);
    println!(
        "Tmp dir path is {:?}, canonicalized {:?}",
        temp_dir,
        temp_dir.canonicalize()
    );

    let inner_dir = temp_dir.path().join("inner_dir");
    fs::create_dir(inner_dir).unwrap();

    let test_location = temp_dir.path().join("inner_dir").join("test_file.ts");
    let mut test_file = fs::File::create(test_location).unwrap();
    write!(test_file, r#"it("does stuff", x)"#).unwrap();

    let junit_location = temp_dir.path().join("junit_file.xml");
    let mut junit_file = fs::File::create(junit_location).unwrap();
    write!(junit_file, r#"
        <?xml version="1.0" encoding="UTF-8" ?>
        <testsuites name="vitest tests" tests="1" failures="0" errors="0" time="1.128069555">
            <testsuite name="test_file.ts" timestamp="2025-05-27T15:31:07.510Z" hostname="christian-cloudtop" tests="10" failures="0" errors="0" skipped="0" time="0.007118101">
                <testcase classname="test_file.ts" name="Product Parsers &gt; Server-side parsers &gt; has parsers for all products" time="0.001408508">
                </testcase>
            </testsuite>
        </testsuites>
    "#).unwrap();

    let state = MockServerBuilder::new().spawn_mock_server().await;
    let assert = CommandBuilder::upload(temp_dir.path(), state.host.clone())
        .junit_paths("junit_file.xml")
        .command()
        .assert()
        .success();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 3);

    let file_upload = assert_matches!(requests.get(1).unwrap(), RequestPayload::S3Upload(d) => d);
    let file = fs::File::open(file_upload.join("meta.json")).unwrap();
    let reader = BufReader::new(file);
    let bundle_meta: BundleMeta = serde_json::from_reader(reader).unwrap();
    let internal_bundled_file = bundle_meta.internal_bundled_file.as_ref().unwrap();
    let bin = fs::read(file_upload.join(&internal_bundled_file.path)).unwrap();
    let report = proto::test_context::test_run::TestResult::decode(&*bin).unwrap();

    println!("{assert}");

    assert_eq!(report.test_case_runs.len(), 1);
    let test_case_run = &report.test_case_runs.first().unwrap();
    assert_eq!(test_case_run.classname, "test_file.ts");
    let expected_file = temp_dir
        .path()
        .canonicalize()
        .unwrap()
        .join("inner_dir")
        .join("test_file.ts")
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(test_case_run.file, expected_file);
}
