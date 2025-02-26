use bundle::Test;
use context::repo::RepoUrlParts;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
pub struct CreateBundleUploadRequest {
    pub repo: RepoUrlParts,
    #[serde(rename = "orgUrlSlug")]
    pub org_url_slug: String,
    #[serde(rename = "clientVersion")]
    pub client_version: String,
    #[serde(rename = "remoteUrls")]
    pub remote_urls: Vec<String>,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct CreateBundleUploadResponse {
    pub id: String,
    #[serde(rename = "idV2")]
    pub id_v2: String,
    pub url: String,
    pub key: String,
}

#[derive(Debug, Serialize, Clone, Deserialize, Default)]
pub struct GetQuarantineConfigResponse {
    #[serde(rename = "isDisabled")]
    pub is_disabled: bool,
    #[serde(rename = "testIds")]
    pub quarantined_tests: Vec<String>,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
pub struct GetQuarantineConfigRequest {
    pub repo: RepoUrlParts,
    #[serde(rename = "orgUrlSlug")]
    pub org_url_slug: String,
    #[serde(rename = "testIdentifiers")]
    pub test_identifiers: Vec<Test>,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
pub struct CreateBundleUploadIntentRequest {
    pub repo: RepoUrlParts,
    #[serde(rename = "orgUrlSlug")]
    pub org_url_slug: String,
    #[serde(rename = "clientVersion")]
    pub client_version: String,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
pub struct CreateBundleUploadIntentResponse {
    pub repo: RepoUrlParts,
    #[serde(rename = "orgUrlSlug")]
    pub org_url_slug: String,
    #[serde(rename = "clientVersion")]
    pub client_version: String,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct TelemetryUploadMetricsRequest {
    pub upload_metrics: proto::upload_metrics::trunk::UploadMetrics,
}
