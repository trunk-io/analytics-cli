use bundle::Test;
use context::repo::RepoUrlParts;
use url::{ParseError, Url, form_urlencoded};

pub fn url_for_test_case(
    public_api_address: &str,
    org_url_slug: &String,
    repo: &RepoUrlParts,
    test_case: &Test,
) -> Result<String, ParseError> {
    let mut url = Url::parse(convert_to_app_url(public_api_address).as_str())?;
    url.set_path(test_path(org_url_slug, test_case).as_str());
    url.set_query(Some(repo_query(repo).as_str()));
    Ok(url.to_string())
}

fn convert_to_app_url(public_api_address: &str) -> String {
    public_api_address.replace("https://api.", "https://app.")
}

fn test_path(org_url_slug: &String, test_case: &Test) -> String {
    format!("{}/flaky-tests/test/{}", org_url_slug, test_case.id)
}

fn repo_query(repo: &RepoUrlParts) -> String {
    let value: String =
        form_urlencoded::byte_serialize(format!("{}/{}", repo.owner, repo.name).as_bytes())
            .collect();
    format!("repo={}", value)
}

#[test]
fn test_url_generated() {
    let repo = RepoUrlParts {
        host: String::from("https://github.com"),
        owner: String::from("bad-app"),
        name: String::from("ios-app"),
    };

    let test = Test {
        name: String::from("can math"),
        parent_name: String::from("basic suite"),
        class_name: None,
        file: None,
        id: String::from("c33a7f64-8f3e-5db9-b37b-2ea870d2441b"),
        timestamp_millis: None,
        is_quarantined: false,
        failure_message: None,
        variant: None,
    };

    let actual = url_for_test_case(
        &String::from("https://api.trunk-staging.io"),
        &String::from("bad-app-org"),
        &repo,
        &test,
    );

    assert_eq!(
        actual,
        Ok(String::from(
            "https://app.trunk-staging.io/bad-app-org/flaky-tests/test/c33a7f64-8f3e-5db9-b37b-2ea870d2441b?repo=bad-app%2Fios-app"
        )),
    );
}
