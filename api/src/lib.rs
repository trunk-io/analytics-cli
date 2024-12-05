use std::collections::HashSet;

use context::repo::RepoUrlParts;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
pub struct CreateBundleUploadRequest {
    pub repo: RepoUrlParts,
    #[serde(rename = "orgUrlSlug")]
    pub org_url_slug: String,
    #[serde(rename = "clientVersion")]
    pub client_version: String,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct CreateBundleUploadResponse {
    pub id: String,
    #[serde(rename = "idV2")]
    pub id_v2: Option<String>,
    pub url: String,
    pub key: String,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
pub enum BundleUploadStatus {
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "UPLOAD_COMPLETE")]
    UploadComplete,
    #[serde(rename = "UPLOAD_FAILED")]
    UploadFailed,
    #[serde(rename = "DRY_RUN")]
    DryRun,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
pub struct UpdateBundleUploadRequest {
    pub id: String,
    #[serde(rename = "uploadStatus")]
    pub upload_status: BundleUploadStatus,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
pub struct CreateRepoRequest {
    pub repo: RepoUrlParts,
    #[serde(rename = "orgUrlSlug")]
    pub org_url_slug: String,
    #[serde(rename = "remoteUrls")]
    pub remote_urls: Vec<String>,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
pub struct GetQuarantineBulkTestStatusRequest {
    pub repo: RepoUrlParts,
    #[serde(rename = "orgUrlSlug")]
    pub org_url_slug: String,
}

#[derive(Debug, Serialize, Clone, Deserialize, Default)]
pub struct QuarantineConfig {
    #[serde(rename = "isPreview")]
    pub is_preview_mode: bool,
    #[serde(rename = "isDisabled")]
    pub is_disabled: bool,
    #[serde(rename = "testIds")]
    pub quarantined_tests: HashSet<String>,
}
