use crate::utils::{
    generate_mock_codeowners, generate_mock_git_repo, generate_mock_valid_junit_xmls, CARGO_RUN,
};
use assert_cmd::Command;
use assert_matches::assert_matches;
use bundle::BundleMeta;
use predicates::prelude::*;
use std::{fs, io::BufReader};
use tempfile::tempdir;
use test_utils::mock_server::{MockServerBuilder, RequestPayload};

// NOTE: must be multi threaded to start a mock server
#[tokio::test(flavor = "multi_thread")]
async fn test_command_succeeds_with_successful_upload() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let args = &[
        "test",
        "--junit-paths",
        "./*",
        "--org-url-slug",
        "test-org",
        "--token",
        "test-token",
        // Note: quarantining is disabled, as it intercepts failures if you don't actually produce a failing test file
        "--use-quarantining=false",
        "bash",
        "-c",
        "exit 0",
    ];

    Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env("TRUNK_PUBLIC_API_ADDRESS", &state.host)
        .env("CI", "1")
        .env("GITHUB_JOB", "test-job")
        .args(args)
        .assert()
        .success()
        .code(0);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_command_fails_with_successful_upload() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let args = &[
        "test",
        "--junit-paths",
        "./*",
        "--org-url-slug",
        "test-org",
        "--token",
        "test-token",
        "--use-quarantining=false",
        "bash",
        "-c",
        "exit 1",
    ];

    Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env("TRUNK_PUBLIC_API_ADDRESS", &state.host)
        .env("CI", "1")
        .env("GITHUB_JOB", "test-job")
        .args(args)
        .assert()
        .failure()
        .code(1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_command_fails_with_no_junit_files_no_quarantine_successful_upload() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let args = &[
        "test",
        "--junit-paths",
        "./*",
        "--org-url-slug",
        "test-org",
        "--token",
        "test-token",
        "bash",
        "-c",
        "exit 128",
    ];

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env("TRUNK_PUBLIC_API_ADDRESS", &state.host)
        .env("CI", "1")
        .env("GITHUB_JOB", "test-job")
        .args(args)
        .assert()
        .failure()
        .code(128)
        .stderr(predicate::str::contains(
            "No JUnit files found, not quarantining any tests",
        ));

    println!("{assert}");

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 4);
    let mut requests_iter = requests.into_iter();

    assert!(matches!(
        requests_iter.next().unwrap(),
        RequestPayload::CreateRepo(..)
    ));
    assert!(matches!(
        requests_iter.next().unwrap(),
        RequestPayload::CreateBundleUpload(..)
    ));

    let tar_extract_directory =
        assert_matches!(requests_iter.next().unwrap(), RequestPayload::S3Upload(d) => d);
    let file = fs::File::open(tar_extract_directory.join("meta.json")).unwrap();
    let reader = BufReader::new(file);
    let bundle_meta: BundleMeta = serde_json::from_reader(reader).unwrap();
    assert_eq!(
        bundle_meta.base_props.test_command.unwrap(),
        "bash -c exit 128"
    );

    assert!(matches!(
        requests_iter.next().unwrap(),
        RequestPayload::UpdateBundleUpload(..)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_command_succeeds_with_upload_not_connected() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let args = &[
        "test",
        "--junit-paths",
        "./*",
        "--org-url-slug",
        "test-org",
        "--token",
        "test-token",
        "--use-quarantining=false",
        "bash",
        "-c",
        "exit 0",
    ];

    Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env("TRUNK_PUBLIC_API_ADDRESS", "https://localhost:10")
        .env("CI", "1")
        .env("GITHUB_JOB", "test-job")
        .args(args)
        .assert()
        .success()
        .code(0);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_command_fails_with_upload_not_connected() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let args = &[
        "test",
        "--junit-paths",
        "./*",
        "--org-url-slug",
        "test-org",
        "--token",
        "test-token",
        "--use-quarantining=false",
        "bash",
        "-c",
        "exit 1",
    ];

    Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env("TRUNK_PUBLIC_API_ADDRESS", "https://localhost:10")
        .env("CI", "1")
        .env("GITHUB_JOB", "test-job")
        .args(args)
        .assert()
        .failure()
        .code(1);
}
