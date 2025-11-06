use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

use bazel_bep::types::build_event_stream::{
    build_event::Payload,
    build_event_id::{Id, TestResultId, TestSummaryId},
    file::File::Uri,
    BuildEvent, BuildEventId, File, TestResult, TestStatus, TestSummary,
};
use chrono::{TimeDelta, Utc};
use clap::Parser;
use junit_mock::JunitMock;
use lazy_static::lazy_static;
use test_utils::mock_git_repo::setup_repo_with_commit;

lazy_static! {
    pub static ref CLI_BIN_PATH: PathBuf = {
        // Manually construct the binary path, detecting whether we're running under cargo-llvm-cov.
        // cargo-llvm-cov places binaries in target/llvm-cov-target/debug/ instead of target/debug/
        // This ensures the binary is instrumented for coverage, unlike escargot which builds separately.
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let target_dir = manifest_dir.parent().unwrap().join("target");

        // Check if we're running under cargo-llvm-cov
        let binary_name = if cfg!(windows) {
            "trunk-analytics-cli.exe"
        } else {
            "trunk-analytics-cli"
        };

        // Try llvm-cov-target first (for coverage), then fall back to regular target
        let llvm_cov_path = target_dir.join("llvm-cov-target/debug").join(binary_name);
        if llvm_cov_path.exists() {
            llvm_cov_path
        } else {
            target_dir.join("debug").join(binary_name)
        }
    };
}

pub fn generate_mock_git_repo<T: AsRef<Path>>(directory: T) {
    setup_repo_with_commit(directory).unwrap();
}

fn generate_mock_valid_junit_mocker() -> JunitMock {
    JunitMock::new(junit_mock::Options::default())
}

pub fn generate_mock_valid_junit_xmls<T: AsRef<Path>>(directory: T) -> Vec<PathBuf> {
    let mut jm = generate_mock_valid_junit_mocker();
    let reports = jm.generate_reports();
    jm.write_reports_to_file(directory.as_ref(), reports)
        .unwrap()
}

pub fn generate_mock_valid_junit_xmls_with_failures<T: AsRef<Path>>(directory: T) -> Vec<PathBuf> {
    let test_case_options = junit_mock::TestCaseOptions {
        test_case_names: Some(vec![String::from("test_case")]),
        test_case_classnames: Some(vec![String::from("TestClass")]),
        test_case_random_count: 0usize,
        test_case_sys_out_percentage: 0u8,
        test_case_sys_err_percentage: 0u8,
        test_case_duration_range: vec![Duration::new(10, 0).into(), Duration::new(20, 0).into()],
        test_case_success_to_skip_to_fail_to_error_percentage: vec![vec![0u8, 0u8, 100u8, 0u8]],
    };
    let options = junit_mock::Options {
        global: junit_mock::GlobalOptions::try_parse_from([""]).unwrap(),
        report: junit_mock::ReportOptions::try_parse_from([""]).unwrap(),
        test_suite: junit_mock::TestSuiteOptions::try_parse_from([""]).unwrap(),
        test_case: test_case_options,
        test_rerun: junit_mock::TestRerunOptions::try_parse_from([""]).unwrap(),
    };
    let mut mock = JunitMock::new(options);
    let reports = mock.generate_reports();
    mock.write_reports_to_file(directory.as_ref(), reports)
        .unwrap()
}

pub fn generate_mock_bazel_bep<T: AsRef<Path>>(directory: T) -> PathBuf {
    let mut jm = generate_mock_valid_junit_mocker();
    let reports = jm.generate_reports();
    let mock_junits = jm
        .write_reports_to_file(directory.as_ref(), &reports)
        .unwrap();

    let build_events = mock_junits
        .iter()
        .zip(reports.iter())
        .flat_map(|(junit, report)| {
            let file = File {
                name: junit.file_name().unwrap().to_str().unwrap().to_string(),
                file: Some(Uri(junit.to_string_lossy().to_string())),
                ..Default::default()
            };
            let test_start_time = report.timestamp.map(|ts| ts.to_utc().into());
            let test_duration = report.time.map(|d| d.into());
            [
                BuildEvent {
                    id: Some(BuildEventId {
                        id: Some(Id::TestResult(TestResultId {
                            label: "//path:test".to_string(),
                            ..Default::default()
                        })),
                    }),
                    children: vec![],
                    last_message: false,
                    payload: Some(Payload::TestResult(TestResult {
                        status: TestStatus::Passed.into(),
                        test_attempt_start: test_start_time.clone(),
                        test_attempt_duration: test_duration.clone(),
                        test_action_output: vec![file.clone()],
                        ..Default::default()
                    })),
                },
                BuildEvent {
                    id: Some(BuildEventId {
                        id: Some(Id::TestSummary(TestSummaryId {
                            label: "//path:test".to_string(),
                            ..Default::default()
                        })),
                    }),
                    children: vec![],
                    last_message: false,
                    payload: Some(Payload::TestSummary(TestSummary {
                        overall_status: TestStatus::Passed.into(),
                        total_run_count: 1,
                        run_count: 1,
                        attempt_count: 1,
                        shard_count: 1,
                        passed: vec![file],
                        failed: vec![],
                        total_num_cached: 0,
                        first_start_time: test_start_time.clone(),
                        last_stop_time: Some(
                            (report.timestamp.unwrap() + report.time.unwrap())
                                .to_utc()
                                .into(),
                        ),
                        total_run_duration: test_duration,
                        ..Default::default()
                    })),
                },
            ]
        })
        .collect::<Vec<_>>();

    // BEP is JSON streaming, delimited by newlines
    let outputs_contents = build_events
        .iter()
        .map(|l| serde_json::to_string(l).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let file_path = directory.as_ref().join("bep.json");
    let mut file = fs::File::create(&file_path).unwrap();
    file.write_all(outputs_contents.as_bytes()).unwrap();
    file_path
}

pub fn generate_mock_invalid_junit_xmls<T: AsRef<Path>>(directory: T) {
    let mut jm_options = junit_mock::Options::default();
    jm_options.test_suite.test_suite_names = Some(vec!["".to_string()]);
    jm_options.global.timestamp = Utc::now()
        .fixed_offset()
        .checked_sub_signed(TimeDelta::minutes(1));
    let mut jm = JunitMock::new(jm_options);
    let reports = jm.generate_reports();
    jm.write_reports_to_file(directory.as_ref(), reports)
        .unwrap();
}

pub fn generate_mock_suboptimal_junit_xmls<T: AsRef<Path>>(directory: T) {
    let mut jm_options = junit_mock::Options::default();
    jm_options.global.timestamp = Utc::now()
        .fixed_offset()
        .checked_sub_signed(TimeDelta::hours(24));
    let mut jm = JunitMock::new(jm_options);
    let reports = jm.generate_reports();
    jm.write_reports_to_file(directory.as_ref(), reports)
        .unwrap();
}

pub fn generate_mock_missing_filepath_suboptimal_junit_xmls<T: AsRef<Path>>(directory: T) {
    let jm_options = junit_mock::Options::default();
    let mut jm = JunitMock::new(jm_options);
    let mut reports = jm.generate_reports();
    for report in reports.iter_mut() {
        for testsuite in report.test_suites.iter_mut() {
            for test_case in testsuite.test_cases.iter_mut() {
                test_case.extra.swap_remove("file");
            }
        }
    }
    jm.write_reports_to_file(directory.as_ref(), reports)
        .unwrap();
}

pub fn generate_mock_codeowners<T: AsRef<Path>>(directory: T) {
    const CODEOWNERS: &str = r#"
        [Owners of Everything]
        * @user @user2
    "#;
    fs::write(directory.as_ref().join("CODEOWNERS"), CODEOWNERS).unwrap();
}

pub fn write_junit_xml_to_dir<T: AsRef<Path>>(xml: &str, directory: T) {
    let path = directory.as_ref().join("junit-0.xml");
    let mut file = fs::File::create(path).unwrap();
    file.write_all(xml.as_bytes()).unwrap();
}
