mod common;

use std::{env, thread};

use api::message::{GetQuarantineConfigRequest, GetQuarantineConfigResponse};
use axum::{Json, extract::State, http::StatusCode};
use common::{clean_up_cache_files, cleanup_env_vars};
use constants::{
    TRUNK_API_CLIENT_RETRY_COUNT_ENV, TRUNK_API_TOKEN_ENV, TRUNK_ORG_URL_SLUG_ENV,
    TRUNK_PUBLIC_API_ADDRESS_ENV, TRUNK_QUARANTINED_TESTS_DISK_CACHE_TTL_SECS_ENV,
    TRUNK_REPO_HEAD_AUTHOR_NAME_ENV, TRUNK_REPO_HEAD_BRANCH_ENV, TRUNK_REPO_HEAD_SHA_ENV,
    TRUNK_REPO_URL_ENV, TRUNK_USE_UNCLONED_REPO_ENV,
};
use proto::upload_metrics::trunk::QuarantineQueryResult;
use serial_test::serial;
use tempfile::tempdir;
use test_report::report::{MutTestReport, Status};
use test_utils::mock_git_repo::setup_repo_with_commit;
use test_utils::mock_server::{MockServerBuilder, RequestPayload, SharedMockServerState};

fn assert_telemetry_quarantine_query_result(
    requests: &[RequestPayload],
    expected: QuarantineQueryResult,
) {
    let quarantine_query_result = requests
        .iter()
        .rev()
        .find_map(|req| match req {
            RequestPayload::TelemetryUploadMetrics(ur) => Some(ur.quarantine_query_result),
            _ => None,
        })
        .expect("expected telemetry upload metrics request");
    assert_eq!(
        QuarantineQueryResult::try_from(quarantine_query_result).unwrap(),
        expected
    );
}

fn set_mock_api_env(api_host: &str) {
    unsafe {
        env::set_var(TRUNK_PUBLIC_API_ADDRESS_ENV, api_host);
        env::set_var(TRUNK_API_TOKEN_ENV, "test-token");
    }
}

fn set_uncloned_repo_publish_env(repo_url: &str) {
    unsafe {
        env::set_var(TRUNK_ORG_URL_SLUG_ENV, "test-org");
        env::set_var(TRUNK_REPO_URL_ENV, repo_url);
        env::set_var(TRUNK_USE_UNCLONED_REPO_ENV, "true");
        env::set_var(TRUNK_REPO_HEAD_SHA_ENV, "");
        env::set_var(TRUNK_REPO_HEAD_BRANCH_ENV, "");
        env::set_var(TRUNK_REPO_HEAD_AUTHOR_NAME_ENV, "");
        env::set_var("CI", "1");
        env::set_var("GITHUB_JOB", "test-job");
    }
}

fn publish_minimal_success_test(test_report: &MutTestReport) {
    test_report.add_test(
        Some("1".into()),
        "test-name".into(),
        "test-classname".into(),
        "test-file".into(),
        "test-parent-name".into(),
        None,
        Status::Success,
        0,
        1000,
        1001,
        String::new(),
        String::new(),
        false,
    );
    assert!(test_report.publish());
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn telemetry_query_result_success_on_publish() {
    cleanup_env_vars();
    let temp_dir = tempdir().unwrap();
    let repo_setup_res = setup_repo_with_commit(&temp_dir);
    assert!(repo_setup_res.is_ok());
    assert!(env::set_current_dir(&temp_dir).is_ok());
    let state = MockServerBuilder::new().spawn_mock_server().await;
    set_mock_api_env(&state.host);
    unsafe {
        env::set_var(TRUNK_ORG_URL_SLUG_ENV, "test-org");
        env::set_var(TRUNK_QUARANTINED_TESTS_DISK_CACHE_TTL_SECS_ENV, "0");
    }

    thread::spawn(|| {
        let test_report = MutTestReport::new(
            "test".into(),
            "telemetry-success".into(),
            Some("test-variant".into()),
        );
        test_report.is_quarantined(
            Some("2".into()),
            Some("test-name".into()),
            Some("test-parent-name".into()),
            Some("test-classname".into()),
            Some("test-file".into()),
        );
        publish_minimal_success_test(&test_report);
    })
    .join()
    .unwrap();

    let requests = state.requests.lock().unwrap().clone();
    assert_telemetry_quarantine_query_result(&requests, QuarantineQueryResult::Success);
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn telemetry_query_result_skipped_without_lookup_on_publish() {
    cleanup_env_vars();
    let temp_dir = tempdir().unwrap();
    let _ = env::set_current_dir(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;
    set_mock_api_env(&state.host);
    set_uncloned_repo_publish_env("https://github.com/test-org/test-repo-skipped-telemetry.git");

    thread::spawn(|| {
        let test_report = MutTestReport::new("test".into(), "telemetry-skipped".into(), None);
        publish_minimal_success_test(&test_report);
    })
    .join()
    .unwrap();

    let requests = state.requests.lock().unwrap().clone();
    assert_telemetry_quarantine_query_result(&requests, QuarantineQueryResult::Skipped);
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn telemetry_query_result_cached_on_publish() {
    cleanup_env_vars();
    clean_up_cache_files();

    let temp_dir = tempdir().unwrap();
    let _ = env::set_current_dir(&temp_dir);

    let repo_url = "https://github.com/test-org/test-repo-cache-telemetry.git";
    set_uncloned_repo_publish_env(repo_url);

    let state = MockServerBuilder::new().spawn_mock_server().await;
    set_mock_api_env(&state.host);

    thread::spawn(|| {
        let warm_cache = MutTestReport::new("test".into(), "warm-cache".into(), None);
        warm_cache.is_quarantined(
            None,
            Some("warm".into()),
            Some("warm-parent".into()),
            Some("Warm".into()),
            Some("warm.rb".into()),
        );

        let test_report = MutTestReport::new("test".into(), "cached-publish".into(), None);
        test_report.is_quarantined(
            None,
            Some("cached".into()),
            Some("cached-parent".into()),
            Some("Cached".into()),
            Some("cached.rb".into()),
        );
        publish_minimal_success_test(&test_report);
    })
    .join()
    .unwrap();

    let requests = state.requests.lock().unwrap().clone();
    let get_quarantine_config_count = requests
        .iter()
        .filter(|req| matches!(req, RequestPayload::GetQuarantineConfig(_)))
        .count();
    assert_eq!(
        get_quarantine_config_count, 1,
        "second report should use disk cache during suite"
    );
    // Checks the last telemetry call
    assert_telemetry_quarantine_query_result(&requests, QuarantineQueryResult::Cached);
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn telemetry_query_result_disabled_on_publish() {
    cleanup_env_vars();

    let temp_dir = tempdir().unwrap();
    let _ = env::set_current_dir(&temp_dir);

    let repo_url = "https://github.com/test-org/test-repo-disabled-telemetry.git";
    set_uncloned_repo_publish_env(repo_url);

    let state = {
        let mut builder = MockServerBuilder::new();
        builder.set_get_quarantining_config_handler(
            |_state: State<SharedMockServerState>, _req: Json<GetQuarantineConfigRequest>| async move {
                Json(GetQuarantineConfigResponse {
                    is_disabled: true,
                    quarantined_tests: vec![],
                })
            },
        );
        builder.spawn_mock_server().await
    };

    set_mock_api_env(&state.host);
    unsafe {
        env::set_var(TRUNK_QUARANTINED_TESTS_DISK_CACHE_TTL_SECS_ENV, "0");
    }

    thread::spawn(|| {
        let test_report = MutTestReport::new("test".into(), "disabled-publish".into(), None);
        test_report.is_quarantined(
            None,
            Some("test-name".into()),
            Some("test-parent-name".into()),
            Some("TestClass".into()),
            Some("test_file.rs".into()),
        );
        publish_minimal_success_test(&test_report);
    })
    .join()
    .unwrap();

    let requests = state.requests.lock().unwrap().clone();
    assert_telemetry_quarantine_query_result(&requests, QuarantineQueryResult::Disabled);
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn telemetry_query_result_failure_on_publish() {
    cleanup_env_vars();
    clean_up_cache_files();

    let temp_dir = tempdir().unwrap();
    let _ = env::set_current_dir(&temp_dir);

    let repo_url = "https://github.com/test-org/test-repo-failure-telemetry.git";
    set_uncloned_repo_publish_env(repo_url);

    let state = {
        let mut builder = MockServerBuilder::new();
        builder.set_get_quarantining_config_handler(
            |Json(_): Json<GetQuarantineConfigRequest>| async {
                Err::<Json<GetQuarantineConfigResponse>, StatusCode>(
                    StatusCode::INTERNAL_SERVER_ERROR,
                )
            },
        );
        builder.spawn_mock_server().await
    };

    set_mock_api_env(&state.host);
    unsafe {
        env::set_var(TRUNK_QUARANTINED_TESTS_DISK_CACHE_TTL_SECS_ENV, "0");
        env::set_var(TRUNK_API_CLIENT_RETRY_COUNT_ENV, "0");
    }

    thread::spawn(|| {
        let test_report = MutTestReport::new("test".into(), "failure-publish".into(), None);
        test_report.is_quarantined(
            None,
            Some("test-name".into()),
            Some("test-parent-name".into()),
            Some("TestClass".into()),
            Some("test_file.rs".into()),
        );
        publish_minimal_success_test(&test_report);
    })
    .join()
    .unwrap();

    let requests = state.requests.lock().unwrap().clone();
    assert_telemetry_quarantine_query_result(&requests, QuarantineQueryResult::Failure);
}
