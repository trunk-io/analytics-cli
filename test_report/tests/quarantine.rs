mod common;

use std::{env, fs, thread};

use api::message::{GetQuarantineConfigRequest, GetQuarantineConfigResponse};
use axum::{Json, extract::State, http::StatusCode};
use bundle::Test;
use common::{clean_up_cache_files, cleanup_env_vars, setup_quarantine_disk_cache_dir};
use constants::{
    TRUNK_API_CLIENT_RETRY_COUNT_ENV, TRUNK_API_TOKEN_ENV, TRUNK_ORG_URL_SLUG_ENV,
    TRUNK_PUBLIC_API_ADDRESS_ENV, TRUNK_QUARANTINED_TESTS_DISK_CACHE_TTL_SECS_ENV,
    TRUNK_REPO_HEAD_AUTHOR_NAME_ENV, TRUNK_REPO_HEAD_BRANCH_ENV, TRUNK_REPO_HEAD_COMMIT_EPOCH_ENV,
    TRUNK_REPO_HEAD_SHA_ENV, TRUNK_REPO_ROOT_ENV, TRUNK_REPO_URL_ENV, TRUNK_USE_UNCLONED_REPO_ENV,
    TRUNK_VARIANT_ENV,
};
use context::repo::RepoUrlParts;
use serial_test::serial;
use tempfile::tempdir;
use test_report::report::MutTestReport;
use test_utils::mock_git_repo::setup_repo_with_commit;
use test_utils::mock_server::{MockServerBuilder, RequestPayload, SharedMockServerState};

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn quarantine_variant_impacts_quarantining() {
    cleanup_env_vars();
    let temp_dir = tempdir().unwrap();
    setup_quarantine_disk_cache_dir(&temp_dir);
    let repo_setup_res = setup_repo_with_commit(&temp_dir);
    assert!(repo_setup_res.is_ok());
    let _ = env::set_current_dir(&temp_dir);

    // Test parameters that will be used for quarantine checking
    let test_name = Some("test_name".to_string());
    let test_parent_name = Some("test_parent".to_string());
    let test_classname = Some("TestClass".to_string());
    let test_file = Some("test_file.rs".to_string());

    let repo = RepoUrlParts {
        host: "github.com".into(),
        owner: "trunk-io".into(),
        name: "analytics-cli".into(),
    };

    // Generate a base test ID (without variant) - used when testing with ID parameter
    let base_test_id = Test::new(
        None,
        test_name.clone().unwrap_or_default(),
        test_parent_name.clone().unwrap_or_default(),
        test_classname.clone(),
        test_file.clone(),
        "test-org".to_string(),
        &repo,
        None,
        "".to_string(), // No variant for base ID
    )
    .id;

    // Generate the expected test ID with variant1 from scratch (for "without ID" case)
    // This matches what happens when is_quarantined is called with None and variant1
    let expected_test_id_variant1 = Test::new(
        None,
        test_name.clone().unwrap_or_default(),
        test_parent_name.clone().unwrap_or_default(),
        test_classname.clone(),
        test_file.clone(),
        "test-org".to_string(),
        &repo,
        None,
        "variant1".to_string(),
    )
    .id;

    // Generate the expected test ID with variant1 using the base ID (for "with ID" case)
    // This matches what happens when is_quarantined is called with base_test_id and variant1
    let expected_test_id_variant1_from_base = Test::new(
        Some(base_test_id.clone()),
        test_name.clone().unwrap_or_default(),
        test_parent_name.clone().unwrap_or_default(),
        test_classname.clone(),
        test_file.clone(),
        "test-org".to_string(),
        &repo,
        None,
        "variant1".to_string(),
    )
    .id;

    // Generate the expected test ID with variant2 using the base ID (for verification)
    let expected_test_id_variant2 = Test::new(
        Some(base_test_id.clone()),
        test_name.clone().unwrap_or_default(),
        test_parent_name.clone().unwrap_or_default(),
        test_classname.clone(),
        test_file.clone(),
        "test-org".to_string(),
        &repo,
        None,
        "variant2".to_string(),
    )
    .id;

    // Verify they're different
    assert_ne!(base_test_id, expected_test_id_variant1);
    assert_ne!(base_test_id, expected_test_id_variant2);
    assert_ne!(expected_test_id_variant1, expected_test_id_variant2);

    // Create a custom mock server handler that returns quarantined tests
    // We need to return both IDs: one for "without ID" case and one for "with ID" case
    use api::message::GetQuarantineConfigResponse;
    let state = {
        let mut builder = MockServerBuilder::new();
        let expected_id_v1 = expected_test_id_variant1.clone();
        let expected_id_v1_from_base = expected_test_id_variant1_from_base.clone();
        builder.set_get_quarantining_config_handler(
            move |_state: State<SharedMockServerState>,
                  _req: Json<api::message::GetQuarantineConfigRequest>| async move {
                Json(GetQuarantineConfigResponse {
                    is_disabled: false,
                    quarantined_tests: vec![expected_id_v1, expected_id_v1_from_base],
                })
            },
        );
        builder.spawn_mock_server().await
    };

    unsafe {
        env::set_var(TRUNK_PUBLIC_API_ADDRESS_ENV, &state.host);
        env::set_var(TRUNK_API_TOKEN_ENV, "test-token");
        env::set_var(TRUNK_ORG_URL_SLUG_ENV, "test-org");
        env::set_var(TRUNK_USE_UNCLONED_REPO_ENV, "false");
        env::set_var(TRUNK_QUARANTINED_TESTS_DISK_CACHE_TTL_SECS_ENV, "0");

        // Test with variant1 - should find the quarantined test (without ID)
        env::set_var(TRUNK_VARIANT_ENV, "variant1");
    }
    let test_name_v1 = test_name.clone();
    let test_parent_name_v1 = test_parent_name.clone();
    let test_classname_v1 = test_classname.clone();
    let test_file_v1 = test_file.clone();
    let is_quarantined_v1 = thread::spawn(move || {
        let test_report_v1 = MutTestReport::new("test".into(), "test-command".into(), None);
        test_report_v1.is_quarantined(
            None,
            test_name_v1,
            test_parent_name_v1,
            test_classname_v1,
            test_file_v1,
        )
    })
    .join()
    .unwrap();
    assert!(
        is_quarantined_v1,
        "Test should be quarantined when variant matches (without ID)"
    );
    assert!(
        !is_quarantined_v1.quarantining_disabled_for_repo,
        "Quarantining should not be disabled for the repo"
    );
    assert!(
        !is_quarantined_v1.quarantine_lookup_failed,
        "Quarantine lookup should not fail"
    );

    // Test with variant1 - should find the quarantined test (with ID)
    unsafe {
        env::set_var(TRUNK_VARIANT_ENV, "variant1");
    }
    let test_name_v1_id = test_name.clone();
    let test_parent_name_v1_id = test_parent_name.clone();
    let test_classname_v1_id = test_classname.clone();
    let test_file_v1_id = test_file.clone();
    let base_id_v1 = base_test_id.clone();
    let is_quarantined_v1_id = thread::spawn(move || {
        let test_report_v1 = MutTestReport::new("test".into(), "test-command".into(), None);
        test_report_v1.is_quarantined(
            Some(base_id_v1),
            test_name_v1_id,
            test_parent_name_v1_id,
            test_classname_v1_id,
            test_file_v1_id,
        )
    })
    .join()
    .unwrap();
    assert!(
        is_quarantined_v1_id,
        "Test should be quarantined when variant matches (with ID)"
    );

    // Test with variant2 - should NOT find the quarantined test (different variant, without ID)
    unsafe {
        env::set_var(TRUNK_VARIANT_ENV, "variant2");
    }
    let test_name_v2 = test_name.clone();
    let test_parent_name_v2 = test_parent_name.clone();
    let test_classname_v2 = test_classname.clone();
    let test_file_v2 = test_file.clone();
    let is_quarantined_v2 = thread::spawn(move || {
        let test_report_v2 = MutTestReport::new("test".into(), "test-command".into(), None);
        test_report_v2.is_quarantined(
            None,
            test_name_v2,
            test_parent_name_v2,
            test_classname_v2,
            test_file_v2,
        )
    })
    .join()
    .unwrap();
    assert!(
        !is_quarantined_v2,
        "Test should NOT be quarantined when variant doesn't match (without ID)"
    );

    // Test with variant2 - should NOT find the quarantined test (different variant, with ID)
    unsafe {
        env::set_var(TRUNK_VARIANT_ENV, "variant2");
    }
    let test_name_v2_id = test_name.clone();
    let test_parent_name_v2_id = test_parent_name.clone();
    let test_classname_v2_id = test_classname.clone();
    let test_file_v2_id = test_file.clone();
    let base_id_v2 = base_test_id.clone();
    let is_quarantined_v2_id = thread::spawn(move || {
        let test_report_v2 = MutTestReport::new("test".into(), "test-command".into(), None);
        test_report_v2.is_quarantined(
            Some(base_id_v2),
            test_name_v2_id,
            test_parent_name_v2_id,
            test_classname_v2_id,
            test_file_v2_id,
        )
    })
    .join()
    .unwrap();
    assert!(
        !is_quarantined_v2_id,
        "Test should NOT be quarantined when variant doesn't match (with ID)"
    );

    // Test with no variant - should NOT find the quarantined test (without ID)
    unsafe {
        env::remove_var(TRUNK_VARIANT_ENV);
    }
    let test_name_v3 = test_name.clone();
    let test_parent_name_v3 = test_parent_name.clone();
    let test_classname_v3 = test_classname.clone();
    let test_file_v3 = test_file.clone();
    let is_quarantined_v3 = thread::spawn(move || {
        let test_report_v3 = MutTestReport::new("test".into(), "test-command".into(), None);
        test_report_v3.is_quarantined(
            None,
            test_name_v3,
            test_parent_name_v3,
            test_classname_v3,
            test_file_v3,
        )
    })
    .join()
    .unwrap();
    assert!(
        !is_quarantined_v3,
        "Test should NOT be quarantined when variant is empty (without ID)"
    );

    // Test with no variant - should NOT find the quarantined test (with ID)
    unsafe {
        env::remove_var(TRUNK_VARIANT_ENV);
    }
    let test_name_v3_id = test_name.clone();
    let test_parent_name_v3_id = test_parent_name.clone();
    let test_classname_v3_id = test_classname.clone();
    let test_file_v3_id = test_file.clone();
    let base_id_v3 = base_test_id.clone();
    let is_quarantined_v3_id = thread::spawn(move || {
        let test_report_v3 = MutTestReport::new("test".into(), "test-command".into(), None);
        test_report_v3.is_quarantined(
            Some(base_id_v3),
            test_name_v3_id,
            test_parent_name_v3_id,
            test_classname_v3_id,
            test_file_v3_id,
        )
    })
    .join()
    .unwrap();
    assert!(
        !is_quarantined_v3_id,
        "Test should NOT be quarantined when variant is empty (with ID)"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn quarantine_disk_cache() {
    cleanup_env_vars();

    let temp_dir = tempdir().unwrap();
    setup_quarantine_disk_cache_dir(&temp_dir);
    clean_up_cache_files();

    let _ = env::set_current_dir(&temp_dir);

    let repo_url_1 = "https://github.com/test-org/test-repo-1.git";
    let repo_1 = RepoUrlParts::from_url(repo_url_1).unwrap();

    let test_name_1 = Some("test-name-1".to_string());
    let test_parent_name_1 = Some("test-parent-name-1".to_string());
    let test_classname_1 = Some("TestClass1".to_string());
    let test_file_1 = Some("test_file_1.rs".to_string());

    let repo_url_2 = "https://github.com/test-org/test-repo-2.git";
    let repo_2 = RepoUrlParts::from_url(repo_url_2).unwrap();

    let test_name_2 = Some("test-name-2".to_string());
    let test_parent_name_2 = Some("test-parent-name-2".to_string());
    let test_classname_2 = Some("TestClass2".to_string());
    let test_file_2 = Some("test_file_2.rs".to_string());

    unsafe {
        env::set_var(TRUNK_ORG_URL_SLUG_ENV, "test-org");
        env::set_var(TRUNK_REPO_URL_ENV, repo_url_1);
        env::set_var(TRUNK_USE_UNCLONED_REPO_ENV, "true");
        env::set_var(TRUNK_REPO_HEAD_SHA_ENV, "");
        env::set_var(TRUNK_REPO_HEAD_BRANCH_ENV, "");
        env::set_var(TRUNK_REPO_HEAD_AUTHOR_NAME_ENV, "");
    }

    use context::repo::BundleRepo;
    let bundle_repo_1 = BundleRepo::new(
        env::var(TRUNK_REPO_ROOT_ENV).ok(),
        env::var(TRUNK_REPO_URL_ENV).ok(),
        env::var(TRUNK_REPO_HEAD_SHA_ENV).ok(),
        env::var(TRUNK_REPO_HEAD_BRANCH_ENV).ok(),
        env::var(TRUNK_REPO_HEAD_COMMIT_EPOCH_ENV).ok(),
        env::var(TRUNK_REPO_HEAD_AUTHOR_NAME_ENV).ok(),
        true,
    )
    .unwrap();
    let computed_test_id_1 = Test::new(
        None,
        test_name_1.clone().unwrap_or_default(),
        test_parent_name_1.clone().unwrap_or_default(),
        test_classname_1.clone(),
        test_file_1.clone(),
        "test-org".to_string(),
        &bundle_repo_1.repo,
        None,
        "".to_string(),
    )
    .id;

    unsafe {
        env::set_var(TRUNK_REPO_URL_ENV, repo_url_2);
    }
    let bundle_repo_2 = BundleRepo::new(
        env::var(TRUNK_REPO_ROOT_ENV).ok(),
        env::var(TRUNK_REPO_URL_ENV).ok(),
        env::var(TRUNK_REPO_HEAD_SHA_ENV).ok(),
        env::var(TRUNK_REPO_HEAD_BRANCH_ENV).ok(),
        env::var(TRUNK_REPO_HEAD_COMMIT_EPOCH_ENV).ok(),
        env::var(TRUNK_REPO_HEAD_AUTHOR_NAME_ENV).ok(),
        true,
    )
    .unwrap();
    let computed_test_id_2 = Test::new(
        None,
        test_name_2.clone().unwrap_or_default(),
        test_parent_name_2.clone().unwrap_or_default(),
        test_classname_2.clone(),
        test_file_2.clone(),
        "test-org".to_string(),
        &bundle_repo_2.repo,
        None,
        "".to_string(),
    )
    .id;

    unsafe {
        env::set_var(TRUNK_REPO_URL_ENV, repo_url_1);
    }

    // test_id_1 and test_id_2 are quarantined
    use api::message::GetQuarantineConfigResponse;
    let computed_test_id_1_clone = computed_test_id_1.clone();
    let computed_test_id_2_clone = computed_test_id_2.clone();
    let state = {
        let mut builder = MockServerBuilder::new();
        builder.set_get_quarantining_config_handler(
            move |state: State<SharedMockServerState>,
                  req: Json<api::message::GetQuarantineConfigRequest>| async move {
                state
                    .requests
                    .lock()
                    .unwrap()
                    .push(RequestPayload::GetQuarantineConfig(req.0));
                Json(GetQuarantineConfigResponse {
                    is_disabled: false,
                    quarantined_tests: vec![
                        computed_test_id_1_clone.clone(),
                        computed_test_id_2_clone.clone(),
                    ],
                })
            },
        );
        builder.spawn_mock_server().await
    };

    unsafe {
        env::set_var(TRUNK_PUBLIC_API_ADDRESS_ENV, &state.host);
        env::set_var(TRUNK_API_TOKEN_ENV, "test-token");
    }

    let (test_1_is_quarantined, test_3_is_quarantined) = thread::spawn(|| {
        let test_report_1 = MutTestReport::new("test".into(), "test-command-1".into(), None);
        // call twice to also validate in-memory cache
        let test_1_is_quarantined = test_report_1.is_quarantined(
            None,
            Some("test-name-1".to_string()),
            Some("test-parent-name-1".to_string()),
            Some("TestClass1".to_string()),
            Some("test_file_1.rs".to_string()),
        );
        let test_3_is_quarantined = test_report_1.is_quarantined(
            None,
            Some("test-name-3".to_string()),
            Some("test-parent-name-3".to_string()),
            Some("TestClass3".to_string()),
            Some("test_file_3.rs".to_string()),
        );
        (test_1_is_quarantined, test_3_is_quarantined)
    })
    .join()
    .unwrap();
    assert!(test_1_is_quarantined);
    assert!(!test_3_is_quarantined);

    let (test_1_is_quarantined, test_3_is_quarantined) = thread::spawn(|| {
        let test_report_2 = MutTestReport::new("test".into(), "test-command-2".into(), None);
        // call twice to also validate in-memory cache
        let test_1_is_quarantined = test_report_2.is_quarantined(
            None,
            Some("test-name-1".to_string()),
            Some("test-parent-name-1".to_string()),
            Some("TestClass1".to_string()),
            Some("test_file_1.rs".to_string()),
        );
        let test_3_is_quarantined = test_report_2.is_quarantined(
            None,
            Some("test-name-3".to_string()),
            Some("test-parent-name-3".to_string()),
            Some("TestClass3".to_string()),
            Some("test_file_3.rs".to_string()),
        );
        (test_1_is_quarantined, test_3_is_quarantined)
    })
    .join()
    .unwrap();
    assert!(test_1_is_quarantined);
    assert!(!test_3_is_quarantined);

    {
        let requests = state.requests.lock().unwrap();
        let get_quarantine_config_count = requests
            .iter()
            .filter(|req| matches!(req, RequestPayload::GetQuarantineConfig(_)))
            .count();
        assert_eq!(
            get_quarantine_config_count, 1,
            "get_quarantine_config should be called only once across two test reports with the same repo"
        );
        let get_quarantine_config_request = requests
            .iter()
            .find_map(|req| match req {
                RequestPayload::GetQuarantineConfig(req) => Some(req),
                _ => None,
            })
            .unwrap();
        assert_eq!(get_quarantine_config_request.org_url_slug, "test-org");
        assert_eq!(get_quarantine_config_request.repo, repo_1,);
    }

    // third test report with different repo - should make a new API call due to cache miss
    unsafe {
        env::set_var(TRUNK_REPO_URL_ENV, repo_url_2);
    }

    thread::spawn(|| {
        let test_report_3 = MutTestReport::new("test".into(), "test-command-3".into(), None);
        // call twice to also validate in-memory cache
        let test_2_is_quarantined = test_report_3.is_quarantined(
            None,
            Some("test-name-2".to_string()),
            Some("test-parent-name-2".to_string()),
            Some("TestClass2".to_string()),
            Some("test_file_2.rs".to_string()),
        );
        let test_3_is_quarantined = test_report_3.is_quarantined(
            None,
            Some("test-name-3".to_string()),
            Some("test-parent-name-3".to_string()),
            Some("TestClass3".to_string()),
            Some("test_file_3.rs".to_string()),
        );
        assert!(test_2_is_quarantined);
        assert!(!test_3_is_quarantined);
    })
    .join()
    .unwrap();

    {
        let requests = state.requests.lock().unwrap();
        let get_quarantine_config_count = requests
            .iter()
            .filter(|req| matches!(req, RequestPayload::GetQuarantineConfig(_)))
            .count();
        assert_eq!(
            get_quarantine_config_count, 2,
            "get_quarantine_config should be called twice: once for test-repo-1 and once for test-repo-2"
        );
        let get_quarantine_config_requests: Vec<_> = requests
            .iter()
            .filter_map(|req| match req {
                RequestPayload::GetQuarantineConfig(req) => Some(req),
                _ => None,
            })
            .collect();
        assert_eq!(get_quarantine_config_requests.len(), 2);
        assert_eq!(get_quarantine_config_requests[1].repo, repo_2,);
    }
}
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn quarantine_disabled_for_repo() {
    cleanup_env_vars();

    let temp_dir = tempdir().unwrap();
    setup_quarantine_disk_cache_dir(&temp_dir);
    let _ = env::set_current_dir(&temp_dir);

    let repo_url = "https://github.com/test-org/test-repo.git";
    let test_name = Some("test-name".to_string());
    let test_parent_name = Some("test-parent-name".to_string());
    let test_classname = Some("TestClass".to_string());
    let test_file = Some("test_file.rs".to_string());

    unsafe {
        env::set_var(TRUNK_ORG_URL_SLUG_ENV, "test-org");
        env::set_var(TRUNK_REPO_URL_ENV, repo_url);
        env::set_var(TRUNK_USE_UNCLONED_REPO_ENV, "true");
        env::set_var(TRUNK_REPO_HEAD_SHA_ENV, "");
        env::set_var(TRUNK_REPO_HEAD_BRANCH_ENV, "");
        env::set_var(TRUNK_REPO_HEAD_AUTHOR_NAME_ENV, "");
    }

    let state = {
        let mut builder = MockServerBuilder::new();
        builder.set_get_quarantining_config_handler(
            move |_state: State<SharedMockServerState>,
                  _req: Json<GetQuarantineConfigRequest>| async move {
                Json(GetQuarantineConfigResponse {
                    is_disabled: true,
                    quarantined_tests: vec![],
                })
            },
        );
        builder.spawn_mock_server().await
    };

    unsafe {
        env::set_var(TRUNK_PUBLIC_API_ADDRESS_ENV, &state.host);
        env::set_var(TRUNK_API_TOKEN_ENV, "test-token");
        env::set_var(TRUNK_QUARANTINED_TESTS_DISK_CACHE_TTL_SECS_ENV, "0");
    }

    let test_name_spawn = test_name.clone();
    let test_parent_name_spawn = test_parent_name.clone();
    let test_classname_spawn = test_classname.clone();
    let test_file_spawn = test_file.clone();
    let result = thread::spawn(move || {
        let test_report = MutTestReport::new("test".into(), "test-command".into(), None);
        test_report.is_quarantined(
            None,
            test_name_spawn,
            test_parent_name_spawn,
            test_classname_spawn,
            test_file_spawn,
        )
    })
    .join()
    .unwrap();

    assert!(
        result.quarantining_disabled_for_repo,
        "Quarantining should be disabled for the repo"
    );
    assert!(
        !result.quarantine_lookup_failed,
        "Quarantine lookup should not fail"
    );
    assert!(
        !result.test_is_quarantined,
        "Test should not be quarantined"
    );
    assert!(!bool::from(result), "Test should not be quarantined");
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn quarantine_lookup_failed_when_endpoint_fails() {
    cleanup_env_vars();

    let temp_dir = tempdir().unwrap();
    setup_quarantine_disk_cache_dir(&temp_dir);
    clean_up_cache_files();

    let _ = env::set_current_dir(&temp_dir);

    let repo_url = "https://github.com/test-org/test-repo.git";

    unsafe {
        env::set_var(TRUNK_ORG_URL_SLUG_ENV, "test-org");
        env::set_var(TRUNK_REPO_URL_ENV, repo_url);
        env::set_var(TRUNK_USE_UNCLONED_REPO_ENV, "true");
        env::set_var(TRUNK_REPO_HEAD_SHA_ENV, "");
        env::set_var(TRUNK_REPO_HEAD_BRANCH_ENV, "");
        env::set_var(TRUNK_REPO_HEAD_AUTHOR_NAME_ENV, "");
    }

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

    unsafe {
        env::set_var(TRUNK_PUBLIC_API_ADDRESS_ENV, &state.host);
        env::set_var(TRUNK_API_TOKEN_ENV, "test-token");
        env::set_var(TRUNK_QUARANTINED_TESTS_DISK_CACHE_TTL_SECS_ENV, "0");
        env::set_var(TRUNK_API_CLIENT_RETRY_COUNT_ENV, "0");
    }

    let result = thread::spawn(|| {
        let test_report = MutTestReport::new("test".into(), "test-command".into(), None);
        test_report.is_quarantined(
            None,
            Some("test-name".to_string()),
            Some("test-parent-name".to_string()),
            Some("TestClass".to_string()),
            Some("test_file.rs".to_string()),
        )
    })
    .join()
    .unwrap();

    assert!(
        result.quarantine_lookup_failed,
        "Quarantine lookup should fail"
    );
    assert!(
        !result.quarantining_disabled_for_repo,
        "Quarantining should not be disabled for the repo"
    );
    assert!(
        !result.test_is_quarantined,
        "Test should not be quarantined"
    );
    assert!(!bool::from(result), "Test should not be quarantined");
}
