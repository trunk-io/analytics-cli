use std::sync::{Arc, Mutex};
use std::{fs, io::BufReader};

use api::message::{
    BundleUploadStatus, CreateBundleUploadRequest, CreateBundleUploadResponse, CreateRepoRequest,
    GetQuarantineConfigRequest, GetQuarantineConfigResponse, UpdateBundleUploadRequest,
};
use assert_cmd::Command;
use assert_matches::assert_matches;
use axum::{extract::State, Json};
use bundle::{BundleMeta, FileSetType};
use codeowners::CodeOwners;
use constants::{TRUNK_API_CLIENT_RETRY_COUNT_ENV, TRUNK_PUBLIC_API_ADDRESS_ENV};
use context::{
    bazel_bep::parser::BazelBepParser, junit::parser::JunitParser, repo::RepoUrlParts as Repo,
};
use lazy_static::lazy_static;
use predicates::prelude::*;
use tempfile::tempdir;
use test_utils::{
    inputs::get_test_file_path,
    mock_server::{MockServerBuilder, RequestPayload, SharedMockServerState},
};

use crate::utils::{
    generate_mock_bazel_bep, generate_mock_codeowners, generate_mock_git_repo,
    generate_mock_valid_junit_xmls, CARGO_RUN,
};

// NOTE: must be multi threaded to start a mock server
#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let args = &[
        "upload",
        "--junit-paths",
        "./*",
        "--org-url-slug",
        "test-org",
        "--token",
        "test-token",
    ];

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env(TRUNK_PUBLIC_API_ADDRESS_ENV, &state.host)
        .env(TRUNK_API_CLIENT_RETRY_COUNT_ENV, "0")
        .env("CI", "1")
        .env("GITHUB_JOB", "test-job")
        .args(args)
        .assert()
        // should fail due to quarantine and succeed without quarantining
        .failure();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 5);
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

    assert_eq!(
        requests_iter.next().unwrap(),
        RequestPayload::CreateRepo(CreateRepoRequest {
            repo: Repo {
                host: String::from("github.com"),
                owner: String::from("trunk-io"),
                name: String::from("analytics-cli"),
            },
            org_url_slug: String::from("test-org"),
            remote_urls: Vec::from(&[String::from(
                "https://github.com/trunk-io/analytics-cli.git"
            )]),
        })
    );

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

    let file_set = base_props.file_sets.get(0).unwrap();
    assert_eq!(file_set.file_set_type, FileSetType::Junit);
    assert_eq!(file_set.glob, "./*");
    assert_eq!(file_set.files.len(), 1);

    let bundled_file = file_set.files.get(0).unwrap();
    assert_eq!(bundled_file.path, "junit/0");
    assert!(
        fs::File::open(tar_extract_directory.join(&bundled_file.path))
            .unwrap()
            .metadata()
            .unwrap()
            .is_file()
    );
    assert_eq!(bundled_file.owners, ["@user"]);
    assert_eq!(bundled_file.team, None);

    assert_eq!(
        requests_iter.next().unwrap(),
        RequestPayload::UpdateBundleUpload(UpdateBundleUploadRequest {
            id: "test-bundle-upload-id".to_string(),
            upload_status: BundleUploadStatus::UploadComplete
        }),
    );

    assert!(debug_props
        .command_line
        .ends_with(&args.join(" ").replace("test-token", "***")));

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_using_bep() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_bazel_bep(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let args = &[
        "upload",
        "--bazel-bep-path",
        "./bep.json",
        "--org-url-slug",
        "test-org",
        "--token",
        "test-token",
    ];

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env(TRUNK_PUBLIC_API_ADDRESS_ENV, &state.host)
        .env(TRUNK_API_CLIENT_RETRY_COUNT_ENV, "0")
        .env("CI", "1")
        .env("GITHUB_JOB", "test-job")
        .args(args)
        .assert()
        .failure();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 5);

    let tar_extract_directory = assert_matches!(&requests[3], RequestPayload::S3Upload(d) => d);

    let junit_file = fs::File::open(tar_extract_directory.join("junit/0")).unwrap();
    let junit_reader = BufReader::new(junit_file);

    // Uploaded file is a junit, even when using BEP
    let mut junit_parser = JunitParser::new();
    assert!(junit_parser.parse(junit_reader).is_ok());
    assert!(junit_parser.issues().is_empty());

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
    // The test cases need not match up or have timestamps, so long as there is a testSummary
    // That indicates a flake or pass
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

    let args = &[
        "upload",
        "--bazel-bep-path",
        "./bep.json",
        "--org-url-slug",
        "test-org",
        "--token",
        "test-token",
    ];

    // Even though the junits contain failures, they contain retries that succeeded,
    // so the upload command should have a successful exit code
    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env(TRUNK_PUBLIC_API_ADDRESS_ENV, &state.host)
        .env(TRUNK_API_CLIENT_RETRY_COUNT_ENV, "0")
        .env("CI", "1")
        .env("GITHUB_JOB", "test-job")
        .args(args)
        .assert()
        .code(0)
        .success();

    // No quarantine request
    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 4);

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_success_timestamp_status_code() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    let test_bep_path = get_test_file_path("test_fixtures/bep_retries_timestamp");
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

    let args = &[
        "upload",
        "--bazel-bep-path",
        "./bep.json",
        "--org-url-slug",
        "test-org",
        "--token",
        "test-token",
    ];

    // Even though the junits contain failures, they contain retries that succeeded,
    // so the upload command should have a successful exit code
    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env(TRUNK_PUBLIC_API_ADDRESS_ENV, &state.host)
        .env(TRUNK_API_CLIENT_RETRY_COUNT_ENV, "0")
        .env("CI", "1")
        .env("GITHUB_JOB", "test-job")
        .args(args)
        .assert()
        .code(0)
        .success();

    // No quarantine request
    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 4);

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_empty_junit_paths() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env(TRUNK_PUBLIC_API_ADDRESS_ENV, &state.host)
        .env(TRUNK_API_CLIENT_RETRY_COUNT_ENV, "0")
        .env("CI", "1")
        .args(&[
            "upload",
            "--junit-paths",
            "",
            "--org-url-slug",
            "test-org",
            "--token",
            "test-token",
        ])
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

        let mut args = vec![
            "upload",
            "--junit-paths",
            "./*",
            "--org-url-slug",
            "test-org",
            "--token",
            "test-token",
        ];

        match flag {
            Flag::Long => args.push("--allow-empty-test-results"),
            Flag::LongWithEquals => args.push("--allow-empty-test-results=true"),
            Flag::Alias => args.push("--allow-missing-junit-files"),
            Flag::AliasWithEquals => args.push("--allow-missing-junit-files=true"),
            Flag::Default => {}
            Flag::Off => {
                args.push("--allow-empty-test-results");
                args.push("false");
            }
            Flag::OffWithEquals => {
                args.push("--allow-empty-test-results=false");
            }
            Flag::OffAlias => {
                args.push("--allow-missing-junit-files");
                args.push("false");
            }
            Flag::OffAliasWithEquals => {
                args.push("--allow-missing-junit-files=false");
            }
        };

        args.push("--print-files");

        let mut assert = Command::new(CARGO_RUN.path())
            .current_dir(&temp_dir)
            .env(TRUNK_PUBLIC_API_ADDRESS_ENV, &state.host)
            .env(TRUNK_API_CLIENT_RETRY_COUNT_ENV, "0")
            .env("CI", "1")
            .args(&args)
            .assert();

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
async fn upload_bundle_valid_repo_root() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env(TRUNK_PUBLIC_API_ADDRESS_ENV, &state.host)
        .env(TRUNK_API_CLIENT_RETRY_COUNT_ENV, "0")
        .env("CI", "1")
        .args(&[
            "upload",
            "--junit-paths",
            "./*",
            "--org-url-slug",
            "test-org",
            "--repo-root",
            "../",
            "--token",
            "test-token",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Error: Failed to open git repository at \"../\"",
        ));

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_when_server_down() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);

    let args = &[
        "upload",
        "--junit-paths",
        "./*",
        "--org-url-slug",
        "test-org",
        "--token",
        "test-token",
    ];

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env(TRUNK_PUBLIC_API_ADDRESS_ENV, "https://localhost:10")
        .env(TRUNK_API_CLIENT_RETRY_COUNT_ENV, "0")
        .env("CI", "1")
        .env("GITHUB_JOB", "test-job")
        .args(args)
        .assert()
        .success();

    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_with_no_junit_files_no_quarantine_successful_upload() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env(TRUNK_PUBLIC_API_ADDRESS_ENV, &state.host)
        .env(TRUNK_API_CLIENT_RETRY_COUNT_ENV, "0")
        .env("CI", "1")
        .env("GITHUB_JOB", "test-job")
        .args([
            "upload",
            "--junit-paths",
            "./*",
            "--org-url-slug",
            "test-org",
            "--token",
            "test-token",
        ])
        .assert()
        .code(0)
        .success()
        .stderr(predicate::str::contains(
            "No JUnit files found, not quarantining any tests",
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

    let args = &[
        "upload",
        "--junit-paths",
        "./*",
        "--org-url-slug",
        "test-org",
        "--token",
        "test-token",
    ];

    let mut command = Command::new(CARGO_RUN.path());
    command
        .current_dir(&temp_dir)
        .env(TRUNK_PUBLIC_API_ADDRESS_ENV, &state.host)
        .env(TRUNK_API_CLIENT_RETRY_COUNT_ENV, "0")
        .env("CI", "1")
        .env("GITHUB_JOB", "test-job")
        .args(args);

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
