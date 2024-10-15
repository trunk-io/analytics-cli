use crate::upload::CARGO_RUN;
use assert_cmd::Command;
use junit_mock::JunitMock;
use std::path::Path;
use tempfile::tempdir;

fn generate_valid_mock_junit_xmls<T: AsRef<Path>>(directory: T) {
    let mut jm = JunitMock::new(junit_mock::Options::default());
    let reports = jm.generate_reports();
    JunitMock::write_reports_to_file(directory.as_ref(), reports).unwrap();
}

fn generate_invalid_mock_junit_xmls<T: AsRef<Path>>(directory: T) {
    let mut jm_options = junit_mock::Options::default();
    jm_options.test_suite.test_suite_names = Some(vec!["".to_string()]);
    let mut jm = JunitMock::new(jm_options);
    let reports = jm.generate_reports();
    JunitMock::write_reports_to_file(directory.as_ref(), reports).unwrap();
}

#[test]
fn validate_success() {
    let temp_dir = tempdir().unwrap();
    generate_valid_mock_junit_xmls(&temp_dir);

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .args(&["validate", "--junit-paths", "./*"])
        .assert()
        .success();

    println!("{assert}");
}

#[test]
fn validate_no_junits() {
    let temp_dir = tempdir().unwrap();

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .args(&["validate", "--junit-paths", "./*"])
        .assert()
        .failure();

    println!("{assert}");
}

#[test]
fn validate_invalid_junits() {
    let temp_dir = tempdir().unwrap();
    generate_invalid_mock_junit_xmls(&temp_dir);

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .args(&["validate", "--junit-paths", "./*"])
        .assert()
        .failure();

    println!("{assert}");
}
