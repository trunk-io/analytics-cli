use chrono::{TimeDelta, Utc};
use escargot::{CargoBuild, CargoRun};
use junit_mock::JunitMock;
use lazy_static::lazy_static;
use std::{
    env, fs,
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

pub fn generate_mock_valid_junit_xmls<T: AsRef<Path>>(directory: T) {
    let mut jm_options = junit_mock::Options::default();
    jm_options.global.timestamp = Utc::now()
        .fixed_offset()
        .checked_sub_signed(TimeDelta::minutes(1));
    let mut jm = JunitMock::new(junit_mock::Options::default());
    let reports = jm.generate_reports();
    JunitMock::write_reports_to_file(directory.as_ref(), reports).unwrap();
}

pub fn generate_mock_invalid_junit_xmls<T: AsRef<Path>>(directory: T) {
    let mut jm_options = junit_mock::Options::default();
    jm_options.test_suite.test_suite_names = Some(vec!["".to_string()]);
    jm_options.global.timestamp = Utc::now()
        .fixed_offset()
        .checked_sub_signed(TimeDelta::minutes(1));
    let mut jm = JunitMock::new(jm_options);
    let reports = jm.generate_reports();
    JunitMock::write_reports_to_file(directory.as_ref(), reports).unwrap();
}

pub fn generate_mock_suboptimal_junit_xmls<T: AsRef<Path>>(directory: T) {
    let mut jm_options = junit_mock::Options::default();
    jm_options.global.timestamp = Utc::now()
        .fixed_offset()
        .checked_sub_signed(TimeDelta::hours(24));
    let mut jm = JunitMock::new(jm_options);
    let reports = jm.generate_reports();
    JunitMock::write_reports_to_file(directory.as_ref(), reports).unwrap();
}

pub fn generate_mock_codeowners<T: AsRef<Path>>(directory: T) {
    const CODEOWNERS: &str = r#"
        [Owners of Everything]
        * @user
    "#;
    fs::write(directory.as_ref().join("CODEOWNERS"), CODEOWNERS).unwrap();
}
