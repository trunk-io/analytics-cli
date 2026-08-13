pub use bundle::QuarantineResolutionMode;
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
    /// Repo UUID used to key test collection URLs; absent on older servers.
    pub repo_id: Option<String>,
    /// Same server-side calculation as `quarantine_resolution_mode`; picks the test URL format.
    #[serde(default)]
    pub test_collection_migration_state: TestCollectionMigrationState,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TestCollectionMigrationState {
    Repo,
    TestCollection,
    #[default]
    Unspecified,
}

impl<'de> Deserialize<'de> for TestCollectionMigrationState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(
            match serde_json::Value::deserialize(deserializer)?.as_str() {
                Some("repo") => Self::Repo,
                Some("test_collection") => Self::TestCollection,
                _ => Self::Unspecified,
            },
        )
    }
}

impl From<TestCollectionMigrationState>
    for proto::upload_metrics::trunk::TestCollectionMigrationState
{
    fn from(state: TestCollectionMigrationState) -> Self {
        match state {
            TestCollectionMigrationState::Repo => Self::Repo,
            TestCollectionMigrationState::TestCollection => Self::TestCollection,
            TestCollectionMigrationState::Unspecified => Self::Unspecified,
        }
    }
}

#[derive(Debug, Serialize, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GetQuarantineConfigResponse {
    pub is_disabled: bool,
    #[serde(rename = "testIds")]
    pub quarantined_tests: Vec<String>,
    #[serde(default)]
    pub quarantine_resolution_mode: QuarantineResolutionMode,
    /// Repo UUID used to key test collection URLs; absent on older servers.
    pub repo_id: Option<String>,
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

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct TelemetryUploadMetricsRequest {
    pub upload_metrics: proto::upload_metrics::trunk::UploadMetrics,
}
