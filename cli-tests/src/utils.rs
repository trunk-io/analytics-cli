use bazel_bep::types::build_event_stream::{
    build_event::Payload,
    build_event_id::{Id, TestResultId},
    file::File::Uri,
    BuildEvent, BuildEventId, File, TestResult,
};
use chrono::{TimeDelta, Utc};
use escargot::{CargoBuild, CargoRun};
use junit_mock::JunitMock;
use lazy_static::lazy_static;
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};
use test_utils::mock_git_repo::setup_repo_with_commit;

lazy_static! {
    static ref CARGO_MANIFEST_DIR: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    pub static ref CARGO_RUN: CargoRun = CargoBuild::new()
        .bin("trunk-analytics-cli")
        .target_dir(CARGO_MANIFEST_DIR.join("../target"))
        .manifest_path(CARGO_MANIFEST_DIR.join("../cli/Cargo.toml"))
        .features("force-sentry-env-dev")
        .current_release()
        .current_target()
        .run()
        .unwrap();
}

pub fn generate_mock_git_repo<T: AsRef<Path>>(directory: T) {
    setup_repo_with_commit(directory).unwrap();
}

pub fn generate_mock_valid_junit_xmls<T: AsRef<Path>>(directory: T) -> Vec<PathBuf> {
    let mut jm_options = junit_mock::Options::default();
    jm_options.global.timestamp = Utc::now()
        .fixed_offset()
        .checked_sub_signed(TimeDelta::minutes(1));
    let mut jm = JunitMock::new(junit_mock::Options::default());
    let reports = jm.generate_reports();
    jm.write_reports_to_file(directory.as_ref(), reports)
        .unwrap()
}

pub fn generate_mock_bazel_bep<T: AsRef<Path>>(directory: T) {
    let mock_junits = generate_mock_valid_junit_xmls(&directory);

    // TODO: TYLER SHOULD WE MAKE TESTSUMMARY EVENTS TOO? DONOTLAND
    let build_events: Vec<BuildEvent> = mock_junits
        .iter()
        .map(|junit| {
            let mut build_event = BuildEvent::default();
            let mut payload = TestResult::default();
            payload.test_action_output = vec![File {
                name: junit.file_name().unwrap().to_str().unwrap().to_string(),
                file: Some(Uri(junit.to_string_lossy().to_string())),
                ..Default::default()
            }];
            build_event.payload = Some(Payload::TestResult(payload));
            build_event.id = Some(BuildEventId {
                id: Some(Id::TestResult(TestResultId {
                    label: "//path:test".to_string(),
                    ..Default::default()
                })),
            });
            build_event
        })
        .collect();

    // bep JSON is a list of new-line separated JSON objects
    let outputs_contents = build_events
        .iter()
        .map(|be| serde_json::to_string(be).unwrap())
        .collect::<Vec<String>>()
        .join("\n");
    let mut file = fs::File::create(&directory.as_ref().join("bep.json")).unwrap();
    file.write_all(outputs_contents.as_bytes()).unwrap();
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
        * @user
    "#;
    fs::write(directory.as_ref().join("CODEOWNERS"), CODEOWNERS).unwrap();
}

pub fn write_junit_xml_to_dir<T: AsRef<Path>>(xml: &str, directory: T) {
    let path = directory.as_ref().join("junit-0.xml");
    let mut file = fs::File::create(&path).unwrap();
    file.write_all(xml.as_bytes()).unwrap();
}
