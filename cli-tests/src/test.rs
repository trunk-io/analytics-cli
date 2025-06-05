use std::{
    fs::{self, File},
    io::BufReader,
};

use api::message::{
    CreateBundleUploadRequest, CreateBundleUploadResponse, GetQuarantineConfigRequest,
    GetQuarantineConfigResponse,
};
use assert_matches::assert_matches;
use axum::{extract::State, Json};
use bundle::BundleMeta;
use context::{bazel_bep::parser::BazelBepParser, junit::parser::JunitParser};
use predicates::prelude::*;
use tempfile::tempdir;
use test_utils::mock_server::{MockServerBuilder, RequestPayload, SharedMockServerState};

use crate::{
    command_builder::CommandBuilder,
    utils::{
        generate_mock_bazel_bep, generate_mock_codeowners, generate_mock_git_repo,
        generate_mock_valid_junit_xmls,
    },
};

// NOTE: must be multi threaded to start a mock server
#[tokio::test(flavor = "multi_thread")]
async fn test_command_succeeds_with_successful_upload() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    CommandBuilder::test(
        temp_dir.path(),
        state.host.clone(),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("exit 0"),
        ],
    )
    .use_quarantining(false)
    .command()
    .assert()
    .success()
    .code(0);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_command_fails_with_successful_upload() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    CommandBuilder::test(
        temp_dir.path(),
        state.host.clone(),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("exit 1"),
        ],
    )
    .use_quarantining(false)
    .command()
    .assert()
    .failure()
    .code(1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_run_test_with_really_long_command() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    // Create a very long command with many arguments
    let long_command = vec![
        String::from("bash"),
        String::from("-c"),
        format!(
            "echo '{}' && exit 0",
            "x".repeat(10000) // Create a 10000-character string
        ),
    ];

    let assert = CommandBuilder::test(temp_dir.path(), state.host.clone(), long_command)
        .use_quarantining(false)
        .command()
        .assert()
        .success();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 3);
    let mut requests_iter = requests.into_iter();

    // First request should be create bundle upload
    assert_matches!(
        requests_iter.next().unwrap(),
        RequestPayload::CreateBundleUpload(_)
    );

    // Second request should be s3 upload
    let tar_extract_directory =
        assert_matches!(requests_iter.next().unwrap(), RequestPayload::S3Upload(d) => d);

    // Verify the command in meta.json is not truncated
    let file = fs::File::open(tar_extract_directory.join("meta.json")).unwrap();
    let reader = BufReader::new(file);
    let bundle_meta: BundleMeta = serde_json::from_reader(reader).unwrap();
    let test_command = bundle_meta.base_props.test_command.unwrap();
    assert!(test_command.len() > 10000, "Command was truncated");

    // Third request should be telemetry
    assert_matches!(
        requests_iter.next().unwrap(),
        RequestPayload::TelemetryUploadMetrics(_)
    );

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_command_fails_with_no_junit_files_no_quarantine_successful_upload() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let assert = CommandBuilder::test(
        temp_dir.path(),
        state.host.clone(),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("exit 128"),
        ],
    )
    .command()
    .assert()
    .failure()
    .code(128)
    .stderr(predicate::str::contains(
        "No tests were found in the provided test results",
    ));

    println!("{assert}");

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 3);
    let mut requests_iter = requests.into_iter();

    assert!(matches!(
        requests_iter.next().unwrap(),
        RequestPayload::CreateBundleUpload(..)
    ));

    let tar_extract_directory =
        assert_matches!(requests_iter.next().unwrap(), RequestPayload::S3Upload(d) => d);
    let file = fs::File::open(tar_extract_directory.join("meta.json")).unwrap();
    let reader = BufReader::new(file);
    let bundle_meta: BundleMeta = serde_json::from_reader(reader).unwrap();
    assert_eq!(
        bundle_meta.base_props.test_command.unwrap(),
        "bash -c exit 128"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_command_succeeds_with_upload_not_connected() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    CommandBuilder::test(
        temp_dir.path(),
        String::from("https://localhost:10"),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("exit 0"),
        ],
    )
    .use_quarantining(false)
    .command()
    .assert()
    .success()
    .code(0);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_command_fails_with_upload_not_connected() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    CommandBuilder::test(
        temp_dir.path(),
        String::from("https://localhost:10"),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("exit 1"),
        ],
    )
    .use_quarantining(false)
    .command()
    .assert()
    .failure()
    .code(1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_command_succeeds_with_bundle_using_bep() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_bazel_bep(&temp_dir);
    // Rename the file to simulate a BEP file being generated by bazel
    let result = std::fs::rename(
        temp_dir.path().join("bep.json"),
        temp_dir.path().join("bep.json.tmp"),
    );
    assert!(result.is_ok());

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let assert = CommandBuilder::test(
        temp_dir.path(),
        state.host.clone(),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("mv ./bep.json.tmp ./bep.json && exit 1"),
        ],
    )
    .bazel_bep_path("./bep.json")
    .command()
    .assert()
    .failure();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 4);

    let tar_extract_directory = assert_matches!(&requests[2], RequestPayload::S3Upload(d) => d);

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
async fn quarantining_resets_fail_code() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let mut mock_server_builder = MockServerBuilder::new();

    mock_server_builder.set_get_quarantining_config_handler(
        |Json(get_quarantine_bulk_test_status_request): Json<GetQuarantineConfigRequest>| {
            let test_ids = get_quarantine_bulk_test_status_request
                .test_identifiers
                .into_iter()
                .map(|t| t.id)
                .collect::<Vec<_>>();
            async move {
                Json(GetQuarantineConfigResponse {
                    is_disabled: false,
                    quarantined_tests: test_ids,
                })
            }
        },
    );

    mock_server_builder.set_create_bundle_handler(
        |State(state): State<SharedMockServerState>, _: Json<CreateBundleUploadRequest>| {
            async move {
                let host = &state.host.clone();
                Ok::<axum::Json<CreateBundleUploadResponse>, String>(Json(CreateBundleUploadResponse {
                    id: String::from("test-bundle-upload-id"),
                    id_v2: String::from("test-bundle-upload-id-v2"),
                    url: format!("{host}/s3upload"),
                    key: String::from("unused"),
                }))
            }
        },
    );
    let state = mock_server_builder.spawn_mock_server().await;

    CommandBuilder::test(
        temp_dir.path(),
        state.host.clone(),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("sleep 1; touch ./*; exit 1"),
        ],
    )
    .command()
    .assert()
    .success();
}

#[tokio::test(flavor = "multi_thread")]
async fn quarantining_not_active_when_disable_quarantining_set() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let mut mock_server_builder = MockServerBuilder::new();

    mock_server_builder.set_get_quarantining_config_handler(
        |Json(get_quarantine_bulk_test_status_request): Json<GetQuarantineConfigRequest>| {
            let test_ids = get_quarantine_bulk_test_status_request
                .test_identifiers
                .into_iter()
                .map(|t| t.id)
                .collect::<Vec<_>>();
            async move {
                Json(GetQuarantineConfigResponse {
                    is_disabled: false,
                    quarantined_tests: test_ids,
                })
            }
        },
    );

    mock_server_builder.set_create_bundle_handler(
        |State(state): State<SharedMockServerState>, _: Json<CreateBundleUploadRequest>| {
            async move {
                let host = &state.host.clone();
                Ok::<axum::Json<CreateBundleUploadResponse>, String>(Json(CreateBundleUploadResponse {
                    id: String::from("test-bundle-upload-id"),
                    id_v2: String::from("test-bundle-upload-id-v2"),
                    url: format!("{host}/s3upload"),
                    key: String::from("unused"),
                }))
            }
        },
    );
    let state = mock_server_builder.spawn_mock_server().await;

    CommandBuilder::test(
        temp_dir.path(),
        state.host.clone(),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("touch ./*; exit 1"),
        ],
    )
    .disable_quarantining(true)
    .command()
    .assert()
    .failure();
}

#[tokio::test(flavor = "multi_thread")]
async fn quarantining_not_active_when_use_quarantining_false() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let mut mock_server_builder = MockServerBuilder::new();

    mock_server_builder.set_get_quarantining_config_handler(
        |Json(get_quarantine_bulk_test_status_request): Json<GetQuarantineConfigRequest>| {
            let test_ids = get_quarantine_bulk_test_status_request
                .test_identifiers
                .into_iter()
                .map(|t| t.id)
                .collect::<Vec<_>>();
            async move {
                Json(GetQuarantineConfigResponse {
                    is_disabled: false,
                    quarantined_tests: test_ids,
                })
            }
        },
    );

    mock_server_builder.set_create_bundle_handler(
        |State(state): State<SharedMockServerState>, _: Json<CreateBundleUploadRequest>| {
            async move {
                let host = &state.host.clone();
                Ok::<axum::Json<CreateBundleUploadResponse>, String>(Json(CreateBundleUploadResponse {
                    id: String::from("test-bundle-upload-id"),
                    id_v2: String::from("test-bundle-upload-id-v2"),
                    url: format!("{host}/s3upload"),
                    key: String::from("unused"),
                }))
            }
        },
    );
    let state = mock_server_builder.spawn_mock_server().await;

    CommandBuilder::test(
        temp_dir.path(),
        state.host.clone(),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("touch ./*; exit 1"),
        ],
    )
    .use_quarantining(false)
    .command()
    .assert()
    .failure();
}

#[tokio::test(flavor = "multi_thread")]
async fn quarantining_not_active_when_disable_true_but_use_true() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let mut mock_server_builder = MockServerBuilder::new();

    mock_server_builder.set_get_quarantining_config_handler(
        |Json(get_quarantine_bulk_test_status_request): Json<GetQuarantineConfigRequest>| {
            let test_ids = get_quarantine_bulk_test_status_request
                .test_identifiers
                .into_iter()
                .map(|t| t.id)
                .collect::<Vec<_>>();
            async move {
                Json(GetQuarantineConfigResponse {
                    is_disabled: false,
                    quarantined_tests: test_ids,
                })
            }
        },
    );

    mock_server_builder.set_create_bundle_handler(
        |State(state): State<SharedMockServerState>, _: Json<CreateBundleUploadRequest>| {
            async move {
                let host = &state.host.clone();
                Ok::<axum::Json<CreateBundleUploadResponse>, String>(Json(CreateBundleUploadResponse {
                    id: String::from("test-bundle-upload-id"),
                    id_v2: String::from("test-bundle-upload-id-v2"),
                    url: format!("{host}/s3upload"),
                    key: String::from("unused"),
                }))
            }
        },
    );
    let state = mock_server_builder.spawn_mock_server().await;

    CommandBuilder::test(
        temp_dir.path(),
        state.host.clone(),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("touch ./*; exit 1"),
        ],
    )
    .disable_quarantining(true)
    .use_quarantining(true)
    .command()
    .assert()
    .failure();
}

#[tokio::test(flavor = "multi_thread")]
async fn quarantining_not_active_when_disable_false_but_use_false() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let mut mock_server_builder = MockServerBuilder::new();

    mock_server_builder.set_get_quarantining_config_handler(
        |Json(get_quarantine_bulk_test_status_request): Json<GetQuarantineConfigRequest>| {
            let test_ids = get_quarantine_bulk_test_status_request
                .test_identifiers
                .into_iter()
                .map(|t| t.id)
                .collect::<Vec<_>>();
            async move {
                Json(GetQuarantineConfigResponse {
                    is_disabled: false,
                    quarantined_tests: test_ids,
                })
            }
        },
    );

    mock_server_builder.set_create_bundle_handler(
        |State(state): State<SharedMockServerState>, _: Json<CreateBundleUploadRequest>| {
            async move {
                let host = &state.host.clone();
                Ok::<axum::Json<CreateBundleUploadResponse>, String>(Json(CreateBundleUploadResponse {
                    id: String::from("test-bundle-upload-id"),
                    id_v2: String::from("test-bundle-upload-id-v2"),
                    url: format!("{host}/s3upload"),
                    key: String::from("unused"),
                }))
            }
        },
    );
    let state = mock_server_builder.spawn_mock_server().await;

    CommandBuilder::test(
        temp_dir.path(),
        state.host.clone(),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("touch ./*; exit 1"),
        ],
    )
    .disable_quarantining(false)
    .use_quarantining(false)
    .command()
    .assert()
    .failure();
}

#[tokio::test(flavor = "multi_thread")]
async fn returns_exit_code_from_execution_when_failures_not_quarantined() {
    use std::io::Write;
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);

    let junit_location = temp_dir.path().join("junit.xml");
    let mut junit_file = File::create(junit_location).unwrap();
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

    let assert = CommandBuilder::test(
        temp_dir.path(),
        state.host.clone(),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("touch ./*; exit 123"),
        ],
    )
    .disable_quarantining(false)
    .use_quarantining(true)
    .junit_paths("junit.xml")
    .command()
    .assert()
    .code(predicate::eq(123));

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}
