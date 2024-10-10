use std::{
    env, fs,
    io::BufReader,
    path::{Path, PathBuf},
};

use api::{CreateRepoRequest, GetQuarantineBulkTestStatusRequest};
use assert_cmd::Command;
use assert_matches::assert_matches;
use context::repo::RepoUrlParts as Repo;
use escargot::{CargoBuild, CargoRun};
use junit_mock::JunitMock;
use lazy_static::lazy_static;
use tempfile::tempdir;
use test_utils::{
    mock_git_repo::setup_repo_with_commit,
    mock_server::{spawn_mock_server, RequestPayload},
};
use trunk_analytics_cli::{
    codeowners::CodeOwners,
    types::{BundleMeta, FileSetType},
};

lazy_static! {
    static ref CARGO_MANIFEST_DIR: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    static ref CARGO_RUN: CargoRun = CargoBuild::new()
        .bin("trunk-analytics-cli")
        .target_dir(CARGO_MANIFEST_DIR.join("../target"))
        .manifest_path(CARGO_MANIFEST_DIR.join("../cli/Cargo.toml"))
        .features("force-sentry-env-dev")
        .current_release()
        .current_target()
        .run()
        .unwrap();
}

fn generate_mock_git_repo<T: AsRef<Path>>(directory: T) {
    setup_repo_with_commit(directory).unwrap();
}

fn generate_mock_junit_xmls<T: AsRef<Path>>(directory: T) {
    let mut jm = JunitMock::new(junit_mock::Options::default());
    let reports = jm.generate_reports();
    JunitMock::write_reports_to_file(directory.as_ref(), reports).unwrap();
}

fn generate_mock_codeowners<T: AsRef<Path>>(directory: T) {
    const CODEOWNERS: &str = r#"
        [Owners of Everything]
        * @user
    "#;
    fs::write(directory.as_ref().join("CODEOWNERS"), CODEOWNERS).unwrap();
}

// NOTE: must be multi threaded to start a mock server
#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);
    generate_mock_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let state = spawn_mock_server().await;

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env("TRUNK_PUBLIC_API_ADDRESS", &state.host)
        .env("CI", "1")
        .args(&[
            "upload",
            "--use-quarantining",
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
    assert_eq!(requests.len(), 4);
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

    assert_eq!(bundle_meta.org, "test-org");
    assert_eq!(
        bundle_meta.repo.repo,
        Repo {
            host: String::from("github.com"),
            owner: String::from("trunk-io"),
            name: String::from("analytics-cli"),
        }
    );
    assert_eq!(
        bundle_meta.repo.repo_url,
        "https://github.com/trunk-io/analytics-cli.git"
    );
    assert_eq!(bundle_meta.repo.repo_head_branch, "refs/heads/trunk/test");
    assert_eq!(bundle_meta.repo.repo_head_author_name, "Your Name");
    assert_eq!(
        bundle_meta.repo.repo_head_author_email,
        "your.email@example.com"
    );
    assert_eq!(bundle_meta.bundle_upload_id, "test-bundle-upload-id");
    assert_eq!(bundle_meta.tags, &[]);
    assert_eq!(bundle_meta.file_sets.len(), 1);
    assert_eq!(bundle_meta.envs.get("CI"), Some(&String::from("1")));
    let time_since_upload = chrono::Utc::now()
        - chrono::DateTime::from_timestamp(bundle_meta.upload_time_epoch as i64, 0).unwrap();
    more_asserts::assert_lt!(time_since_upload.num_minutes(), 5);
    assert_eq!(bundle_meta.test_command, None);
    assert!(bundle_meta.os_info.is_some());
    assert!(bundle_meta.quarantined_tests.is_empty());
    assert_eq!(
        bundle_meta.codeowners,
        Some(CodeOwners {
            path: temp_dir.as_ref().join("CODEOWNERS").canonicalize().unwrap(),
            owners: None,
        })
    );

    let file_set = bundle_meta.file_sets.get(0).unwrap();
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
    let time_since_junit_modified = chrono::Utc::now()
        - chrono::DateTime::from_timestamp_nanos(bundled_file.last_modified_epoch_ns as i64);
    more_asserts::assert_lt!(time_since_junit_modified.num_minutes(), 5);
    assert_eq!(bundled_file.owners, ["@user"]);
    assert_eq!(bundled_file.team, None);

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

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_no_files() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);

    let state = spawn_mock_server().await;

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env("TRUNK_PUBLIC_API_ADDRESS", &state.host)
        .env("CI", "1")
        .args(&[
            "upload",
            "--use-quarantining",
            "--junit-paths",
            "./*",
            "--org-url-slug",
            "test-org",
            "--token",
            "test-token",
        ])
        .assert()
        .failure();

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}

#[tokio::test(flavor = "multi_thread")]
async fn upload_bundle_no_files_allow_missing_junit_files() {
    let temp_dir = tempdir().unwrap();
    generate_mock_git_repo(&temp_dir);

    let state = spawn_mock_server().await;

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .env("TRUNK_PUBLIC_API_ADDRESS", &state.host)
        .env("CI", "1")
        .args(&[
            "upload",
            "--use-quarantining",
            "--junit-paths",
            "./*",
            "--org-url-slug",
            "test-org",
            "--token",
            "test-token",
            "--allow-missing-junit-files",
        ])
        .assert()
        .success();

    // HINT: View CLI output with `cargo test -- --nocapture`
    println!("{assert}");
}
