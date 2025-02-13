use std::{fs, io::BufReader};

use assert_matches::assert_matches;
use bundle::BundleMeta;
use context::{bazel_bep::parser::BazelBepParser, junit::parser::JunitParser};
use predicates::prelude::*;
use tempfile::tempdir;
use test_utils::mock_server::{MockServerBuilder, RequestPayload};

use crate::{
    command_builder::CommandBuilder,
    utils::{
        generate_mock_bazel_bep, generate_mock_codeowners, generate_mock_git_repo,
        generate_mock_valid_junit_xmls,
    },
};

// NOTE: must be multi threaded to start a mock server
#[tokio::test(flavor = "multi_thread")]
async fn test_command_succeeds_with_successful_upload() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    CommandBuilder::test(
        temp_dir.path(),
        state.host.clone(),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("exit 0"),
        ],
    )
    .use_quarantining(false)
    .command()
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

    CommandBuilder::test(
        temp_dir.path(),
        state.host.clone(),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("exit 1"),
        ],
    )
    .use_quarantining(false)
    .command()
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

    let assert = CommandBuilder::test(
        temp_dir.path(),
        state.host.clone(),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("exit 128"),
        ],
    )
    .command()
    .assert()
    .failure()
    .code(128)
    .stdout(predicate::str::contains(
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

    CommandBuilder::test(
        temp_dir.path(),
        String::from("https://localhost:10"),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("exit 0"),
        ],
    )
    .use_quarantining(false)
    .command()
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

    CommandBuilder::test(
        temp_dir.path(),
        String::from("https://localhost:10"),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("exit 1"),
        ],
    )
    .use_quarantining(false)
    .command()
    .assert()
    .failure()
    .code(1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_command_succeeds_with_bundle_using_bep() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_bazel_bep(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let assert = CommandBuilder::test(
        temp_dir.path(),
        state.host.clone(),
        vec![
            String::from("bash"),
            String::from("-c"),
            String::from("exit 1"),
        ],
    )
    .bazel_bep_path("./bep.json")
    .command()
    .assert()
    .failure();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 5);

    let tar_extract_directory = assert_matches!(&requests[3], RequestPayload::S3Upload(d) => d);

    let junit_file = fs::File::open(tar_extract_directory.join("junit/0")).unwrap();
    let junit_reader = BufReader::new(junit_file);

    // Uploaded file is a junit, even when using BEP
    let mut junit_parser = JunitParser::new();
    assert!(junit_parser.parse(junit_reader).is_ok());
    assert!(junit_parser.issues().is_empty());

    let mut bazel_bep_parser = BazelBepParser::new(tar_extract_directory.join("bazel_bep.json"));
    let parse_result = bazel_bep_parser.parse().ok().unwrap();
    assert!(parse_result.errors.is_empty());
    assert_eq!(parse_result.xml_file_counts(), (1, 0));

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}
