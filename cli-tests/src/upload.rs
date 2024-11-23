use std::{fs, io::BufReader};

use crate::utils::{
    generate_mock_codeowners, generate_mock_git_repo, generate_mock_valid_junit_xmls, CARGO_RUN,
};
use api::{
    BundleUploadStatus, CreateRepoRequest, GetQuarantineBulkTestStatusRequest,
    UpdateBundleUploadRequest,
};
use assert_cmd::Command;
use assert_matches::assert_matches;
use bundle::{BundleMeta, FileSetType};
use codeowners::CodeOwners;
use context::repo::RepoUrlParts as Repo;
use predicates::prelude::*;
use tempfile::tempdir;
use test_utils::mock_server::{MockServerBuilder, RequestPayload};

// NOTE: must be multi threaded to start a mock server
#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env("TRUNK_PUBLIC_API_ADDRESS", &state.host)
        .env("CI", "1")
        .env("GITHUB_JOB", "test-job")
        .args(&[
            "upload",
            "--junit-paths",
            "./*",
            "--org-url-slug",
            "test-org",
            "--token",
            "test-token",
        ])
        .assert()
        // should fail due to quarantine and succeed without quarantining
        .failure();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 5);
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

    assert_eq!(
        requests_iter.next().unwrap(),
        RequestPayload::CreateRepo(CreateRepoRequest {
            repo: Repo {
                host: String::from("github.com"),
                owner: String::from("trunk-io"),
                name: String::from("analytics-cli"),
            },
            org_url_slug: String::from("test-org"),
            remote_urls: Vec::from(&[String::from(
                "https://github.com/trunk-io/analytics-cli.git"
            )]),
        })
    );

    let upload_request = assert_matches!(requests_iter.next().unwrap(), RequestPayload::CreateBundleUpload(ur) => ur);
    assert_eq!(
        upload_request.repo,
        Repo {
            host: String::from("github.com"),
            owner: String::from("trunk-io"),
            name: String::from("analytics-cli"),
        }
    );
    assert_eq!(upload_request.org_url_slug, "test-org");
    assert!(upload_request
        .client_version
        .starts_with("trunk-analytics-cli cargo="));
    assert!(upload_request.client_version.contains(" git="));
    assert!(upload_request.client_version.contains(" rustc="));

    let tar_extract_directory =
        assert_matches!(requests_iter.next().unwrap(), RequestPayload::S3Upload(d) => d);

    let file = fs::File::open(tar_extract_directory.join("meta.json")).unwrap();
    let reader = BufReader::new(file);
    let bundle_meta: BundleMeta = serde_json::from_reader(reader).unwrap();
    let base_props = bundle_meta.base_props;
    let junit_props = bundle_meta.junit_props;

    assert_eq!(base_props.org, "test-org");
    assert_eq!(
        base_props.repo.repo,
        Repo {
            host: String::from("github.com"),
            owner: String::from("trunk-io"),
            name: String::from("analytics-cli"),
        }
    );
    assert_eq!(
        base_props.repo.repo_url,
        "https://github.com/trunk-io/analytics-cli.git"
    );
    assert!(!base_props.repo.repo_head_sha.is_empty());
    let repo_head_sha_short = base_props.repo.repo_head_sha_short.unwrap();
    assert!(!repo_head_sha_short.is_empty());
    assert!(&repo_head_sha_short.len() < &base_props.repo.repo_head_sha.len());
    assert!(base_props
        .repo
        .repo_head_sha
        .starts_with(&repo_head_sha_short));
    assert_eq!(base_props.repo.repo_head_branch, "refs/heads/trunk/test");
    assert_eq!(base_props.repo.repo_head_author_name, "Your Name");
    assert_eq!(
        base_props.repo.repo_head_author_email,
        "your.email@example.com"
    );
    assert_eq!(base_props.bundle_upload_id, "test-bundle-upload-id");
    assert_eq!(base_props.tags, &[]);
    assert_eq!(base_props.file_sets.len(), 1);
    assert_eq!(junit_props.num_files, 1);
    assert_eq!(junit_props.num_tests, 500);
    assert_eq!(base_props.envs.get("CI"), Some(&String::from("1")));
    assert_eq!(
        base_props.envs.get("GITHUB_JOB"),
        Some(&String::from("test-job"))
    );
    let time_since_upload = chrono::Utc::now()
        - chrono::DateTime::from_timestamp(base_props.upload_time_epoch as i64, 0).unwrap();
    more_asserts::assert_lt!(time_since_upload.num_minutes(), 5);
    assert_eq!(base_props.test_command, None);
    assert!(base_props.os_info.is_some());
    assert!(base_props.quarantined_tests.is_empty());
    assert_eq!(
        base_props.codeowners,
        Some(CodeOwners {
            path: temp_dir.as_ref().join("CODEOWNERS").canonicalize().unwrap(),
            owners: None,
        })
    );

    let file_set = base_props.file_sets.get(0).unwrap();
    assert_eq!(file_set.file_set_type, FileSetType::Junit);
    assert_eq!(file_set.glob, "./*");
    assert_eq!(file_set.files.len(), 1);

    let bundled_file = file_set.files.get(0).unwrap();
    assert_eq!(bundled_file.path, "junit/0");
    assert!(
        fs::File::open(tar_extract_directory.join(&bundled_file.path))
            .unwrap()
            .metadata()
            .unwrap()
            .is_file()
    );
    assert_eq!(bundled_file.owners, ["@user"]);
    assert_eq!(bundled_file.team, None);

    assert_eq!(
        requests_iter.next().unwrap(),
        RequestPayload::UpdateBundleUpload(UpdateBundleUploadRequest {
            id: "test-bundle-upload-id".to_string(),
            upload_status: BundleUploadStatus::UploadComplete
        }),
    );

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_empty_junit_paths() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);

    let state = MockServerBuilder::new().spawn_mock_server().await;

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env("TRUNK_PUBLIC_API_ADDRESS", &state.host)
        .env("CI", "1")
        .args(&[
            "upload",
            "--junit-paths",
            "",
            "--org-url-slug",
            "test-org",
            "--token",
            "test-token",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "error: a value is required for '--junit-paths <JUNIT_PATHS>' but none was supplied",
        ));

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_no_files_allow_missing_junit_files() {
    enum Flag {
        Long,
        Alias,
        Default,
        Off,
    }

    for flag in [Flag::Long, Flag::Alias, Flag::Default, Flag::Off] {
        let temp_dir = tempdir().unwrap();
        generate_mock_git_repo(&temp_dir);

        let state = MockServerBuilder::new().spawn_mock_server().await;

        let mut args = vec![
            "upload",
            "--junit-paths",
            "./*",
            "--org-url-slug",
            "test-org",
            "--token",
            "test-token",
        ];

        match flag {
            Flag::Long => args.push("--allow-empty-test-results"),
            Flag::Alias => args.push("--allow-missing-junit-files"),
            Flag::Default => {}
            Flag::Off => {
                args.push("--allow-empty-test-results false");
            }
        };

        let mut assert = Command::new(CARGO_RUN.path())
            .current_dir(&temp_dir)
            .env("TRUNK_PUBLIC_API_ADDRESS", &state.host)
            .env("CI", "1")
            .args(&args)
            .assert();

        assert = if matches!(flag, Flag::Off) {
            assert.failure()
        } else {
            assert.success()
        };

        // HINT: View CLI output with `cargo test -- --nocapture`
        println!("{assert}");
    }
}
