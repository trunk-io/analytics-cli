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
    #[serde(rename = "remoteUrls")]
    pub remote_urls: Vec<String>,
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

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct PageQuery {
    #[serde(rename = "pageSize")]
    pub page_size: i32,
    #[serde(rename = "pageToken")]
    pub page_token: String,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct Page {
    pub total_rows: i32,
    pub total_pages: i32,
    pub next_page_token: String,
    pub prev_page_token: String,
    pub last_page_token: String,
    pub page_index: i32,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct ListQuarantinedTestsRequest {
    pub repo: RepoUrlParts,
    #[serde(rename = "orgUrlSlug")]
    pub org_url_slug: String,
    #[serde(rename = "pageQuery")]
    pub page_query: PageQuery,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub enum QuarantineSetting {
    #[serde(rename = "ALWAYS_QUARANTINE")]
    AlwaysQuarantine,
    #[serde(rename = "AUTO_QUARANTINE")]
    AutoQuarantine,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct QuarantinedTest {
    pub name: String,
    pub parent: Option<String>,
    pub file: Option<String>,
    #[serde(rename = "className")]
    pub class_name: Option<String>,
    pub status: String,
    pub quarantine_setting: QuarantineSetting,
    pub test_case_id: String,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct ListQuarantinedTestsResponse {
    pub quarantined_tests: Vec<QuarantinedTest>,
    pub page: Page,
}
