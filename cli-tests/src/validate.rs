use crate::utils::{
    generate_mock_invalid_junit_xmls, generate_mock_suboptimal_junit_xmls,
    generate_mock_valid_junit_xmls, CARGO_RUN,
};
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn validate_success() {
    let temp_dir = tempdir().unwrap();
    generate_mock_valid_junit_xmls(&temp_dir);

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .args(&["validate", "--junit-paths", "./*"])
        .assert()
        .success()
        .stderr(predicate::str::contains("0 validation errors"))
        .stderr(predicate::str::contains("All 1 files are valid"));

    println!("{assert}");
}

#[test]
fn validate_no_junits() {
    let temp_dir = tempdir().unwrap();

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .args(&["validate", "--junit-paths", "./*"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("No JUnit files found to validate"));

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
        .failure()
        .stderr(predicate::str::contains("1 validation error"))
        .stderr(predicate::str::contains(
            "INVALID - test suite name too short",
        ));

    println!("{assert}");
}

#[test]
fn validate_suboptimal_junits() {
    let temp_dir = tempdir().unwrap();
    generate_mock_suboptimal_junit_xmls(&temp_dir);

    let assert = Command::new(CARGO_RUN.path())
        .current_dir(&temp_dir)
        .args(&["validate", "--junit-paths", "./*"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "0 validation errors, 1 validation warning",
        ))
        .stderr(predicate::str::contains(
            "OPTIONAL - report has stale (> 1 hour(s)) timestamps",
        ));

    println!("{assert}");
}
