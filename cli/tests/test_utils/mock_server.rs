use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{any, post, put};
use axum::{Json, Router};
use tempfile::tempdir;
use tokio::net::TcpListener;
use tokio::spawn;
use trunk_analytics_cli::types::{
    CreateBundleUploadRequest, CreateBundleUploadResponse, CreateRepoRequest,
    GetQuarantineBulkTestStatusRequest, QuarantineConfig,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestPayload {
    CreateRepo(CreateRepoRequest),
    CreateBundleUpload(CreateBundleUploadRequest),
    GetQuarantineBulkTestStatus(GetQuarantineBulkTestStatusRequest),
    S3Upload(PathBuf),
}

#[derive(Debug, Default)]
pub struct MockServerState {
    pub requests: Mutex<Vec<RequestPayload>>,
    pub host: String,
}

pub type SharedMockServerState = Arc<MockServerState>;

/// Mock server spawned in a new thread.
///
/// NOTE: must use a multithreaded executor to have the server run while running tests
#[allow(dead_code)] // TODO: move this to its own crate to get rid of the need for this
pub async fn spawn_mock_server() -> SharedMockServerState {
    let listener = TcpListener::bind("localhost:0").await.unwrap();
    let random_port = listener.local_addr().unwrap().port();
    let host = format!("http://localhost:{random_port}");

    let state = Arc::new(MockServerState {
        host,
        ..Default::default()
    });

    let mut app = Router::new()
        .route("/v1/repo/create", post(repo_create_handler))
        .route(
            "/v1/metrics/createBundleUpload",
            post(create_bundle_handler),
        )
        .route(
            "/v1/metrics/getQuarantineConfig",
            post(get_quarantining_config_handler),
        )
        .route("/s3upload", put(s3_upload_handler));

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

#[allow(dead_code)] // TODO: move this to its own crate to get rid of the need for this
#[axum::debug_handler]
async fn repo_create_handler(
    State(state): State<SharedMockServerState>,
    Json(create_repo_request): Json<CreateRepoRequest>,
) -> Response<String> {
    state
        .requests
        .lock()
        .unwrap()
        .push(RequestPayload::CreateRepo(create_repo_request));
    Response::new(String::from("OK"))
}

#[allow(dead_code)] // TODO: move this to its own crate to get rid of the need for this
#[axum::debug_handler]
async fn create_bundle_handler(
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
        url: format!("{host}/s3upload"),
        key: String::from("unused"),
    })
}

#[allow(dead_code)] // TODO: move this to its own crate to get rid of the need for this
#[axum::debug_handler]
async fn get_quarantining_config_handler(
    State(state): State<SharedMockServerState>,
    Json(get_quarantine_bulk_test_status_request): Json<GetQuarantineBulkTestStatusRequest>,
) -> Json<QuarantineConfig> {
    state
        .requests
        .lock()
        .unwrap()
        .push(RequestPayload::GetQuarantineBulkTestStatus(
            get_quarantine_bulk_test_status_request,
        ));
    Json(QuarantineConfig {
        is_preview_mode: true,
        quarantined_tests: HashSet::new(),
    })
}

#[allow(dead_code)] // TODO: move this to its own crate to get rid of the need for this
#[axum::debug_handler]
async fn s3_upload_handler(
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
