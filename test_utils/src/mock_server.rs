use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use api::message::{
    CreatedTestUpload, QuarantineStatus, QuarantineStatusTest, QuarantinedTest, QuarantinedTestList,
};
use api::urls::TestCaseGuidScope;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    handler::Handler,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{MethodRouter, any, post, put},
};
use context::repo::RepoUrlParts;
use proto::upload_metrics::trunk::UploadMetrics;
use serde::Deserialize;
use tempfile::tempdir;
use tokio::{net::TcpListener, spawn};

pub const MOCK_REPO_ID: &str = "7a1f0e3d-2b4c-4d5e-8f90-123456789abc";
pub const MOCK_TEST_COLLECTION_ID: &str = "018f6d3a-6f2e-4c4a-9b1e-2f3a4b5c6d7e";
pub const MOCK_UPLOAD_ID: &str = "82c6a6e5-f8ea-4d93-9a26-b8ab6ff8f6bc";
pub const MOCK_UPLOAD_CREATED_AT: &str = "2026-05-10T12:34:56.000Z";

pub const UPLOADS_ROUTE: &str = "/v2/test-collections/:id/uploads";
pub const QUARANTINE_STATUS_ROUTE: &str = "/v2/test-collections/:id/quarantine-status";
pub const ALL_QUARANTINED_TESTS_ROUTE: &str = "/v2/test-collections/:id/all-quarantined-tests";

#[derive(Debug, Clone, PartialEq)]
pub enum RequestPayload {
    CreateTestUpload(CreateTestUpload),
    QuarantineStatus(QuarantineStatusLookup),
    AllQuarantinedTests(AllQuarantinedTestsLookup),
    S3Upload(PathBuf),
    TelemetryUploadMetrics(UploadMetrics),
}

/// `POST /v2/test-collections/{id}/uploads`, with the collection from its path.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTestUpload {
    #[serde(skip)]
    pub test_collection_short_id: String,
    pub repo: Option<RepoUrlParts>,
    pub remote_urls: Vec<String>,
    pub client_version: String,
    #[serde(default)]
    pub external_id: Option<String>,
}

/// `POST /v2/test-collections/{id}/quarantine-status`, with the collection from its path.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineStatusLookup {
    #[serde(skip)]
    pub test_collection_short_id: String,
    pub repo: Option<RepoUrlParts>,
    pub test_identifiers: Vec<QuarantineStatusIdentifier>,
}

impl QuarantineStatusLookup {
    pub fn test_ids(&self) -> Vec<String> {
        self.test_identifiers
            .iter()
            .map(|identifier| identifier.id.clone())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct QuarantineStatusIdentifier {
    pub id: String,
}

/// `POST /v2/test-collections/{id}/all-quarantined-tests`, with the collection from its path and
/// the cursor from its query.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllQuarantinedTestsLookup {
    #[serde(skip)]
    pub test_collection_short_id: String,
    #[serde(skip)]
    pub cursor: Option<String>,
    pub repo: Option<RepoUrlParts>,
}

/// The scope every mock response names, which a test runner's GUIDs are computed from.
pub fn mock_test_case_guid_scope() -> TestCaseGuidScope {
    TestCaseGuidScope {
        test_collection_id: String::from(MOCK_TEST_COLLECTION_ID),
        repo_id: String::from(MOCK_REPO_ID),
    }
}

/// The GUID the mock scope gives a test id, as `all-quarantined-tests` lists it.
pub fn mock_test_case_guid(test_id: &str) -> String {
    mock_test_case_guid_scope()
        .test_case_guid(test_id)
        .expect("a test id is a UUID")
        .to_string()
}

pub fn created_test_upload(host: &str) -> CreatedTestUpload {
    CreatedTestUpload {
        id: String::from(MOCK_UPLOAD_ID),
        upload_url: format!("{host}/s3upload"),
        test_collection_uuid: String::from(MOCK_TEST_COLLECTION_ID),
        repository_id: String::from(MOCK_REPO_ID),
        created_at: String::from(MOCK_UPLOAD_CREATED_AT),
    }
}

/// A `quarantine-status` answer naming each looked-up test, quarantined when it is in
/// `quarantined_test_ids`.
pub fn quarantine_status<I, S>(
    lookup: &QuarantineStatusLookup,
    quarantine_enabled: bool,
    quarantined_test_ids: I,
) -> QuarantineStatus
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let quarantined: HashSet<String> = quarantined_test_ids.into_iter().map(Into::into).collect();
    QuarantineStatus {
        test_collection_uuid: String::from(MOCK_TEST_COLLECTION_ID),
        repository_id: String::from(MOCK_REPO_ID),
        quarantine_enabled,
        tests: lookup
            .test_ids()
            .into_iter()
            .map(|id| QuarantineStatusTest {
                quarantined: quarantined.contains(&id),
                client_test_id: Some(id),
            })
            .collect(),
    }
}

/// A one-page `all-quarantined-tests` answer listing the GUIDs of `quarantined_test_ids`.
pub fn quarantined_test_list<I, S>(
    quarantine_enabled: bool,
    quarantined_test_ids: I,
) -> QuarantinedTestList
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    QuarantinedTestList {
        data: quarantined_test_ids
            .into_iter()
            .map(|id| QuarantinedTest {
                test_id: mock_test_case_guid(id.as_ref()),
            })
            .collect(),
        next_cursor: None,
        test_collection_uuid: String::from(MOCK_TEST_COLLECTION_ID),
        repository_id: String::from(MOCK_REPO_ID),
        quarantine_enabled,
    }
}

pub struct RespError {
    pub code: StatusCode,
    pub msg: String,
}
impl IntoResponse for RespError {
    fn into_response(self) -> axum::response::Response {
        (self.code, self.msg).into_response()
    }
}

#[derive(Debug, Default)]
pub struct MockServerState {
    pub requests: Mutex<Vec<RequestPayload>>,
    /// Request headers captured per endpoint (keyed by endpoint name, e.g.
    /// "uploads", "quarantineStatus" or "allQuarantinedTests"), so tests can assert on auth headers.
    pub request_headers: Mutex<HashMap<String, HeaderMap>>,
    /// Requests to the routes added with `MockServerBuilder::route`, in arrival order. Those
    /// handlers are the test's own, so each pushes its request with `record_route_request`.
    pub route_requests: Mutex<Vec<RouteRequest>>,
    pub host: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouteRequest {
    pub uri: String,
    pub authorization: Option<String>,
    pub trunk_token: Option<String>,
    pub body: Option<serde_json::Value>,
}

impl MockServerState {
    pub fn record_route_request(
        &self,
        headers: &HeaderMap,
        uri: &axum::http::Uri,
        body: Option<serde_json::Value>,
    ) {
        let header = |name| {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(String::from)
        };
        self.route_requests.lock().unwrap().push(RouteRequest {
            uri: uri.to_string(),
            authorization: header("authorization"),
            trunk_token: header("x-trunk-token"),
            body,
        });
    }
}

#[derive(Debug, Clone)]
pub struct MockServerBuilder {
    create_test_upload_handler: MethodRouter<SharedMockServerState>,
    quarantine_status_handler: MethodRouter<SharedMockServerState>,
    all_quarantined_tests_handler: MethodRouter<SharedMockServerState>,
    s3_upload_handler: MethodRouter<SharedMockServerState>,
    telemetry_upload_metrics: MethodRouter<SharedMockServerState>,
    routes: Vec<(&'static str, MethodRouter<SharedMockServerState>)>,
}

impl MockServerBuilder {
    pub fn new() -> Self {
        Self {
            create_test_upload_handler: post(create_test_upload_handler),
            quarantine_status_handler: post(quarantine_status_handler),
            all_quarantined_tests_handler: post(all_quarantined_tests_handler),
            s3_upload_handler: put(s3_upload_handler),
            telemetry_upload_metrics: post(telemetry_upload_metrics_handler),
            routes: Vec::new(),
        }
    }

    /// Serves `path` with `method_router`, in place of the builder's default for it if it has one.
    pub fn route(
        &mut self,
        path: &'static str,
        method_router: MethodRouter<SharedMockServerState>,
    ) {
        self.routes.push((path, method_router));
    }

    pub fn set_create_test_upload_handler<H, T>(&mut self, handler: H)
    where
        H: Handler<T, SharedMockServerState>,
        T: 'static,
    {
        self.create_test_upload_handler = post(handler);
    }

    pub fn set_quarantine_status_handler<H, T>(&mut self, handler: H)
    where
        H: Handler<T, SharedMockServerState>,
        T: 'static,
    {
        self.quarantine_status_handler = post(handler);
    }

    pub fn set_all_quarantined_tests_handler<H, T>(&mut self, handler: H)
    where
        H: Handler<T, SharedMockServerState>,
        T: 'static,
    {
        self.all_quarantined_tests_handler = post(handler);
    }

    pub fn set_s3_upload_handler<H, T>(&mut self, handler: H)
    where
        H: Handler<T, SharedMockServerState>,
        T: 'static,
    {
        self.s3_upload_handler = put(handler);
    }

    pub fn set_telemetry_upload_metrics_handler<H, T>(&mut self, handler: H)
    where
        H: Handler<T, SharedMockServerState>,
        T: 'static,
    {
        self.telemetry_upload_metrics = post(handler);
    }

    /// Mock server spawned in a new thread.
    pub async fn spawn_mock_server(self) -> SharedMockServerState {
        let listener = TcpListener::bind("localhost:0").await.unwrap();
        let random_port = listener.local_addr().unwrap().port();
        let host = format!("http://localhost:{random_port}");

        let state = Arc::new(MockServerState {
            host,
            ..Default::default()
        });

        let defaults = [
            (UPLOADS_ROUTE, self.create_test_upload_handler),
            (QUARANTINE_STATUS_ROUTE, self.quarantine_status_handler),
            (
                ALL_QUARANTINED_TESTS_ROUTE,
                self.all_quarantined_tests_handler,
            ),
            ("/s3upload", self.s3_upload_handler),
            (api::otlp::CLI_TELEMETRY_PATH, self.telemetry_upload_metrics),
        ];
        let mut app = Router::new();
        // A route the test serves itself replaces the default for that path.
        for (path, method_router) in defaults {
            if !self.routes.iter().any(|(route, _)| *route == path) {
                app = app.route(path, method_router);
            }
        }
        for (path, method_router) in self.routes {
            app = app.route(path, method_router);
        }

        app = app.route(
            "/*rest",
            any(|| async {
                let mut res = Response::new(String::from(
                    r#"{ "status_code": 404, "error": "not found" }"#,
                ));
                *res.status_mut() = StatusCode::NOT_FOUND;
                res
            }),
        );

        let spawn_state = state.clone();
        spawn(async move {
            axum::serve(listener, app.with_state(spawn_state))
                .await
                .unwrap();
        });

        state
    }
}

impl Default for MockServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedMockServerState = Arc<MockServerState>;

impl MockServerState {
    fn record(&self, endpoint: &str, headers: HeaderMap, payload: RequestPayload) {
        self.request_headers
            .lock()
            .unwrap()
            .insert(String::from(endpoint), headers);
        self.requests.lock().unwrap().push(payload);
    }

    /// Records a create-upload request the way the default handler does, for a test's own handler.
    pub fn record_create_test_upload(
        &self,
        headers: HeaderMap,
        test_collection_short_id: String,
        mut request: CreateTestUpload,
    ) {
        request.test_collection_short_id = test_collection_short_id;
        self.record(
            "uploads",
            headers,
            RequestPayload::CreateTestUpload(request),
        );
    }

    /// Records a quarantine-status request the way the default handler does, for a test's own
    /// handler.
    pub fn record_quarantine_status(
        &self,
        headers: HeaderMap,
        test_collection_short_id: String,
        mut lookup: QuarantineStatusLookup,
    ) -> QuarantineStatusLookup {
        lookup.test_collection_short_id = test_collection_short_id;
        self.record(
            "quarantineStatus",
            headers,
            RequestPayload::QuarantineStatus(lookup.clone()),
        );
        lookup
    }

    /// Records an all-quarantined-tests request the way the default handler does, for a test's
    /// own handler.
    pub fn record_all_quarantined_tests(
        &self,
        headers: HeaderMap,
        test_collection_short_id: String,
        query: HashMap<String, String>,
        mut lookup: AllQuarantinedTestsLookup,
    ) {
        lookup.test_collection_short_id = test_collection_short_id;
        lookup.cursor = query.get("cursor").cloned();
        self.record(
            "allQuarantinedTests",
            headers,
            RequestPayload::AllQuarantinedTests(lookup),
        );
    }
}

#[axum::debug_handler]
pub async fn create_test_upload_handler(
    State(state): State<SharedMockServerState>,
    Path(test_collection_short_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<CreateTestUpload>,
) -> (StatusCode, Json<CreatedTestUpload>) {
    state.record_create_test_upload(headers, test_collection_short_id, request);
    (StatusCode::CREATED, Json(created_test_upload(&state.host)))
}

/// Answers that quarantining is on and no looked-up test is quarantined.
#[axum::debug_handler]
pub async fn quarantine_status_handler(
    State(state): State<SharedMockServerState>,
    Path(test_collection_short_id): Path<String>,
    headers: HeaderMap,
    Json(lookup): Json<QuarantineStatusLookup>,
) -> Json<QuarantineStatus> {
    let lookup = state.record_quarantine_status(headers, test_collection_short_id, lookup);
    Json(quarantine_status(&lookup, true, Vec::<String>::new()))
}

/// Answers that quarantining is on and every looked-up test is quarantined.
#[axum::debug_handler]
pub async fn quarantine_all_handler(
    State(state): State<SharedMockServerState>,
    Path(test_collection_short_id): Path<String>,
    headers: HeaderMap,
    Json(lookup): Json<QuarantineStatusLookup>,
) -> Json<QuarantineStatus> {
    let lookup = state.record_quarantine_status(headers, test_collection_short_id, lookup);
    let test_ids = lookup.test_ids();
    Json(quarantine_status(&lookup, true, test_ids))
}

/// Answers that quarantining is off.
#[axum::debug_handler]
pub async fn quarantining_disabled_handler(
    State(state): State<SharedMockServerState>,
    Path(test_collection_short_id): Path<String>,
    headers: HeaderMap,
    Json(lookup): Json<QuarantineStatusLookup>,
) -> Json<QuarantineStatus> {
    let lookup = state.record_quarantine_status(headers, test_collection_short_id, lookup);
    Json(quarantine_status(&lookup, false, Vec::<String>::new()))
}

/// Answers that quarantining is on and no test is quarantined.
#[axum::debug_handler]
pub async fn all_quarantined_tests_handler(
    State(state): State<SharedMockServerState>,
    Path(test_collection_short_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    Json(lookup): Json<AllQuarantinedTestsLookup>,
) -> Json<QuarantinedTestList> {
    state.record_all_quarantined_tests(headers, test_collection_short_id, query, lookup);
    Json(quarantined_test_list(true, Vec::<String>::new()))
}

#[axum::debug_handler]
pub async fn telemetry_upload_metrics_handler(
    State(state): State<SharedMockServerState>,
    bytes: Bytes,
) -> Response<String> {
    let upload_metrics = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|logs| upload_metrics_from_otlp(&logs));
    if let Some(upload_metrics) = upload_metrics {
        state
            .requests
            .lock()
            .unwrap()
            .push(RequestPayload::TelemetryUploadMetrics(upload_metrics));
        Response::new(String::from("OK"))
    } else {
        tracing::error!("Failed to decode upload metrics");
        Response::new(String::from("Err"))
    }
}

#[axum::debug_handler]
pub async fn s3_upload_handler(
    State(state): State<SharedMockServerState>,
    bytes: Bytes,
) -> Response<String> {
    let uncompressed_bytes = zstd::decode_all(bytes.as_ref()).unwrap();
    let mut archive = tar::Archive::new(uncompressed_bytes.as_slice());
    let tar_extract_directory = tempdir().unwrap();
    for file_entry in archive.entries().unwrap() {
        let mut file_entry = file_entry.unwrap();
        let mut file_entry_bytes = Vec::new();
        file_entry.read_to_end(&mut file_entry_bytes).unwrap();
        let file_entry_path = file_entry.header().path().unwrap();
        let file_path = tar_extract_directory.as_ref().join(file_entry_path);
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(file_path, file_entry_bytes).unwrap();
    }
    state
        .requests
        .lock()
        .unwrap()
        .push(RequestPayload::S3Upload(tar_extract_directory.into_path()));
    Response::new(String::from("OK"))
}

/// The fields of `UploadMetrics` that `api::otlp::upload_metrics_logs` writes, read back from the
/// OTLP JSON it sent, so the tests check what reaches the wire. `None` unless the body is one
/// `trunk-analytics-cli` record.
pub fn upload_metrics_from_otlp(logs: &serde_json::Value) -> Option<UploadMetrics> {
    use api::otlp;
    use proto::upload_metrics::trunk::{QuarantineQueryResult, QuarantineResolutionMode, Repo};

    let resource_logs = &logs["resourceLogs"][0];
    let attribute = |list: &serde_json::Value, key: &str| -> Option<serde_json::Value> {
        list.as_array()?
            .iter()
            .find(|attribute| attribute["key"] == key)
            .map(|attribute| attribute["value"].clone())
    };
    if attribute(&resource_logs["resource"]["attributes"], "service.name")?["stringValue"]
        != otlp::SERVICE_NAME
    {
        return None;
    }
    let record = &resource_logs["scopeLogs"][0]["logRecords"][0]["attributes"];
    let text = |key: &str| {
        attribute(record, key)
            .and_then(|value| value["stringValue"].as_str().map(str::to_owned))
            .unwrap_or_default()
    };
    let upper = |key: &str| text(key).to_uppercase();
    Some(UploadMetrics {
        failed: attribute(record, otlp::FAILED_ATTRIBUTE)?["boolValue"].as_bool()?,
        failure_reason: text(otlp::FAILURE_REASON_ATTRIBUTE),
        quarantine_query_result: QuarantineQueryResult::from_str_name(&format!(
            "QUARANTINE_QUERY_RESULT_{}",
            upper(otlp::QUARANTINE_QUERY_RESULT_ATTRIBUTE)
        ))?
        .into(),
        quarantine_resolution_mode: QuarantineResolutionMode::from_str_name(&format!(
            "QUARANTINE_RESOLUTION_MODE_{}",
            upper(otlp::QUARANTINE_RESOLUTION_MODE_ATTRIBUTE)
        ))?
        .into(),
        repo: Some(Repo {
            host: text(otlp::REPOSITORY_HOST_ATTRIBUTE),
            owner: text(otlp::REPOSITORY_OWNER_ATTRIBUTE),
            name: text(otlp::REPOSITORY_NAME_ATTRIBUTE),
        }),
        ..UploadMetrics::default()
    })
}
