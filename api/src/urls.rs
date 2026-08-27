use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use bundle::Test;
use chrono::DateTime;
use context::repo::RepoUrlParts;
use serde::Serialize;
use url::{ParseError, Url, form_urlencoded};

pub fn url_for_test_case(
    public_api_address: &str,
    org_url_slug: &String,
    repo: &RepoUrlParts,
    test_case: &Test,
    test_collection_short_id: Option<&str>,
) -> Result<String, ParseError> {
    let mut url = Url::parse(convert_to_app_url(public_api_address).as_str())?;
    let path = match test_collection_short_id {
        Some(short_id) => collection_test_path(org_url_slug, short_id, test_case),
        None => test_path(org_url_slug, test_case),
    };
    url.set_path(path.as_str());
    url.set_query(Some(repo_query(repo).as_str()));
    Ok(url.to_string())
}

/// Serialized to match trunk2's `encodeBundleMetaKey` byte for byte — hence the field order
/// and a `Serialize` struct rather than a `format!`.
#[derive(Serialize)]
struct BundleMetaKey<'a> {
    id: &'a str,
    #[serde(rename = "createdAt")]
    created_at: i64,
}

/// Link to a single upload, in the webapp's canonical `uploads/{bundleMetaKey}` form. Needs
/// no `repo` query param — the collection short id fully scopes the upload.
///
/// The key carries the timestamp because the server cannot cheaply recover it from the id:
/// `test_collection_upload`'s primary key leads with `(test_collection_id, repo_id, …)`, and
/// a link carries no repo, so an id-only lookup cannot use the index.
pub fn url_for_upload(
    public_api_address: &str,
    org_url_slug: &str,
    test_collection_short_id: &str,
    bundle_meta_id: &str,
    bundle_meta_created_at: &str,
) -> anyhow::Result<String> {
    let created_at = DateTime::parse_from_rfc3339(bundle_meta_created_at).with_context(|| {
        format!("Unparseable test_collection_bundle_meta_created_at: {bundle_meta_created_at}")
    })?;
    let key = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&BundleMetaKey {
        id: bundle_meta_id,
        created_at: created_at.timestamp_millis(),
    })?);

    let mut url = Url::parse(convert_to_app_url(public_api_address).as_str())?;
    url.set_path(
        format!("{org_url_slug}/flaky-tests/collections/{test_collection_short_id}/uploads/{key}")
            .as_str(),
    );
    Ok(url.to_string())
}

fn convert_to_app_url(public_api_address: &str) -> String {
    public_api_address.replace("https://api.", "https://app.")
}

fn test_path(org_url_slug: &String, test_case: &Test) -> String {
    format!("{}/flaky-tests/test/{}", org_url_slug, test_case.id)
}

// Short-link form: the webapp resolves the repo query param to a repo id and
// redirects to the canonical collections/<short_id>/tests/<repo_id>_<test_case_id> page.
fn collection_test_path(org_url_slug: &String, short_id: &str, test_case: &Test) -> String {
    format!(
        "{}/flaky-tests/collections/{}/t/{}",
        org_url_slug, short_id, test_case.id
    )
}

fn repo_query(repo: &RepoUrlParts) -> String {
    let value: String =
        form_urlencoded::byte_serialize(format!("{}/{}", repo.owner, repo.name).as_bytes())
            .collect();
    format!("repo={}", value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_repo() -> RepoUrlParts {
        RepoUrlParts {
            host: String::from("https://github.com"),
            owner: String::from("bad-app"),
            name: String::from("ios-app"),
        }
    }

    fn test_case() -> Test {
        Test {
            name: String::from("can math"),
            parent_name: String::from("basic suite"),
            class_name: None,
            file: None,
            id: String::from("c33a7f64-8f3e-5db9-b37b-2ea870d2441b"),
            timestamp_millis: None,
            is_quarantined: false,
            failure_message: None,
            variant: None,
        }
    }

    #[test]
    fn test_url_generated() {
        let actual = url_for_test_case(
            &String::from("https://api.trunk-staging.io"),
            &String::from("bad-app-org"),
            &test_repo(),
            &test_case(),
            None,
        );

        assert_eq!(
            actual,
            Ok(String::from(
                "https://app.trunk-staging.io/bad-app-org/flaky-tests/test/c33a7f64-8f3e-5db9-b37b-2ea870d2441b?repo=bad-app%2Fios-app"
            )),
        );
    }

    const UPLOAD_ID: &str = "c8034184-a9c2-5c53-be91-ef38ffb90df9";
    // 1787777896873
    const UPLOAD_CREATED_AT: &str = "2026-08-26T20:58:16.873Z";
    // base64url of {"id":"c8034184-a9c2-5c53-be91-ef38ffb90df9","createdAt":1787777896873},
    // cross-checked against trunk2's `encodeBundleMetaKey`. A change here stops the webapp
    // from routing the URL we print.
    const UPLOAD_KEY: &str = "eyJpZCI6ImM4MDM0MTg0LWE5YzItNWM1My1iZTkxLWVmMzhmZmI5MGRmOSIsImNyZWF0ZWRBdCI6MTc4Nzc3Nzg5Njg3M30";

    fn upload_url(created_at: &str) -> anyhow::Result<String> {
        url_for_upload(
            "https://api.trunk-staging.io",
            "bad-app-org",
            "tc_123",
            UPLOAD_ID,
            created_at,
        )
    }

    #[test]
    fn test_upload_url_generated() {
        assert_eq!(
            upload_url(UPLOAD_CREATED_AT).unwrap(),
            format!(
                "https://app.trunk-staging.io/bad-app-org/flaky-tests/collections/tc_123/uploads/{UPLOAD_KEY}"
            ),
        );
    }

    // The key is keyed on the instant, and the server picks the offset it sends.
    #[test]
    fn test_upload_url_is_offset_independent() {
        assert_eq!(
            upload_url("2026-08-26T13:58:16.873-07:00").unwrap(),
            upload_url(UPLOAD_CREATED_AT).unwrap(),
        );
    }

    #[test]
    fn test_upload_url_rejects_an_unparseable_timestamp() {
        assert!(upload_url("not-a-timestamp").is_err());
    }

    #[test]
    fn test_collection_url_generated() {
        let actual = url_for_test_case(
            &String::from("https://api.trunk-staging.io"),
            &String::from("bad-app-org"),
            &test_repo(),
            &test_case(),
            Some("tc_123"),
        );

        assert_eq!(
            actual,
            Ok(String::from(
                "https://app.trunk-staging.io/bad-app-org/flaky-tests/collections/tc_123/t/c33a7f64-8f3e-5db9-b37b-2ea870d2441b?repo=bad-app%2Fios-app"
            )),
        );
    }
}
