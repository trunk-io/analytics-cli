use crate::utils::{generate_mock_invalid_junit_xmls, generate_mock_valid_junit_xmls, CARGO_RUN};
use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn validate_success() {
    let temp_dir = tempdir().unwrap();
    generate_mock_valid_junit_xmls(&temp_dir);

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
    generate_mock_invalid_junit_xmls(&temp_dir);

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .args(&["validate", "--junit-paths", "./*"])
        .assert()
        .failure();

    println!("{assert}");
}
