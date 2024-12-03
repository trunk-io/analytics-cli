use crate::utils::{
    generate_mock_codeowners, generate_mock_git_repo, generate_mock_valid_junit_xmls, CARGO_RUN,
};
use api::GetQuarantineBulkTestStatusRequest;
use assert_cmd::Command;
use context::repo::RepoUrlParts as Repo;
use tempfile::tempdir;
use test_utils::mock_server::{MockServerBuilder, RequestPayload};

// NOTE: must be multi threaded to start a mock server
#[tokio::test(flavor = "multi_thread")]
async fn quarantine() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let args = &[
        "quarantine",
        "--junit-paths",
        "./*",
        "--org-url-slug",
        "test-org",
        "--token",
        "test-token",
    ];

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env("TRUNK_PUBLIC_API_ADDRESS", &state.host)
        .env("CI", "1")
        .env("GITHUB_JOB", "test-job")
        .args(args)
        .assert()
        .failure();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    let mut requests_iter = requests.into_iter();

    assert_eq!(
        requests_iter.next().unwrap(),
        RequestPayload::GetQuarantineBulkTestStatus(GetQuarantineBulkTestStatusRequest {
            repo: Repo {
                host: String::from("github.com"),
                owner: String::from("trunk-io"),
                name: String::from("analytics-cli"),
            },
            org_url_slug: String::from("test-org"),
        })
    );

    println!("{assert}");
}
