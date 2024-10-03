use std::collections::HashSet;

use context::repo::RepoUrlParts;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Eq)]
pub struct CreateBundleUploadRequest {
    pub repo: RepoUrlParts,
    #[serde(rename = "orgUrlSlug")]
    pub org_url_slug: String,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct CreateBundleUploadResponse {
    pub id: String,
    pub url: String,
    pub key: String,
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
    #[serde(rename = "testIds")]
    pub quarantined_tests: HashSet<String>,
}
