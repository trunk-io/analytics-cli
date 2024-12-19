use prost::Message;
use prost_wkt_types::Timestamp;
use proto::test_context::test_run::TestCaseRunStatus;
use tempfile::tempdir;
use test_report::report::{MutTestReport, Status};
use test_utils::mock_git_repo::setup_repo_with_commit;
use test_utils::mock_server::MockServerBuilder;

#[tokio::test(flavor = "multi_thread")]
async fn publish_test_report() {
    let temp_dir = tempdir().unwrap();
    let repo_setup_res = setup_repo_with_commit(&temp_dir);
    assert!(repo_setup_res.is_ok());
    let set_current_dir_res = std::env::set_current_dir(&temp_dir);
    assert!(set_current_dir_res.is_ok());
    let state = MockServerBuilder::new().spawn_mock_server().await;
    std::env::set_var("TRUNK_PUBLIC_API_ADDRESS", &state.host);
    std::env::set_var("CI", "1");
    std::env::set_var("GITHUB_JOB", "test-job");
    std::env::set_var("TRUNK_API_TOKEN", "test-token");
    std::env::set_var("TRUNK_ORG_URL_SLUG", "test-org");

    let thread_join_handle = std::thread::spawn(|| {
        let test_report = MutTestReport::new("test".into());
        test_report.add_test(
            Some("1".into()),
            "test-name".into(),
            "test-classname".into(),
            "test-file".into(),
            "test-parent-name".into(),
            None,
            Status::Success,
            0,
            0,
            0,
            "test-message".into(),
        );
        let result = test_report.publish(".".into());
        assert_eq!(result, true);
    });
    let thread_res = thread_join_handle.join();
    assert!(thread_res.is_ok());
}

#[test]
fn save_test_report() {
    let temp_dir = tempdir().unwrap();
    setup_repo_with_commit(&temp_dir).unwrap();
    let _ = std::env::set_current_dir(&temp_dir);
    let test_report = MutTestReport::new("test".into());
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
    let result = test_report.save();
    assert!(result.is_ok());

    // read the file result
    let file = std::fs::read(result.unwrap());
    let report = proto::test_context::test_run::TestResult::decode(&*file.unwrap()).unwrap();

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
