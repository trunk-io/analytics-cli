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
    /// Which source the server used to resolve quarantine status: "repo" or
    /// "test_collection". Absent on older servers, hence `Option` + `default`.
    #[serde(default)]
    pub quarantine_resolution_mode: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn base_request() -> GetQuarantineConfigRequest {
        GetQuarantineConfigRequest {
            repo: RepoUrlParts {
                host: String::from("github.com"),
                owner: String::from("trunk-io"),
                name: String::from("analytics-cli"),
            },
            remote_urls: vec![],
            org_url_slug: String::from("trunk"),
            test_identifiers: vec![],
            test_collection_short_id: None,
        }
    }

    #[test]
    fn serializes_test_collection_short_id_as_camel_case_when_set() {
        let request = GetQuarantineConfigRequest {
            test_collection_short_id: Some(String::from("abcd1234")),
            ..base_request()
        };
        let value: serde_json::Value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["testCollectionShortId"], "abcd1234");
    }

    #[test]
    fn omits_test_collection_short_id_when_absent() {
        let value: serde_json::Value = serde_json::to_value(base_request()).unwrap();
        assert!(value.get("testCollectionShortId").is_none());
    }

    #[test]
    fn deserializes_response_with_quarantine_resolution_mode() {
        let response: GetQuarantineConfigResponse = serde_json::from_str(
            r#"{ "isDisabled": false, "testIds": ["id1"], "quarantineResolutionMode": "test_collection" }"#,
        )
        .unwrap();
        assert_eq!(
            response.quarantine_resolution_mode.as_deref(),
            Some("test_collection")
        );
    }

    #[test]
    fn deserializes_response_without_quarantine_resolution_mode() {
        // Older servers omit the field; deserialization must still succeed.
        let response: GetQuarantineConfigResponse =
            serde_json::from_str(r#"{ "isDisabled": false, "testIds": [] }"#).unwrap();
        assert_eq!(response.quarantine_resolution_mode, None);
    }
}
