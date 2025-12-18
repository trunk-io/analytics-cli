use std::{
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
    http::StatusCode,
    response::Response,
    routing::{MethodRouter, any, post, put},
};
use prost::Message;
use proto::upload_metrics::trunk::UploadMetrics;
use tempfile::tempdir;
use tokio::{net::TcpListener, spawn};

#[derive(Debug, Clone, PartialEq)]
pub enum RequestPayload {
    CreateBundleUpload(CreateBundleUploadRequest),
    GetQuarantineConfig(GetQuarantineConfigRequest),
    S3Upload(PathBuf),
    TelemetryUploadMetrics(UploadMetrics),
}

#[derive(Debug, Default)]
pub struct MockServerState {
    pub requests: Mutex<Vec<RequestPayload>>,
    pub host: String,
}

#[derive(Debug, Clone)]
pub struct MockServerBuilder {
    create_bundle_handler: MethodRouter<SharedMockServerState>,
    get_quarantining_config_handler: MethodRouter<SharedMockServerState>,
    s3_upload_handler: MethodRouter<SharedMockServerState>,
    telemetry_upload_metrics: MethodRouter<SharedMockServerState>,
}

impl MockServerBuilder {
    pub fn new() -> Self {
        Self {
            create_bundle_handler: post(create_bundle_handler),
            get_quarantining_config_handler: post(get_quarantining_config_handler),
            s3_upload_handler: put(s3_upload_handler),
            telemetry_upload_metrics: post(telemetry_upload_metrics_handler),
        }
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
                "/v1/flakytests-cli/upload-metrics",
                self.telemetry_upload_metrics,
            );

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
    Json(create_bundle_upload_request): Json<CreateBundleUploadRequest>,
) -> Json<CreateBundleUploadResponse> {
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
    })
}

#[axum::debug_handler]
pub async fn get_quarantining_config_handler(
    State(state): State<SharedMockServerState>,
    Json(get_quarantine_config_request): Json<GetQuarantineConfigRequest>,
) -> Json<GetQuarantineConfigResponse> {
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
    })
}

#[axum::debug_handler]
pub async fn telemetry_upload_metrics_handler(
    State(state): State<SharedMockServerState>,
    bytes: Bytes,
) -> Response<String> {
    let upload_metrics = proto::upload_metrics::trunk::UploadMetrics::decode(bytes);
    if let Ok(upload_metrics) = upload_metrics {
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
