use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use api::message::{
    CreateBundleUploadRequest, CreateBundleUploadResponse, GetQuarantineConfigRequest,
    GetQuarantineConfigResponse,
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::State,
    handler::Handler,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{MethodRouter, any, post, put},
};
use tempfile::tempdir;
use tokio::{net::TcpListener, spawn};

pub const MOCK_REPO_ID: &str = "7a1f0e3d-2b4c-4d5e-8f90-123456789abc";
pub const MOCK_TEST_COLLECTION_ID: &str = "018f6d3a-6f2e-4c4a-9b1e-2f3a4b5c6d7e";

#[derive(Debug, Clone, PartialEq)]
pub enum RequestPayload {
    CreateBundleUpload(CreateBundleUploadRequest),
    GetQuarantineConfig(GetQuarantineConfigRequest),
    S3Upload(PathBuf),
    TelemetryUploadMetrics(TelemetryUploadMetrics),
}

/// What one OTLP metrics request from the CLI says, read back from the wire.
#[derive(Debug, Clone, PartialEq)]
pub struct TelemetryUploadMetrics {
    pub status: String,
    pub failure_reason: String,
    pub quarantine_query_result: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub upload_duration: Option<DurationObservation>,
    pub run_duration: Option<DurationObservation>,
}

/// One duration histogram's single observation.
#[derive(Debug, Clone, PartialEq)]
pub struct DurationObservation {
    pub status: String,
    pub seconds: f64,
    /// The index of the one bucket counting it.
    pub bucket: usize,
    pub explicit_bounds: Vec<f64>,
}

impl DurationObservation {
    /// Whether `bucket` is the OTLP bucket for `seconds`: bucket `i` counts values in
    /// `(bounds[i - 1], bounds[i]]`.
    pub fn is_in_its_bucket(&self) -> bool {
        let lower = self
            .bucket
            .checked_sub(1)
            .map_or(f64::NEG_INFINITY, |index| self.explicit_bounds[index]);
        let upper = self
            .explicit_bounds
            .get(self.bucket)
            .copied()
            .unwrap_or(f64::INFINITY);
        lower < self.seconds && self.seconds <= upper
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
    /// "createBundleUpload"), so tests can assert on auth headers.
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
    create_bundle_handler: MethodRouter<SharedMockServerState>,
    get_quarantining_config_handler: MethodRouter<SharedMockServerState>,
    s3_upload_handler: MethodRouter<SharedMockServerState>,
    telemetry_upload_metrics: MethodRouter<SharedMockServerState>,
    routes: Vec<(&'static str, MethodRouter<SharedMockServerState>)>,
}

impl MockServerBuilder {
    pub fn new() -> Self {
        Self {
            create_bundle_handler: post(create_bundle_handler),
            get_quarantining_config_handler: post(get_quarantining_config_handler),
            s3_upload_handler: put(s3_upload_handler),
            telemetry_upload_metrics: post(telemetry_upload_metrics_handler),
            routes: Vec::new(),
        }
    }

    /// Serves `path` as well, such as a v2 route, which the builder has no default for.
    pub fn route(
        &mut self,
        path: &'static str,
        method_router: MethodRouter<SharedMockServerState>,
    ) {
        self.routes.push((path, method_router));
    }

    pub fn set_create_bundle_handler<H, T>(&mut self, handler: H)
    where
        H: Handler<T, SharedMockServerState>,
        T: 'static,
    {
        self.create_bundle_handler = post(handler);
    }

    pub fn set_get_quarantining_config_handler<H, T>(&mut self, handler: H)
    where
        H: Handler<T, SharedMockServerState>,
        T: 'static,
    {
        self.get_quarantining_config_handler = post(handler);
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

        let mut app = Router::new()
            .route("/v1/metrics/createBundleUpload", self.create_bundle_handler)
            .route(
                "/v1/metrics/getQuarantineConfig",
                self.get_quarantining_config_handler,
            )
            .route("/s3upload", self.s3_upload_handler)
            .route(
                api::otlp::CLI_TELEMETRY_METRICS_PATH,
                self.telemetry_upload_metrics,
            );

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

#[axum::debug_handler]
pub async fn create_bundle_handler(
    State(state): State<SharedMockServerState>,
    headers: HeaderMap,
    Json(create_bundle_upload_request): Json<CreateBundleUploadRequest>,
) -> Json<CreateBundleUploadResponse> {
    state
        .request_headers
        .lock()
        .unwrap()
        .insert(String::from("createBundleUpload"), headers);
    state
        .requests
        .lock()
        .unwrap()
        .push(RequestPayload::CreateBundleUpload(
            create_bundle_upload_request,
        ));
    let host = &state.host;
    Json(CreateBundleUploadResponse {
        id: String::from("test-bundle-upload-id"),
        id_v2: String::from("test-bundle-upload-id-v2"),
        url: format!("{host}/s3upload"),
        key: String::from("unused"),
        test_collection_bundle_meta_id: Some(String::from("82c6a6e5-f8ea-4d93-9a26-b8ab6ff8f6bc")),
        test_collection_bundle_meta_created_at: Some(String::from("2026-05-10T12:34:56.000Z")),
        repo_id: Some(String::from(MOCK_REPO_ID)),
        test_collection_id: Some(String::from(MOCK_TEST_COLLECTION_ID)),
    })
}

#[axum::debug_handler]
pub async fn get_quarantining_config_handler(
    State(state): State<SharedMockServerState>,
    headers: HeaderMap,
    Json(get_quarantine_config_request): Json<GetQuarantineConfigRequest>,
) -> Json<GetQuarantineConfigResponse> {
    state
        .request_headers
        .lock()
        .unwrap()
        .insert(String::from("getQuarantineConfig"), headers);
    state
        .requests
        .lock()
        .unwrap()
        .push(RequestPayload::GetQuarantineConfig(
            get_quarantine_config_request,
        ));
    Json(GetQuarantineConfigResponse {
        is_disabled: false,
        quarantined_tests: Vec::new(),
        ..Default::default()
    })
}

#[axum::debug_handler]
pub async fn telemetry_upload_metrics_handler(
    State(state): State<SharedMockServerState>,
    bytes: Bytes,
) -> Response<String> {
    let upload_metrics = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|request| telemetry_upload_metrics_from_otlp(&request));
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

/// The CLI's OTLP metrics request read back, so the tests check what reaches the wire. `None`
/// unless the body is one `trunk-analytics-cli` resource whose metrics have the names, types,
/// units and temporality `api::otlp` promises, each with one data point.
pub fn telemetry_upload_metrics_from_otlp(
    request: &serde_json::Value,
) -> Option<TelemetryUploadMetrics> {
    use api::otlp;
    use serde_json::Value;

    let attribute = |list: &Value, key: &str| -> Option<String> {
        list.as_array()?
            .iter()
            .find(|attribute| attribute["key"] == key)?["value"]["stringValue"]
            .as_str()
            .map(str::to_owned)
    };
    let resource_metrics = request["resourceMetrics"].as_array()?;
    let [resource_metrics] = resource_metrics.as_slice() else {
        return None;
    };
    if attribute(&resource_metrics["resource"]["attributes"], "service.name")? != otlp::SERVICE_NAME
    {
        return None;
    }
    let metrics = resource_metrics["scopeMetrics"][0]["metrics"].as_array()?;
    let metric = |name: &str| metrics.iter().find(|metric| metric["name"] == name);
    let single_point = |data: &Value| -> Option<Value> {
        if data["aggregationTemporality"] != 1 {
            return None;
        }
        match data["dataPoints"].as_array()?.as_slice() {
            [point] => Some(point.clone()),
            _ => None,
        }
    };

    let uploads = metric(otlp::UPLOADS_METRIC)?;
    if uploads["unit"] != otlp::UPLOADS_UNIT || uploads["sum"]["isMonotonic"] != true {
        return None;
    }
    let point = single_point(&uploads["sum"])?;
    if point["asInt"] != "1" {
        return None;
    }
    let labels = &point["attributes"];

    let duration = |name: &str| -> Option<Option<DurationObservation>> {
        let Some(histogram) = metric(name) else {
            return Some(None);
        };
        if histogram["unit"] != otlp::DURATION_UNIT {
            return None;
        }
        let point = single_point(&histogram["histogram"])?;
        let seconds = point["sum"].as_f64()?;
        if point["count"] != "1"
            || point["min"].as_f64()? != seconds
            || point["max"].as_f64()? != seconds
        {
            return None;
        }
        let explicit_bounds = point["explicitBounds"]
            .as_array()?
            .iter()
            .map(Value::as_f64)
            .collect::<Option<Vec<_>>>()?;
        let counts = point["bucketCounts"].as_array()?;
        if counts.len() != explicit_bounds.len() + 1
            || counts.iter().filter(|count| *count == "1").count() != 1
            || counts.iter().any(|count| *count != "0" && *count != "1")
        {
            return None;
        }
        Some(Some(DurationObservation {
            status: attribute(&point["attributes"], otlp::STATUS_ATTRIBUTE)?,
            seconds,
            bucket: counts.iter().position(|count| count == "1")?,
            explicit_bounds,
        }))
    };

    Some(TelemetryUploadMetrics {
        status: attribute(labels, otlp::STATUS_ATTRIBUTE)?,
        failure_reason: attribute(labels, otlp::FAILURE_REASON_ATTRIBUTE)?,
        quarantine_query_result: attribute(labels, otlp::QUARANTINE_QUERY_RESULT_ATTRIBUTE)?,
        repo_owner: attribute(labels, otlp::REPOSITORY_OWNER_ATTRIBUTE)?,
        repo_name: attribute(labels, otlp::REPOSITORY_NAME_ATTRIBUTE)?,
        upload_duration: duration(otlp::UPLOAD_DURATION_METRIC)?,
        run_duration: duration(otlp::RUN_DURATION_METRIC)?,
    })
}
