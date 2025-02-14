use std::sync::{Arc, Mutex};

use api::message::{
    CreateBundleUploadRequest, CreateBundleUploadResponse, GetQuarantineConfigRequest,
    GetQuarantineConfigResponse,
};
use axum::{extract::State, Json};
use lazy_static::lazy_static;
use predicates::prelude::*;
use tempfile::tempdir;
use test_utils::mock_server::{MockServerBuilder, SharedMockServerState};

use crate::{
    command_builder::CommandBuilder,
    utils::{generate_mock_codeowners, generate_mock_git_repo, generate_mock_valid_junit_xmls},
};

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

// NOTE: must be multi threaded to start a mock server
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

    let mut command = CommandBuilder::quarantine(temp_dir.path(), state.host.clone()).command();

    let upload_failure = predicate::str::contains("Error uploading test results");

    // First run won't quarantine any tests
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::None;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Error;
    command.assert().failure().stdout(upload_failure.clone());

    // Second run quarantines all, but 1 test
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::Some;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Error;
    command.assert().failure().stdout(upload_failure.clone());

    // Third run will quarantine all tests
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::All;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Error;
    command.assert().success().stdout(upload_failure.clone());

    // Fourth run will quarantine all tests, and upload them
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::All;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Success;
    command
        .assert()
        .success()
        .stdout(upload_failure.clone().not());

    // Fifth run will run with quarantining disabled, but will log upload failure
    // there is no provided exit code, so it will default to success.
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::Disabled;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Error;
    command.assert().success().stdout(upload_failure.clone());
}

#[tokio::test(flavor = "multi_thread")]
async fn do_no_quarantines_tests_when_use_quarantined_disabled() {
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

    let mut command = CommandBuilder::quarantine(temp_dir.path(), state.host.clone())
        .use_quarantining(false)
        .command();

    let upload_failure = predicate::str::contains("Error uploading test results");
    // there is no provided exit code, so all of the options below will default to success.

    // First run won't quarantine any tests
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::None;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Error;
    command.assert().success().stdout(upload_failure.clone());

    // Second run won't quarantine even when config generates 1 quarantined test
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::Some;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Error;
    command.assert().success().stdout(upload_failure.clone());

    // Third run won't quarantine even when config generates all tests quarantined
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::All;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Error;
    command.assert().success().stdout(upload_failure.clone());

    // Fourth run won't quarantine tests even when config generates all tests quarantined and upload is successful
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::All;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Success;
    command
        .assert()
        .success()
        .stdout(upload_failure.clone().not());

    // Fifth run will run with quarantining disabled, but will log upload failure
    *QUARANTINE_CONFIG_RESPONSE.lock().unwrap() = QuarantineConfigResponse::Disabled;
    *CREATE_BUNDLE_RESPONSE.lock().unwrap() = CreateBundleResponse::Error;
    command.assert().success().stdout(upload_failure.clone());
}
