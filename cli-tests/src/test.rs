use crate::utils::{
    generate_mock_codeowners, generate_mock_git_repo, generate_mock_valid_junit_xmls, CARGO_RUN,
};
use assert_cmd::Command;
use tempfile::tempdir;
use test_utils::mock_server::MockServerBuilder;

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
        "--use-quarantining",
        "false",
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
        "--use-quarantining",
        "false",
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
        "--use-quarantining",
        "false",
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
        "--use-quarantining",
        "false",
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
