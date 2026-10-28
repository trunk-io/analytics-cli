use std::{collections::HashSet, time::Duration};

use api::client::ApiClient;
use api::message::{self, UploadApiCaller, UploadApiVersion};
use api::quarantine::{QuarantineLookup, QuarantinedTestSet, QuarantinedTests};
use api::urls::TestCaseGuidScope;
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, Uri},
    response::IntoResponse,
    routing::{MethodRouter, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bundle::Test;
use serde_json::{Value, json};
use test_utils::mock_server::{MockServerBuilder, RequestPayload, SharedMockServerState};

const SHORT_ID: &str = "aB3xY9kQ";
const COLLECTION_UUID: &str = "204b6c93-1c51-4e0f-a13c-e07b9bc60688";
const REPO_ID: &str = "7a1f0e3d-2b4c-4d5e-8f90-123456789abc";

const VERSION_ROUTE: &str = "/v1/metrics/getUploadApiVersion";
const UPLOADS_ROUTE: &str = "/v2/test-collections/:id/uploads";
const STATUS_ROUTE: &str = "/v2/test-collections/:id/quarantine-status";
const LIST_ROUTE: &str = "/v2/test-collections/:id/all-quarantined-tests";

type Routes = Vec<(&'static str, MethodRouter<SharedMockServerState>)>;

async fn serve(routes: Routes) -> SharedMockServerState {
    let mut builder = MockServerBuilder::new();
    for (path, method_router) in routes {
        builder.route(path, method_router);
    }
    builder.spawn_mock_server().await
}

fn client(state: &SharedMockServerState, token: Option<&str>) -> ApiClient {
    let mut client = ApiClient::new(token.map(String::from), None, false, "acme", None).unwrap();
    client.api_host.clone_from(&state.host);
    client
}

fn v2_client(state: &SharedMockServerState) -> ApiClient {
    let mut client = client(state, Some("org-key"));
    client.set_upload_api_version(UploadApiVersion::V2);
    client
}

fn uris(state: &SharedMockServerState) -> Vec<String> {
    state
        .route_requests
        .lock()
        .unwrap()
        .iter()
        .map(|request| request.uri.clone())
        .collect()
}

fn bodies(state: &SharedMockServerState) -> Vec<Value> {
    state
        .route_requests
        .lock()
        .unwrap()
        .iter()
        .filter_map(|request| request.body.clone())
        .collect()
}

fn authorization(state: &SharedMockServerState) -> Vec<Option<String>> {
    state
        .route_requests
        .lock()
        .unwrap()
        .iter()
        .map(|request| request.authorization.clone())
        .collect()
}

fn repo() -> context::repo::RepoUrlParts {
    context::repo::RepoUrlParts {
        host: String::from("github.com"),
        owner: String::from("acme"),
        name: String::from("web"),
    }
}

fn version_request(short_id: Option<&str>) -> message::GetUploadApiVersionRequest {
    message::GetUploadApiVersionRequest {
        cli_version: String::from("0.17.0"),
        org_slug: String::from("acme"),
        test_collection_short_id: short_id.map(String::from),
        repo: Some(repo()),
        ci_provider: Some(String::from("GitHubActions")),
        is_fork_pr: false,
        caller: UploadApiCaller::TestRunnerPlugin,
    }
}

fn quarantine_request(test_identifiers: Vec<Test>) -> message::GetQuarantineConfigRequest {
    message::GetQuarantineConfigRequest {
        repo: repo(),
        remote_urls: vec![String::from("https://github.com/acme/web.git")],
        org_url_slug: String::from("acme"),
        test_identifiers,
        test_collection_short_id: Some(String::from(SHORT_ID)),
    }
}

fn failure(index: usize) -> Test {
    Test {
        id: format!("00000000-0000-4000-8000-{index:012}"),
        name: format!("test {index}"),
        parent_name: String::from("Suite"),
        class_name: Some(String::new()),
        file: None,
        timestamp_millis: None,
        is_quarantined: false,
        failure_message: None,
        variant: None,
    }
}

fn scope() -> TestCaseGuidScope {
    TestCaseGuidScope {
        test_collection_id: String::from(COLLECTION_UUID),
        repo_id: String::from(REPO_ID),
    }
}

async fn decide(answer: (StatusCode, &'static str)) -> (UploadApiVersion, SharedMockServerState) {
    let state = serve(vec![(
        VERSION_ROUTE,
        post(
            move |State(state): State<SharedMockServerState>,
                  headers: HeaderMap,
                  uri: Uri,
                  Json(body): Json<Value>| async move {
                state.record_route_request(&headers, &uri, Some(body));
                answer
            },
        ),
    )])
    .await;
    let version = client(&state, Some("org-key"))
        .get_upload_api_version(&version_request(Some(SHORT_ID)))
        .await;
    (version, state)
}

fn quarantine_status_route(
    quarantine_enabled: bool,
) -> (&'static str, MethodRouter<SharedMockServerState>) {
    (
        STATUS_ROUTE,
        post(
            move |State(state): State<SharedMockServerState>,
                  headers: HeaderMap,
                  uri: Uri,
                  Json(body): Json<Value>| async move {
                let tests: Vec<Value> = body["testIdentifiers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|identifier| {
                        json!({
                            "clientTestId": identifier["id"],
                            "testId": null,
                            "quarantined": identifier["id"] != "00000000-0000-4000-8000-000000000000",
                        })
                    })
                    .collect();
                state.record_route_request(&headers, &uri, Some(body));
                Json(json!({
                    "testCollectionUuid": COLLECTION_UUID,
                    "repositoryId": REPO_ID,
                    "quarantineEnabled": quarantine_enabled,
                    "tests": tests,
                }))
            },
        ),
    )
}

fn all_quarantined_tests_route(
    quarantine_enabled: bool,
) -> (&'static str, MethodRouter<SharedMockServerState>) {
    (
        LIST_ROUTE,
        post(
            move |State(state): State<SharedMockServerState>,
                  headers: HeaderMap,
                  uri: Uri,
                  Query(query): Query<std::collections::HashMap<String, String>>,
                  Json(body): Json<Value>| async move {
                state.record_route_request(&headers, &uri, Some(body));
                let (data, next_cursor) = match query.get("cursor").map(String::as_str) {
                    _ if !quarantine_enabled => (json!([]), json!(null)),
                    None => (
                        json!([{"testId": "bfeebcf4-72d1-887d-8bcd-788d0dec7f97"}]),
                        json!("page-2"),
                    ),
                    Some("page-2") => (
                        json!([{"testId": "0f7f61f2-0000-8000-8000-000000000000"}]),
                        json!(null),
                    ),
                    Some(_) => return StatusCode::BAD_REQUEST.into_response(),
                };
                Json(json!({
                    "data": data,
                    "nextCursor": next_cursor,
                    "hasMore": !next_cursor.is_null(),
                    "testCollectionUuid": COLLECTION_UUID,
                    "repositoryId": REPO_ID,
                    "quarantineEnabled": quarantine_enabled,
                }))
                .into_response()
            },
        ),
    )
}

#[tokio::test]
async fn sends_the_run_and_reads_an_explicit_v2() {
    let (version, state) = decide((StatusCode::OK, r#"{"apiVersion":"v2"}"#)).await;

    assert_eq!(version, UploadApiVersion::V2);
    assert_eq!(
        bodies(&state),
        vec![json!({
            "cliVersion": "0.17.0",
            "orgSlug": "acme",
            "testCollectionShortId": SHORT_ID,
            "repo": {"host": "github.com", "owner": "acme", "name": "web"},
            "ciProvider": "GitHubActions",
            "isForkPr": false,
            "caller": "test-runner-plugin",
        })]
    );
}

#[tokio::test]
async fn reads_any_other_answer_as_v1() {
    for (answer, attempts) in [
        ((StatusCode::OK, r#"{"apiVersion":"v1"}"#), 1),
        ((StatusCode::OK, r#"{"apiVersion":"v3"}"#), 1),
        ((StatusCode::OK, "not json"), 1),
        ((StatusCode::UNAUTHORIZED, ""), 1),
        // Only a 5xx, a timeout or a failed connection is worth a second attempt.
        (
            (StatusCode::INTERNAL_SERVER_ERROR, r#"{"apiVersion":"v2"}"#),
            2,
        ),
    ] {
        let (version, state) = decide(answer).await;
        assert_eq!(version, UploadApiVersion::V1, "{answer:?}");
        assert_eq!(bodies(&state).len(), attempts, "{answer:?}");
    }
}

#[tokio::test]
async fn retries_a_failed_check_once() {
    let state = serve(vec![(
        VERSION_ROUTE,
        post(
            |State(state): State<SharedMockServerState>, headers: HeaderMap, uri: Uri| async move {
                state.record_route_request(&headers, &uri, None);
                if state.route_requests.lock().unwrap().len() == 1 {
                    StatusCode::SERVICE_UNAVAILABLE.into_response()
                } else {
                    Json(json!({"apiVersion": "v2"})).into_response()
                }
            },
        ),
    )])
    .await;

    let version = client(&state, Some("org-key"))
        .get_upload_api_version(&version_request(Some(SHORT_ID)))
        .await;

    assert_eq!((version, uris(&state).len()), (UploadApiVersion::V2, 2));
}

#[tokio::test(start_paused = true)]
async fn reads_a_slow_answer_as_v1() {
    let state = serve(vec![(
        VERSION_ROUTE,
        post(|| async {
            tokio::time::sleep(Duration::from_secs(3)).await;
            Json(json!({"apiVersion": "v2"}))
        }),
    )])
    .await;

    let version = client(&state, Some("org-key"))
        .get_upload_api_version(&version_request(Some(SHORT_ID)))
        .await;

    assert_eq!(version, UploadApiVersion::V1);
}

#[tokio::test]
async fn does_not_ask_without_a_token_or_a_collection() {
    let state = serve(vec![(
        VERSION_ROUTE,
        post(
            |State(state): State<SharedMockServerState>, headers: HeaderMap, uri: Uri| async move {
                state.record_route_request(&headers, &uri, None);
                Json(json!({"apiVersion": "v2"}))
            },
        ),
    )])
    .await;

    let mut public_repo_client =
        ApiClient::new(None, Some(String::from("public-id")), false, "acme", None).unwrap();
    public_repo_client.api_host.clone_from(&state.host);
    assert_eq!(
        public_repo_client
            .get_upload_api_version(&version_request(Some(SHORT_ID)))
            .await,
        UploadApiVersion::V1
    );
    for short_id in [None, Some("")] {
        assert_eq!(
            client(&state, Some("org-key"))
                .get_upload_api_version(&version_request(short_id))
                .await,
            UploadApiVersion::V1
        );
    }
    assert!(uris(&state).is_empty());
}

#[tokio::test]
async fn creates_a_v2_upload() {
    let state = serve(vec![(
        UPLOADS_ROUTE,
        post(
            |State(state): State<SharedMockServerState>,
             headers: HeaderMap,
             uri: Uri,
             Json(body): Json<Value>| async move {
                state.record_route_request(&headers, &uri, Some(body));
                (
                    StatusCode::CREATED,
                    Json(json!({
                        "id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
                        "status": "awaitingData",
                        "uploadUrl": "https://bundles.example/key?sig=1",
                        "expiresAt": "2026-10-06T17:00:00.000Z",
                        "testCollectionUuid": COLLECTION_UUID,
                        "repositoryId": REPO_ID,
                        "createdAt": "2026-10-06T16:00:00.000Z",
                    })),
                )
            },
        ),
    )])
    .await;

    let upload = v2_client(&state)
        .create_bundle_upload(&message::CreateBundleUploadRequest {
            repo: repo(),
            org_url_slug: String::from("acme"),
            client_version: String::from("trunk-analytics-cli 0.17.0"),
            remote_urls: vec![String::from("https://github.com/acme/web.git")],
            external_id: None,
            test_collection_short_id: Some(String::from(SHORT_ID)),
        })
        .await
        .unwrap();

    let id_v2: Value = serde_json::from_slice(&STANDARD.decode(&upload.id_v2).unwrap()).unwrap();
    assert_eq!(
        (
            upload.id.as_str(),
            id_v2,
            upload.url.as_str(),
            upload.test_collection_bundle_meta_id.as_deref(),
            upload.test_collection_bundle_meta_created_at.as_deref(),
            upload.repo_id.as_deref(),
            upload.test_collection_id.as_deref(),
        ),
        (
            "f47ac10b-58cc-4372-a567-0e02b2c3d479",
            json!({
                "id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
                "createdAt": "2026-10-06T16:00:00.000Z",
                "repoId": REPO_ID,
            }),
            "https://bundles.example/key?sig=1",
            Some("f47ac10b-58cc-4372-a567-0e02b2c3d479"),
            Some("2026-10-06T16:00:00.000Z"),
            Some(REPO_ID),
            // From the upload itself, so a run that makes no lookup still gets GUID links.
            Some(COLLECTION_UUID),
        )
    );
    assert_eq!(
        uris(&state),
        vec![format!("/v2/test-collections/{SHORT_ID}/uploads")]
    );
    assert_eq!(
        authorization(&state),
        vec![Some(String::from("Bearer org-key"))]
    );
    assert_eq!(
        bodies(&state),
        vec![json!({
            "repo": {"host": "github.com", "owner": "acme", "name": "web"},
            "remoteUrls": ["https://github.com/acme/web.git"],
            "clientVersion": "trunk-analytics-cli 0.17.0",
        })]
    );
}

#[tokio::test]
async fn looks_up_failures_in_batches_of_2000() {
    let state = serve(vec![quarantine_status_route(true)]).await;
    let failures: Vec<Test> = (0..2001).map(failure).collect();

    let config = v2_client(&state)
        .get_quarantining_config(&quarantine_request(failures.clone()))
        .await
        .unwrap();

    assert_eq!(
        config,
        QuarantineLookup {
            is_disabled: false,
            quarantined_tests: failures[1..].iter().map(|test| test.id.clone()).collect(),
            quarantine_resolution_mode: message::QuarantineResolutionMode::TestCollection,
            test_case_guid_scope: Some(scope()),
        }
    );
    let bodies = bodies(&state);
    assert_eq!(
        bodies
            .iter()
            .map(|body| body["testIdentifiers"].as_array().unwrap().len())
            .collect::<Vec<_>>(),
        vec![2000, 1]
    );
    assert_eq!(
        bodies[1],
        json!({
            "repo": {"host": "github.com", "owner": "acme", "name": "web"},
            "testIdentifiers": [{"id": failures[2000].id}],
        })
    );
    assert_eq!(
        authorization(&state),
        vec![Some(String::from("Bearer org-key")); 2]
    );
}

#[tokio::test]
async fn makes_no_request_to_look_up_no_tests() {
    let state = serve(vec![quarantine_status_route(true)]).await;

    let config = v2_client(&state)
        .get_quarantining_config(&quarantine_request(vec![]))
        .await
        .unwrap();

    assert_eq!(
        (config.quarantined_tests, config.test_case_guid_scope),
        (Vec::<String>::new(), None)
    );
    assert!(uris(&state).is_empty());
}

#[tokio::test]
async fn pages_the_whole_quarantined_set_with_no_status_call() {
    let state = serve(vec![
        all_quarantined_tests_route(true),
        quarantine_status_route(true),
    ])
    .await;

    let set = v2_client(&state)
        .get_quarantined_test_set(&quarantine_request(vec![]))
        .await
        .unwrap();

    assert_eq!(
        set,
        QuarantinedTestSet {
            is_disabled: false,
            quarantine_resolution_mode: message::QuarantineResolutionMode::TestCollection,
            tests: QuarantinedTests::ByTestCaseGuid {
                scope: scope(),
                guids: HashSet::from([
                    String::from("bfeebcf4-72d1-887d-8bcd-788d0dec7f97"),
                    String::from("0f7f61f2-0000-8000-8000-000000000000"),
                ]),
            },
        }
    );
    assert_eq!(
        uris(&state),
        vec![
            format!("/v2/test-collections/{SHORT_ID}/all-quarantined-tests?limit=25000"),
            format!(
                "/v2/test-collections/{SHORT_ID}/all-quarantined-tests?limit=25000&cursor=page-2"
            ),
        ]
    );
    let repo = json!({"repo": {"host": "github.com", "owner": "acme", "name": "web"}});
    assert_eq!(bodies(&state), vec![repo.clone(), repo]);
}

#[tokio::test]
async fn names_no_repository_for_an_upload_without_one() {
    let state = serve(vec![all_quarantined_tests_route(false)]).await;
    let request = message::GetQuarantineConfigRequest {
        repo: context::repo::RepoUrlParts::default(),
        ..quarantine_request(vec![])
    };

    v2_client(&state)
        .get_quarantined_test_set(&request)
        .await
        .unwrap();

    assert_eq!(
        uris(&state),
        vec![format!(
            "/v2/test-collections/{SHORT_ID}/all-quarantined-tests?limit=25000"
        )]
    );
    assert_eq!(bodies(&state), vec![json!({"repo": null})]);
}

#[tokio::test]
async fn reads_quarantining_off_from_the_list() {
    let state = serve(vec![all_quarantined_tests_route(false)]).await;

    let set = v2_client(&state)
        .get_quarantined_test_set(&quarantine_request(vec![]))
        .await
        .unwrap();

    assert_eq!(
        (set.is_disabled, set.tests),
        (
            true,
            QuarantinedTests::ByTestCaseGuid {
                scope: scope(),
                guids: HashSet::new(),
            },
        )
    );
    assert_eq!(uris(&state).len(), 1);
}

#[tokio::test]
async fn stops_paging_on_a_repeated_cursor() {
    let state = serve(vec![(
        LIST_ROUTE,
        post(|| async {
            Json(json!({
                "data": [],
                "nextCursor": "same",
                "hasMore": true,
                "testCollectionUuid": COLLECTION_UUID,
                "repositoryId": REPO_ID,
                "quarantineEnabled": true,
            }))
        }),
    )])
    .await;

    let error = v2_client(&state)
        .get_quarantined_test_set(&quarantine_request(vec![]))
        .await
        .unwrap_err();

    assert!(
        format!("{error:?}").contains("repeated its cursor"),
        "{error:?}"
    );
}

#[tokio::test]
async fn reports_a_failed_v2_lookup_without_falling_back_to_v1() {
    let state = serve(vec![(
        STATUS_ROUTE,
        post(
            |State(state): State<SharedMockServerState>, headers: HeaderMap, uri: Uri| async move {
                state.record_route_request(&headers, &uri, None);
                (
                    StatusCode::NOT_FOUND,
                    r#"{"error":{"code":"NOT_FOUND","message":"Test collection not found."}}"#,
                )
            },
        ),
    )])
    .await;

    let error = v2_client(&state)
        .get_quarantining_config(&quarantine_request(vec![failure(0)]))
        .await
        .unwrap_err();

    assert!(format!("{error:?}").contains("404 Not Found"), "{error:?}");
    assert_eq!(
        uris(&state),
        vec![format!("/v2/test-collections/{SHORT_ID}/quarantine-status")]
    );
    // The builder serves v1's lookup, so a fallback would have reached it.
    assert!(
        !state
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|request| matches!(request, RequestPayload::GetQuarantineConfig(_)))
    );
}
