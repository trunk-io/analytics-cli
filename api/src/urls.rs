use bundle::Test;
use context::repo::RepoUrlParts;
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

    // DONOTLAND: intentional failure so CI's self-upload prints the new collection link — revert before merge
    #[test]
    fn donotland_intentional_failure_to_preview_collection_link() {
        panic!("DONOTLAND: intentional failure to preview the test collection link in CI output");
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
