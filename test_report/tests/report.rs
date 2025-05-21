use std::{env, fs, io::BufReader, path::Path, thread};

use assert_matches::assert_matches;
use bundle::{BundleMeta, FileSetType};
use context::repo::RepoUrlParts;
use prost::Message;
use prost_wkt_types::Timestamp;
use proto::test_context::test_run::TestCaseRunStatus;
use tempfile::tempdir;
use test_report::report::{MutTestReport, Status};
use test_utils::mock_git_repo::setup_repo_with_commit;
use test_utils::mock_server::{MockServerBuilder, RequestPayload};

pub fn generate_mock_codeowners<T: AsRef<Path>>(directory: T) {
    const CODEOWNERS: &str = r#"
        test-file @user
        test-file2 @user @user2
    "#;
    fs::write(directory.as_ref().join("CODEOWNERS"), CODEOWNERS).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn publish_test_report() {
    let temp_dir = tempdir().unwrap();
    let repo_setup_res = setup_repo_with_commit(&temp_dir);
    generate_mock_codeowners(&temp_dir);
    assert!(repo_setup_res.is_ok());
    let set_current_dir_res = env::set_current_dir(&temp_dir);
    assert!(set_current_dir_res.is_ok());
    let state = MockServerBuilder::new().spawn_mock_server().await;
    env::set_var("TRUNK_PUBLIC_API_ADDRESS", &state.host);
    env::set_var("CI", "1");
    env::set_var("GITHUB_JOB", "test-job");
    env::set_var("TRUNK_API_TOKEN", "test-token");
    env::set_var("TRUNK_ORG_URL_SLUG", "test-org");

    let thread_join_handle = thread::spawn(|| {
        let test_report = MutTestReport::new("test".into(), "test-command 123".into());
        test_report.add_test(
            Some("1".into()),
            "test-name".into(),
            "test-classname".into(),
            "test-file".into(),
            "test-parent-name".into(),
            None,
            Status::Success,
            0,
            1000,
            1001,
            "test-message".into(),
            false,
        );
        // call this twice to later validate we only send one request
        test_report.is_quarantined(
            Some("2".into()),
            Some("test-name".into()),
            Some("test-parent-name".into()),
            Some("test-classname".into()),
            Some("test-file".into()),
        );
        test_report.is_quarantined(
            Some("2".into()),
            Some("test-name".into()),
            Some("test-parent-name".into()),
            Some("test-classname".into()),
            Some("test-file".into()),
        );
        test_report.add_test(
            Some("2".into()),
            "test-name".into(),
            "test-classname".into(),
            "test-file2".into(),
            "test-parent-name".into(),
            None,
            Status::Failure,
            0,
            1000,
            1001,
            "test-message".into(),
            true,
        );
        let result = test_report.publish();
        assert!(result);
    });
    thread_join_handle.join().unwrap();

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 4);
    let mut requests_iter = requests.iter();
    let list_quarantined_tests_request = assert_matches!(&requests_iter.next().unwrap(), RequestPayload::ListQuarantinedTests(d) => d);
    assert_eq!(list_quarantined_tests_request.org_url_slug, "test-org",);
    assert_eq!(
        list_quarantined_tests_request.repo,
        RepoUrlParts {
            host: "github.com".into(),
            owner: "trunk-io".into(),
            name: "analytics-cli".into()
        }
    );
    // validate we only send one list quarantined tests request
    assert_matches!(&requests_iter.next().unwrap(), RequestPayload::CreateBundleUpload(d) => d);
    let tar_extract_directory =
        assert_matches!(&requests_iter.next().unwrap(), RequestPayload::S3Upload(d) => d);
    let file = fs::File::open(tar_extract_directory.join("meta.json")).unwrap();
    let reader = BufReader::new(file);
    let bundle_meta: BundleMeta = serde_json::from_reader(reader).unwrap();
    let base_props = bundle_meta.base_props;
    assert_eq!(base_props.org, "test-org");
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
    assert_eq!(base_props.envs.get("CI"), Some(&String::from("1")));
    assert_eq!(
        base_props.envs.get("GITHUB_JOB"),
        Some(&String::from("test-job"))
    );
    let time_since_upload = chrono::Utc::now()
        - chrono::DateTime::from_timestamp(base_props.upload_time_epoch as i64, 0).unwrap();
    more_asserts::assert_lt!(time_since_upload.num_minutes(), 5);
    assert_eq!(base_props.test_command, Some("test-command 123".into()));
    assert!(base_props.os_info.is_some());
    assert_eq!(base_props.quarantined_tests.len(), 1);
    assert_eq!(base_props.quarantined_tests[0].id, "2");

    let file_set = base_props.file_sets.first().unwrap();
    assert_eq!(file_set.file_set_type, FileSetType::Internal);
    assert!(file_set.glob.ends_with(".bin"));
    assert_eq!(file_set.files.len(), 1);

    let junit_props = bundle_meta.junit_props;
    assert_eq!(junit_props.num_files, 1);
    assert_eq!(junit_props.num_tests, 2);

    let bundled_file = file_set.files.first().unwrap();
    assert_eq!(bundled_file.path, "internal/0");
    assert_eq!(bundled_file.owners.len(), 0);
    assert_eq!(bundled_file.team, None);

    let internal_bundled_file = bundle_meta.internal_bundled_file.unwrap();
    assert_eq!(internal_bundled_file.path, bundled_file.path);

    let bin = fs::read(tar_extract_directory.join(&bundled_file.path)).unwrap();
    let report = proto::test_context::test_run::TestResult::decode(&*bin).unwrap();

    let test_started_at = Timestamp {
        seconds: 1000,
        nanos: 0,
    };
    let test_finished_at = Timestamp {
        seconds: 1001,
        nanos: 0,
    };
    assert_eq!(report.test_case_runs.len(), 2);
    let test_case_run = &report.test_case_runs[0];
    assert_eq!(test_case_run.id, "1");
    assert_eq!(test_case_run.name, "test-name");
    assert_eq!(test_case_run.classname, "test-classname");
    assert_eq!(test_case_run.file, "test-file");
    assert_eq!(test_case_run.parent_name, "test-parent-name");
    assert_eq!(test_case_run.status, TestCaseRunStatus::Success as i32);
    assert_eq!(test_case_run.line, 0);
    assert_eq!(test_case_run.attempt_number, 0);
    assert_eq!(test_case_run.started_at, Some(test_started_at.clone()));
    assert_eq!(test_case_run.finished_at, Some(test_finished_at.clone()));
    assert!(!test_case_run.is_quarantined);
    assert_eq!(test_case_run.status_output_message, "test-message");
    assert_eq!(test_case_run.codeowners.len(), 1);

    let test_case_run = &report.test_case_runs[1];
    assert_eq!(test_case_run.id, "2");
    assert_eq!(test_case_run.name, "test-name");
    assert_eq!(test_case_run.classname, "test-classname");
    assert_eq!(test_case_run.file, "test-file2");
    assert_eq!(test_case_run.parent_name, "test-parent-name");
    assert_eq!(test_case_run.status, TestCaseRunStatus::Failure as i32);
    assert_eq!(test_case_run.line, 0);
    assert_eq!(test_case_run.attempt_number, 0);
    assert_eq!(test_case_run.started_at, Some(test_started_at));
    assert_eq!(test_case_run.finished_at, Some(test_finished_at));
    assert!(test_case_run.is_quarantined);
    assert_eq!(test_case_run.status_output_message, "test-message");
    assert_eq!(test_case_run.codeowners.len(), 2);
}

#[test]
fn test_mut_test_report_try_save() {
    use std::fs;

    use proto::test_context::test_run::TestResult;
    use tempfile::tempdir;
    use test_report::report::MutTestReport;

    let temp_dir = tempdir().unwrap();
    let report = MutTestReport::new("test-origin".into(), "test-command".into());
    let result = report.try_save(temp_dir.path().to_str().unwrap().to_string());
    assert!(result, "try_save should return true on success");

    let file_path = temp_dir.path().join("trunk_output.bin");
    assert!(file_path.exists(), "Saved file does not exist");
    let data = fs::read(&file_path).expect("Failed to read saved file");
    assert!(!data.is_empty(), "Saved file is empty");
    let deserialized = TestResult::decode(&*data).expect("Failed to decode TestResult");
    // The default TestResult should have no test_case_runs
    assert_eq!(deserialized.test_case_runs.len(), 0);
}
