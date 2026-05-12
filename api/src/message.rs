use bundle::Test;
use context::repo::RepoUrlParts;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateBundleUploadRequest {
    pub repo: RepoUrlParts,
    pub org_url_slug: String,
    pub client_version: String,
    pub remote_urls: Vec<String>,
    pub external_id: Option<String>,
    pub test_collection_short_id: Option<String>,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBundleUploadResponse {
    pub id: String,
    pub id_v2: String,
    pub url: String,
    pub key: String,
    pub test_collection_bundle_meta_id: Option<String>,
    pub test_collection_bundle_meta_created_at: Option<String>,
}

#[derive(Debug, Serialize, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GetQuarantineConfigResponse {
    pub is_disabled: bool,
    #[serde(rename = "testIds")]
    pub quarantined_tests: Vec<String>,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetQuarantineConfigRequest {
    pub repo: RepoUrlParts,
    pub remote_urls: Vec<String>,
    pub org_url_slug: String,
    pub test_identifiers: Vec<Test>,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateBundleUploadIntentRequest {
    pub repo: RepoUrlParts,
    pub org_url_slug: String,
    pub client_version: String,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateBundleUploadIntentResponse {
    pub repo: RepoUrlParts,
    pub org_url_slug: String,
    pub client_version: String,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct TelemetryUploadMetricsRequest {
    pub upload_metrics: proto::upload_metrics::trunk::UploadMetrics,
}
