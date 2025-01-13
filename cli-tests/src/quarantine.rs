use std::sync::atomic::{AtomicUsize, Ordering};

use crate::utils::{
    generate_mock_codeowners, generate_mock_git_repo, generate_mock_valid_junit_xmls, CARGO_RUN,
};
use api::{
    CreateBundleUploadRequest, CreateBundleUploadResponse, GetQuarantineBulkTestStatusRequest,
    QuarantineConfig,
};
use assert_cmd::Command;
use axum::{extract::State, Json};
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
    lazy_static! {
        static ref CALLED_COUNT: AtomicUsize = AtomicUsize::new(0);
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
            let called_count = CALLED_COUNT.load(Ordering::SeqCst);
            let quarantined_tests = match called_count {
                0 => Vec::new(),
                1 => test_ids.split_off(1),
                2..=3 => test_ids,
                _ => panic!("Should not be called again"),
            };
            CALLED_COUNT.store(called_count + 1, Ordering::SeqCst);
            async {
                Json(QuarantineConfig {
                    is_disabled: false,
                    quarantined_tests,
                })
            }
        },
    );
    mock_server_builder.set_create_bundle_handler(
        |State(state): State<SharedMockServerState>, _: Json<CreateBundleUploadRequest>| {
            let result = match CALLED_COUNT.load(Ordering::SeqCst) {
                0..=3 => Err(String::from("Server is down")),
                4 => {
                    let host = &state.host;
                    Ok(Json(CreateBundleUploadResponse {
                        id: String::from("test-bundle-upload-id"),
                        id_v2: String::from("test-bundle-upload-id-v2"),
                        url: format!("{host}/s3upload"),
                        key: String::from("unused"),
                    }))
                }
                _ => panic!("Should not be called again"),
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
        .env("TRUNK_PUBLIC_API_ADDRESS", &state.host)
        .env("CI", "1")
        .env("GITHUB_JOB", "test-job")
        .args(args);

    let upload_failure = predicate::str::contains("Error uploading test results");

    // First run won't quarantine any tests
    command.assert().failure().stderr(upload_failure.clone());

    // Second run quarantines all, but 1 test
    command.assert().failure().stderr(upload_failure.clone());

    // Third run will quarantine all tests
    command.assert().success().stderr(upload_failure.clone());

    // Fourth run will quarantine all tests, and upload them
    command
        .assert()
        .success()
        .stderr(upload_failure.clone().not());
}
