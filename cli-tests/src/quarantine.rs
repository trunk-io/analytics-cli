use std::sync::{Arc, Mutex};

use crate::utils::{
    generate_mock_codeowners, generate_mock_git_repo, generate_mock_valid_junit_xmls, CARGO_RUN,
};
use api::{
    CreateBundleUploadRequest, CreateBundleUploadResponse, GetQuarantineBulkTestStatusRequest,
    QuarantineConfig,
};
use assert_cmd::Command;
use axum::{extract::State, Json};
use constants::{TRUNK_API_CLIENT_RETRY_COUNT_ENV, TRUNK_PUBLIC_API_ADDRESS_ENV};
use lazy_static::lazy_static;
use predicates::prelude::*;
use tempfile::tempdir;
use test_utils::mock_server::{MockServerBuilder, SharedMockServerState};

// NOTE: must be multi threaded to start a mock server
#[tokio::test(flavor = "multi_thread")]
async fn quarantines_tests_regardless_of_upload() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let mut mock_server_builder = MockServerBuilder::new();

    #[derive(Debug, Clone, Copy)]
    enum QuarantineConfigResponse {
        None,
        Some,
        All,
    }
    lazy_static! {
        static ref QUARANTINE_CONFIG_RESPONSE: Arc<Mutex<QuarantineConfigResponse>> =
            Arc::new(Mutex::new(QuarantineConfigResponse::None));
    }
    mock_server_builder.set_get_quarantining_config_handler(
        |Json(get_quarantine_bulk_test_status_request): Json<
            GetQuarantineBulkTestStatusRequest,
        >| {
            let mut test_ids = get_quarantine_bulk_test_status_request
                .test_identifiers
                .into_iter()
                .map(|t| t.id)
                .collect::<Vec<_>>();
            let quarantine_config_response = *QUARANTINE_CONFIG_RESPONSE.lock().unwrap();
            let quarantined_tests = match quarantine_config_response {
                QuarantineConfigResponse::None => Vec::new(),
                QuarantineConfigResponse::Some => test_ids.split_off(1),
                QuarantineConfigResponse::All => test_ids,
            };
            async {
                Json(QuarantineConfig {
                    is_disabled: false,
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
        "quarantine",
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

    let upload_failure = predicate::str::contains("Error uploading test results");

    // First run won't quarantine any tests
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::None;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Error;
    command.assert().failure().stderr(upload_failure.clone());

    // Second run quarantines all, but 1 test
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::Some;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Error;
    command.assert().failure().stderr(upload_failure.clone());

    // Third run will quarantine all tests
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::All;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Error;
    command.assert().success().stderr(upload_failure.clone());

    // Fourth run will quarantine all tests, and upload them
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::All;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Success;
    command
        .assert()
        .success()
        .stderr(upload_failure.clone().not());
}
