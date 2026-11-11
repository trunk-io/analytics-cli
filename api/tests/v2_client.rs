use std::collections::HashSet;

use api::client::{ApiClient, ClientErrorExplanation, GithubForkPullRequest};
use api::credential::LegacyTokenRefused;
use api::message;
use api::quarantine::{QuarantineLookup, QuarantinedTestSet};
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
use test_utils::mock_server::{MockServerBuilder, SharedMockServerState};

const SHORT_ID: &str = "aB3xY9kQ";
const COLLECTION_UUID: &str = "204b6c93-1c51-4e0f-a13c-e07b9bc60688";
const REPO_ID: &str = "7a1f0e3d-2b4c-4d5e-8f90-123456789abc";

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
    let mut client = ApiClient::new(token.map(String::from), false, "acme", None).unwrap();
    client.api_host.clone_from(&state.host);
    client
}

fn v2_client(state: &SharedMockServerState) -> ApiClient {
    client(state, Some("trunk_v2_org-key"))
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
        failure_message_attribute: None,
        failure_text: None,
        variant: None,
    }
}

fn scope() -> TestCaseGuidScope {
    TestCaseGuidScope {
        test_collection_id: String::from(COLLECTION_UUID),
        repo_id: String::from(REPO_ID),
    }
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
        vec![Some(String::from("Bearer trunk_v2_org-key"))]
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
        vec![Some(String::from("Bearer trunk_v2_org-key")); 2]
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
            scope: scope(),
            guids: HashSet::from([
                String::from("bfeebcf4-72d1-887d-8bcd-788d0dec7f97"),
                String::from("0f7f61f2-0000-8000-8000-000000000000"),
            ]),
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
        (set.is_disabled, set.scope, set.guids),
        (true, scope(), HashSet::new())
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
async fn reports_a_failed_lookup() {
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
}

const LOGIN_ROUTE: &str = "/v2/auth/github-actions/login";
const HEAD_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn fork_pull_request(github_token: Option<&str>) -> GithubForkPullRequest {
    GithubForkPullRequest {
        number: 42,
        head_sha: String::from(HEAD_SHA),
        github_token: github_token.map(String::from),
    }
}

fn fork_pr_client(state: &SharedMockServerState, github_token: Option<&str>) -> ApiClient {
    let mut client = ApiClient::new(None, true, "acme", None).unwrap();
    client.api_host.clone_from(&state.host);
    client.set_github_fork_pull_request(fork_pull_request(github_token));
    client
}

fn login_route(
    answer: (StatusCode, &'static str),
) -> (&'static str, MethodRouter<SharedMockServerState>) {
    (
        LOGIN_ROUTE,
        post(
            move |State(state): State<SharedMockServerState>,
                  headers: HeaderMap,
                  uri: Uri,
                  Json(body): Json<Value>| async move {
                assert_eq!(
                    headers
                        .get("x-trunk-allow-forked-pr-uploads")
                        .and_then(|value| value.to_str().ok()),
                    Some("true")
                );
                state.record_route_request(&headers, &uri, Some(body));
                answer
            },
        ),
    )
}

const LOGIN: &str = r#"{"jwt":"trunk-jwt","expiresAt":"2026-11-06T17:15:00Z","organization":{"id":"o","slug":"acme","name":"Acme"},"repository":{"id":"r","name":"acme/web"}}"#;

fn uploads_route() -> (&'static str, MethodRouter<SharedMockServerState>) {
    (
        UPLOADS_ROUTE,
        post(
            |State(state): State<SharedMockServerState>, headers: HeaderMap, uri: Uri| async move {
                state.record_route_request(&headers, &uri, None);
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
    )
}

fn upload_request() -> message::CreateBundleUploadRequest {
    message::CreateBundleUploadRequest {
        repo: repo(),
        org_url_slug: String::from("acme"),
        client_version: String::from("trunk-analytics-cli 0.17.0"),
        remote_urls: vec![],
        external_id: None,
        test_collection_short_id: Some(String::from(SHORT_ID)),
    }
}

#[tokio::test]
async fn a_fork_pr_run_swaps_its_github_token_and_sends_the_login_on_x_trunk_token() {
    let state = serve(vec![
        login_route((StatusCode::OK, LOGIN)),
        quarantine_status_route(true),
        all_quarantined_tests_route(true),
        uploads_route(),
    ])
    .await;
    let mut client = fork_pr_client(&state, Some("ghs_job-token"));

    client.log_in_github_fork_pull_request().await;
    client
        .get_quarantining_config(&quarantine_request(vec![failure(0)]))
        .await
        .unwrap();
    client
        .get_quarantined_test_set(&quarantine_request(vec![]))
        .await
        .unwrap();
    client
        .create_bundle_upload(&upload_request())
        .await
        .unwrap();

    let requests = state.route_requests.lock().unwrap().clone();
    let route = |uri: &str| uri.split('?').next().unwrap().to_string();
    let with_login = |path: String| (path, None, Some(String::from("trunk-jwt")));
    assert_eq!(
        requests
            .iter()
            .map(|request| (
                route(&request.uri),
                request.authorization.clone(),
                request.trunk_token.clone()
            ))
            .collect::<Vec<_>>(),
        vec![
            (String::from(LOGIN_ROUTE), None, None),
            with_login(format!("/v2/test-collections/{SHORT_ID}/quarantine-status")),
            with_login(format!(
                "/v2/test-collections/{SHORT_ID}/all-quarantined-tests"
            )),
            with_login(format!(
                "/v2/test-collections/{SHORT_ID}/all-quarantined-tests"
            )),
            with_login(format!("/v2/test-collections/{SHORT_ID}/uploads")),
        ]
    );
    assert_eq!(
        requests[0].body,
        Some(json!({
            "githubToken": "ghs_job-token",
            "pullRequestNumber": 42,
            "sha": HEAD_SHA,
        }))
    );
}

#[tokio::test]
async fn a_fork_pr_run_whose_swap_is_refused_fails_every_call_with_the_reason() {
    for (answer, reason) in [
        (
            (
                StatusCode::BAD_REQUEST,
                r#"{"error":{"code":"INVALID_ARGUMENT","message":"bad"}}"#,
            ),
            "Trunk answered 400 Bad Request, INVALID_ARGUMENT",
        ),
        (
            (
                StatusCode::UNAUTHORIZED,
                r#"{"error":{"code":"UNAUTHENTICATED","message":"no"}}"#,
            ),
            "Trunk answered 401 Unauthorized, UNAUTHENTICATED",
        ),
        (
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                r#"{"error":{"code":"SHA_NOT_PR_HEAD","message":"moved"}}"#,
            ),
            "Trunk answered 422 Unprocessable Entity, SHA_NOT_PR_HEAD",
        ),
        (
            (StatusCode::BAD_GATEWAY, ""),
            "Trunk answered 502 Bad Gateway",
        ),
        (
            (StatusCode::SERVICE_UNAVAILABLE, ""),
            "Trunk answered 503 Service Unavailable",
        ),
        (
            (StatusCode::TOO_MANY_REQUESTS, ""),
            "Trunk answered 429 Too Many Requests",
        ),
        (
            (StatusCode::OK, r#"{"expiresAt":"2026-11-06T17:15:00Z"}"#),
            "Trunk's answer could not be read",
        ),
    ] {
        let state = serve(vec![login_route(answer), uploads_route()]).await;
        let mut client = fork_pr_client(&state, Some("ghs_job-token"));

        client.log_in_github_fork_pull_request().await;
        let error = client
            .create_bundle_upload(&upload_request())
            .await
            .unwrap_err();

        assert!(
            format!("{error:?}").contains(reason),
            "{answer:?}: {error:?}"
        );
        assert_eq!(uris(&state), vec![LOGIN_ROUTE], "{answer:?}");
    }
}

#[tokio::test]
async fn a_fork_pr_run_without_github_token_makes_no_request() {
    for github_token in [None, Some(""), Some("  ")] {
        let state = serve(vec![login_route((StatusCode::OK, LOGIN)), uploads_route()]).await;
        let mut client = fork_pr_client(&state, github_token);

        client.log_in_github_fork_pull_request().await;
        let error = client
            .create_bundle_upload(&upload_request())
            .await
            .unwrap_err();

        assert!(
            format!("{error:?}").contains("GITHUB_TOKEN is not set"),
            "{github_token:?}: {error:?}"
        );
        assert!(uris(&state).is_empty(), "{github_token:?}");
    }
}

#[tokio::test]
async fn a_run_with_a_token_never_swaps() {
    let state = serve(vec![login_route((StatusCode::OK, LOGIN)), uploads_route()]).await;
    let mut client =
        ApiClient::new(Some(String::from("trunk_v2_org-key")), true, "acme", None).unwrap();
    client.api_host.clone_from(&state.host);
    client.set_github_fork_pull_request(fork_pull_request(Some("ghs_job-token")));

    client.log_in_github_fork_pull_request().await;
    client
        .create_bundle_upload(&upload_request())
        .await
        .unwrap();

    let requests = state.route_requests.lock().unwrap().clone();
    assert_eq!(
        requests
            .iter()
            .map(|request| (
                request.uri.clone(),
                request.authorization.clone(),
                request.trunk_token.clone()
            ))
            .collect::<Vec<_>>(),
        vec![(
            format!("/v2/test-collections/{SHORT_ID}/uploads"),
            Some(String::from("Bearer trunk_v2_org-key")),
            None
        )]
    );
}

// {"alg":"RS256","typ":"JWT"}.{"sub":"org-token:1","exp":1762880400}.signature, which expired at
// 2025-11-11T17:00:00Z.
const EXPIRED_JWT: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJvcmctdG9rZW46MSIsImV4cCI6MTc2Mjg4MDQwMH0.c2lnbmF0dXJl";

fn credential_headers(
    state: &SharedMockServerState,
) -> Vec<(String, Option<String>, Option<String>)> {
    state
        .route_requests
        .lock()
        .unwrap()
        .iter()
        .map(|request| {
            (
                request.uri.split('?').next().unwrap().to_string(),
                request.authorization.clone(),
                request.trunk_token.clone(),
            )
        })
        .collect()
}

#[tokio::test]
async fn sends_a_jwt_on_x_trunk_token_and_never_as_a_bearer() {
    let state = serve(vec![
        quarantine_status_route(true),
        all_quarantined_tests_route(true),
        uploads_route(),
    ])
    .await;
    let client = client(&state, Some(EXPIRED_JWT));

    client
        .get_quarantining_config(&quarantine_request(vec![failure(0)]))
        .await
        .unwrap();
    client
        .get_quarantined_test_set(&quarantine_request(vec![]))
        .await
        .unwrap();
    client
        .create_bundle_upload(&upload_request())
        .await
        .unwrap();

    let with_jwt = |path: String| (path, None, Some(String::from(EXPIRED_JWT)));
    assert_eq!(
        credential_headers(&state),
        vec![
            with_jwt(format!("/v2/test-collections/{SHORT_ID}/quarantine-status")),
            with_jwt(format!(
                "/v2/test-collections/{SHORT_ID}/all-quarantined-tests"
            )),
            with_jwt(format!(
                "/v2/test-collections/{SHORT_ID}/all-quarantined-tests"
            )),
            with_jwt(format!("/v2/test-collections/{SHORT_ID}/uploads")),
        ]
    );
}

#[tokio::test]
async fn sends_an_api_key_as_a_bearer_and_never_on_x_trunk_token() {
    let state = serve(vec![quarantine_status_route(true), uploads_route()]).await;
    // Surrounding whitespace, as a secret pasted with a newline has, is not sent.
    let client = client(&state, Some(" trunk_v2_org-key\n"));

    client
        .get_quarantining_config(&quarantine_request(vec![failure(0)]))
        .await
        .unwrap();
    client
        .create_bundle_upload(&upload_request())
        .await
        .unwrap();

    let with_key = |path: String| (path, Some(String::from("Bearer trunk_v2_org-key")), None);
    assert_eq!(
        credential_headers(&state),
        vec![
            with_key(format!("/v2/test-collections/{SHORT_ID}/quarantine-status")),
            with_key(format!("/v2/test-collections/{SHORT_ID}/uploads")),
        ]
    );
}

#[test]
fn refuses_an_organization_token_before_any_call() {
    for token in ["0123456789abcdef0123456789abcdef", "org.token", "a.b.c.d"] {
        let error = ApiClient::new(Some(String::from(token)), false, "acme", None)
            .err()
            .unwrap_or_else(|| panic!("{token:?} must be refused"));

        assert_eq!(
            error.downcast_ref::<LegacyTokenRefused>(),
            Some(&LegacyTokenRefused),
            "{token:?}"
        );
        let message = error.to_string();
        for part in [
            "analytics-cli 1.0.0 needs a v2 API key",
            "Settings → Developer → API Keys",
            "Upload test results",
            "pin analytics-cli 0.17.x",
        ] {
            assert!(message.contains(part), "{message}");
        }
    }
}

fn refusing_route(
    route: &'static str,
    status: StatusCode,
    body: &'static str,
) -> (&'static str, MethodRouter<SharedMockServerState>) {
    (route, post(move || async move { (status, body) }))
}

const INSUFFICIENT_PERMISSIONS: &str = r#"{"error":{"code":"INSUFFICIENT_PERMISSIONS","message":"You do not have permission to upload test results in this organization"}}"#;

/// Each CLI endpoint's error for `status` and `body`, called with `token`.
async fn refusals(token: &str, status: StatusCode, body: &'static str) -> Vec<String> {
    let state = serve(vec![
        refusing_route(UPLOADS_ROUTE, status, body),
        refusing_route(STATUS_ROUTE, status, body),
        refusing_route(LIST_ROUTE, status, body),
    ])
    .await;
    let client = client(&state, Some(token));
    vec![
        client
            .create_bundle_upload(&upload_request())
            .await
            .unwrap_err(),
        client
            .get_quarantining_config(&quarantine_request(vec![failure(0)]))
            .await
            .unwrap_err(),
        client
            .get_quarantined_test_set(&quarantine_request(vec![]))
            .await
            .unwrap_err(),
    ]
    .into_iter()
    .map(|error| {
        error
            .downcast_ref::<ClientErrorExplanation>()
            .and_then(|explanation| explanation.message.clone())
            .unwrap_or_default()
    })
    .collect()
}

#[tokio::test]
async fn names_the_missing_upload_permission_on_a_403() {
    for (token, refused) in [
        ("trunk_v2_org-key", "Trunk refused this API key: it lacks"),
        (
            EXPIRED_JWT,
            "Trunk refused this JWT: the API key it was minted from lacks",
        ),
    ] {
        for message in refusals(token, StatusCode::FORBIDDEN, INSUFFICIENT_PERMISSIONS).await {
            assert!(message.starts_with(refused), "{message}");
            assert!(
                message.contains(
                    "the Upload test results (`upload_test_results`) permission group. Grant it by editing the key in the Trunk web app under Settings → Developer → API Keys."
                ),
                "{message}"
            );
        }
    }
}

#[tokio::test]
async fn keeps_the_api_explanation_of_any_other_403() {
    let body = r#"{"error":{"code":"SOMETHING_ELSE","message":"Not for you."}}"#;
    assert_eq!(
        refusals("trunk_v2_org-key", StatusCode::FORBIDDEN, body).await,
        vec![String::from("Not for you."); 3]
    );
}

#[tokio::test]
async fn says_a_jwt_has_expired_on_a_401() {
    // The edge refuses an expired JWT before public-api sees it, with no envelope.
    for message in refusals(EXPIRED_JWT, StatusCode::UNAUTHORIZED, "Jwt is expired").await {
        assert!(
            message.starts_with(
                "Trunk did not accept this JWT: it is expired or invalid (it expired at 2025-11-11T17:00:00Z)."
            ),
            "{message}"
        );
        for part in [
            "re-run `trunk login --api-key` right before the upload step",
            "pass a v2 API key with --token or TRUNK_API_TOKEN",
        ] {
            assert!(message.contains(part), "{message}");
        }
    }
}

#[tokio::test]
async fn says_an_api_key_was_not_accepted_on_a_401() {
    let body = r#"{"error":{"code":"INVALID_TOKEN","message":"Invalid token"}}"#;
    for message in refusals("trunk_v2_org-key", StatusCode::UNAUTHORIZED, body).await {
        assert!(
            message.starts_with("Trunk did not accept this API key"),
            "{message}"
        );
    }
}
