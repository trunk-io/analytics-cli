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

/// Which source the server resolved quarantine status from.
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum QuarantineResolutionMode {
    Repo,
    TestCollection,
    #[default]
    Unspecified,
}

impl<'de> Deserialize<'de> for QuarantineResolutionMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match serde_json::Value::deserialize(deserializer)?.as_str() {
            Some("repo") => Self::Repo,
            Some("test_collection") => Self::TestCollection,
            _ => Self::Unspecified,
        })
    }
}

impl From<QuarantineResolutionMode> for proto::upload_metrics::trunk::QuarantineResolutionMode {
    fn from(mode: QuarantineResolutionMode) -> Self {
        match mode {
            QuarantineResolutionMode::Repo => Self::Repo,
            QuarantineResolutionMode::TestCollection => Self::TestCollection,
            QuarantineResolutionMode::Unspecified => Self::Unspecified,
        }
    }
}

impl QuarantineResolutionMode {
    pub fn resolution_log_line(
        &self,
        test_collection_id: Option<&str>,
        repo: &RepoUrlParts,
    ) -> Option<String> {
        match self {
            Self::TestCollection => Some(format!(
                "Resolved quarantine status for test collection {}",
                test_collection_id.unwrap_or("unknown"),
            )),
            Self::Repo => Some(format!(
                "Resolved quarantine status for repo {}",
                repo.repo_full_name()
            )),
            Self::Unspecified => None,
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
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetQuarantineConfigRequest {
    pub repo: RepoUrlParts,
    pub remote_urls: Vec<String>,
    pub org_url_slug: String,
    pub test_identifiers: Vec<Test>,
    /// Optional test collection short id (from `--test-collection-id`). When set and the org is
    /// migrated to test collections, the server resolves quarantine status from the collection.
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
