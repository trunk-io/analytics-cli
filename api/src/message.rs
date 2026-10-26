use std::collections::HashSet;

pub use bundle::QuarantineResolutionMode;
use bundle::Test;
use context::repo::RepoUrlParts;
use serde::{Deserialize, Serialize};

use crate::urls::TestCaseGuidScope;

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
    #[serde(default)]
    pub repo_id: Option<String>,
    #[serde(default)]
    pub test_collection_id: Option<String>,
}

#[derive(Debug, Serialize, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GetQuarantineConfigResponse {
    pub is_disabled: bool,
    #[serde(rename = "testIds")]
    pub quarantined_tests: Vec<String>,
    #[serde(default)]
    pub quarantine_resolution_mode: QuarantineResolutionMode,
    /// Set on v2, which returns the collection's UUID and the repository id.
    #[serde(skip)]
    pub test_case_guid_scope: Option<TestCaseGuidScope>,
    /// v2's whole quarantined set, by test-case GUID. Set only for a request with no
    /// identifiers, where `quarantined_tests` stays empty.
    #[serde(skip)]
    pub quarantined_test_case_guids: Option<HashSet<String>>,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetQuarantineConfigRequest {
    pub repo: RepoUrlParts,
    pub remote_urls: Vec<String>,
    pub org_url_slug: String,
    pub test_identifiers: Vec<Test>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_collection_short_id: Option<String>,
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

/// A 4xx body the API sends for an error it explains itself; the CLI prints `message` as given.
#[derive(Debug, Deserialize)]
pub struct ClientErrorBody {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UploadApiVersion {
    #[default]
    V1,
    V2,
}

impl UploadApiVersion {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::V1 => "v1",
            Self::V2 => "v2",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum UploadApiCaller {
    Cli,
    TestRunnerPlugin,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetUploadApiVersionRequest {
    pub cli_version: String,
    pub org_slug: String,
    pub test_collection_short_id: Option<String>,
    pub repo: Option<RepoUrlParts>,
    pub ci_provider: Option<String>,
    pub is_fork_pr: bool,
    pub caller: UploadApiCaller,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUploadApiVersionResponse {
    pub api_version: String,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateTestUploadRequest {
    pub repo: RepoUrlParts,
    pub remote_urls: Vec<String>,
    pub client_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreatedTestUpload {
    pub id: String,
    pub upload_url: String,
    pub repository_id: String,
    pub created_at: String,
}

/// The opaque `bundle_upload_id_v2` ingestion decodes from the bundle: base64 of this JSON.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleUploadIdV2<'a> {
    pub id: &'a str,
    pub created_at: &'a str,
    pub repo_id: &'a str,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineStatusRequest {
    pub repo: RepoUrlParts,
    pub test_identifiers: Vec<QuarantineStatusIdentifier>,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineStatusIdentifier {
    pub id: String,
    pub name: String,
    pub parent_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
}

impl From<&Test> for QuarantineStatusIdentifier {
    fn from(test: &Test) -> Self {
        Self {
            id: test.id.clone(),
            name: test.name.clone(),
            parent_name: test.parent_name.clone(),
            class_name: test.class_name.clone().filter(|name| !name.is_empty()),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineStatus {
    pub test_collection_uuid: String,
    pub repository_id: String,
    pub quarantine_enabled: bool,
    pub tests: Vec<QuarantineStatusTest>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineStatusTest {
    pub client_test_id: Option<String>,
    pub quarantined: bool,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QuarantinedTestList {
    pub data: Vec<QuarantinedTest>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QuarantinedTest {
    pub test_id: String,
}
