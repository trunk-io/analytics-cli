use std::{env, fs, io::BufReader, thread};

use assert_matches::assert_matches;
use bundle::{BundleMeta, FileSetType};
use prost::Message;
use prost_wkt_types::Timestamp;
use proto::test_context::test_run::TestCaseRunStatus;
use tempfile::tempdir;
use test_report::report::{MutTestReport, Status};
use test_utils::mock_git_repo::setup_repo_with_commit;
use test_utils::mock_server::{MockServerBuilder, RequestPayload};

#[tokio::test(flavor = "multi_thread")]
async fn publish_test_report() {
    let temp_dir = tempdir().unwrap();
    let repo_setup_res = setup_repo_with_commit(&temp_dir);
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
        );
        let result = test_report.publish();
        assert!(result);
    });
    thread_join_handle.join().unwrap();

    let requests = state.requests.lock().unwrap().clone();
    assert!(requests.len() == 3);
    let tar_extract_directory = assert_matches!(&requests[1], RequestPayload::S3Upload(d) => d);
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
    assert!(base_props.quarantined_tests.is_empty());

    let file_set = base_props.file_sets.first().unwrap();
    assert_eq!(file_set.file_set_type, FileSetType::Junit);
    assert!(file_set.glob.ends_with(".bin"));
    assert_eq!(file_set.files.len(), 1);

    let junit_props = bundle_meta.junit_props;
    assert_eq!(junit_props.num_files, 1);
    assert_eq!(junit_props.num_tests, 1);

    let bundled_file = file_set.files.first().unwrap();
    assert_eq!(bundled_file.path, "internal/0");
    assert_eq!(bundled_file.owners.len(), 0);
    assert_eq!(bundled_file.team, None);

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
    assert_eq!(report.test_case_runs.len(), 1);
    assert_eq!(report.test_case_runs[0].id, "1");
    assert_eq!(report.test_case_runs[0].name, "test-name");
    assert_eq!(report.test_case_runs[0].classname, "test-classname");
    assert_eq!(report.test_case_runs[0].file, "test-file");
    assert_eq!(report.test_case_runs[0].parent_name, "test-parent-name");
    assert_eq!(
        report.test_case_runs[0].status,
        TestCaseRunStatus::Success as i32
    );
    assert_eq!(report.test_case_runs[0].line, 0);
    assert_eq!(report.test_case_runs[0].attempt_number, 0);
    assert_eq!(report.test_case_runs[0].started_at, Some(test_started_at));
    assert_eq!(report.test_case_runs[0].finished_at, Some(test_finished_at));
    assert_eq!(
        report.test_case_runs[0].status_output_message,
        "test-message"
    );
}
