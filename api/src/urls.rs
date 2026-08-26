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

/// The `{ id, createdAt }` pair the webapp's canonical upload route is keyed on.
///
/// Field order and the absence of whitespace exist to match `encodeBundleMetaKey` byte for
/// byte (trunk2 `ts/apps/frontend/src/lib/utils.ts`), which is what the golden test below
/// pins. The webapp's decoder parses JSON, so it is order-insensitive — a drift here would
/// still resolve, it would just stop being the same string the webapp itself emits.
#[derive(Serialize)]
struct BundleMetaKey<'a> {
    id: &'a str,
    #[serde(rename = "createdAt")]
    created_at: i64,
}

/// Link to a single upload, in the webapp's canonical `uploads/{bundleMetaKey}` form.
///
/// Needs no `repo` query param — unlike [`url_for_test_case`], the collection short id
/// fully scopes the upload.
///
/// The key carries the upload's timestamp because every query behind that page is keyed on
/// it (it is `test_collection_upload`'s partition key), and the server has no cheap way to
/// recover it from the id alone: that table's primary key leads with
/// `(test_collection_id, repo_id, …)` and a link carries no repo, so an id-only lookup
/// cannot use the index. Emitting the pair we already have avoids the lookup entirely.
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
    // base64url's alphabet is already path-safe, so `set_path` leaves the key alone.
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

    // Golden vector: this key is what `encodeBundleMetaKey` in trunk2
    // (`ts/apps/frontend/src/lib/utils.ts`) produces for the same id and instant, and it is
    // the string the webapp's `decodeBundleMetaKey` reads back. A change here silently
    // stops the CLI from emitting a URL the webapp can route.
    const UPLOAD_KEY: &str = "eyJpZCI6IjgyYzZhNmU1LWY4ZWEtNGQ5My05YTI2LWI4YWI2ZmY4ZjZiYyIsImNyZWF0ZWRBdCI6MTc4Nzc2ODIwODg3M30";

    #[test]
    fn test_upload_url_generated() {
        let actual = url_for_upload(
            "https://api.trunk-staging.io",
            "bad-app-org",
            "tc_123",
            "82c6a6e5-f8ea-4d93-9a26-b8ab6ff8f6bc",
            "2026-08-26T18:16:48.873Z",
        );

        assert_eq!(
            actual.unwrap(),
            format!(
                "https://app.trunk-staging.io/bad-app-org/flaky-tests/collections/tc_123/uploads/{UPLOAD_KEY}"
            ),
        );
    }

    // The bundle's timestamp is whatever offset the server sent; the key is keyed on the
    // instant, so an equivalent non-UTC spelling has to produce the same string.
    #[test]
    fn test_upload_url_is_offset_independent() {
        let utc = url_for_upload(
            "https://api.trunk-staging.io",
            "bad-app-org",
            "tc_123",
            "82c6a6e5-f8ea-4d93-9a26-b8ab6ff8f6bc",
            "2026-08-26T18:16:48.873Z",
        );
        let offset = url_for_upload(
            "https://api.trunk-staging.io",
            "bad-app-org",
            "tc_123",
            "82c6a6e5-f8ea-4d93-9a26-b8ab6ff8f6bc",
            "2026-08-26T11:16:48.873-07:00",
        );

        assert_eq!(utc.unwrap(), offset.unwrap());
    }

    #[test]
    fn test_upload_url_rejects_an_unparseable_timestamp() {
        let actual = url_for_upload(
            "https://api.trunk-staging.io",
            "bad-app-org",
            "tc_123",
            "82c6a6e5-f8ea-4d93-9a26-b8ab6ff8f6bc",
            "not-a-timestamp",
        );

        assert!(actual.is_err());
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
