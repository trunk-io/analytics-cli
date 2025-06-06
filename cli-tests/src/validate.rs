use predicates::prelude::*;
use tempfile::tempdir;

use crate::{
    command_builder::CommandBuilder,
    utils::{
        generate_mock_codeowners, generate_mock_invalid_junit_xmls,
        generate_mock_missing_filepath_suboptimal_junit_xmls, generate_mock_suboptimal_junit_xmls,
        generate_mock_valid_junit_xmls, write_junit_xml_to_dir,
    },
};

#[test]
fn validate_success() {
    let temp_dir = tempdir().unwrap();
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let assert = CommandBuilder::validate(temp_dir.path())
        .command()
        .assert()
        .success()
        .stdout(predicate::str::contains("0 validation errors"))
        .stdout(predicate::str::contains("All 1 files are valid"))
        .stdout(predicate::str::contains("Checking for codeowners file..."))
        .stdout(predicate::str::contains("VALID - Found codeowners:"));

    println!("{assert}");
}

#[test]
fn validate_junit_and_bep() {
    let temp_dir = tempdir().unwrap();

    let assert = CommandBuilder::validate(temp_dir.path())
        .bazel_bep_path("bep.json")
        .command()
        .arg("--junit-paths")
        .arg("./*")
        .assert()
        .failure()
        .stderr(predicate::str::contains("the argument '--bazel-bep-path <BAZEL_BEP_PATH>' cannot be used with '--junit-paths <JUNIT_PATHS>'"));

    println!("{assert}");
}

#[test]
fn validate_no_junits() {
    let temp_dir = tempdir().unwrap();

    let assert = CommandBuilder::validate(temp_dir.path())
        .command()
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "No test output files found to validate",
        ));

    println!("{assert}");
}

#[test]
fn validate_empty_junit_paths() {
    let temp_dir = tempdir().unwrap();

    let assert = CommandBuilder::validate(temp_dir.path())
        .junit_paths("")
        .command()
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "error: a value is required for '--junit-paths <JUNIT_PATHS>' but none was supplied",
        ));

    println!("{assert}");
}

#[test]
fn validate_invalid_junits_no_codeowners() {
    let temp_dir = tempdir().unwrap();
    generate_mock_invalid_junit_xmls(&temp_dir);

    let assert = CommandBuilder::validate(temp_dir.path())
        .command()
        .assert()
        .failure()
        .stdout(predicate::str::contains("1 validation error"))
        .stdout(predicate::str::contains(
            "INVALID - test suite names are missing",
        ))
        .stdout(predicate::str::contains("Checking for codeowners file..."))
        .stdout(predicate::str::contains(
            "OPTIONAL - No codeowners file found",
        ));

    println!("{assert}");
}

#[test]
fn validate_empty_xml() {
    let temp_dir = tempdir().unwrap();
    let empty_xml = "";
    write_junit_xml_to_dir(empty_xml, &temp_dir);

    let assert = CommandBuilder::validate(temp_dir.path())
        .command()
        .assert()
        .success()
        .stdout(predicate::str::contains("1 validation warning"))
        .stdout(predicate::str::contains("OPTIONAL - no reports found"));

    println!("{assert}");
}

#[test]
fn validate_invalid_xml() {
    let temp_dir = tempdir().unwrap();
    let invalid_xml = "<bad<attrs<><><";
    write_junit_xml_to_dir(invalid_xml, &temp_dir);

    let assert = CommandBuilder::validate(temp_dir.path())
        .command()
        .assert()
        .failure()
        .stdout(predicate::str::contains("1 validation error"))
        .stdout(predicate::str::contains(
            "INVALID - syntax error: tag not closed",
        ));

    println!("{assert}");
}

#[test]
fn validate_suboptimal_junits() {
    let temp_dir = tempdir().unwrap();
    generate_mock_suboptimal_junit_xmls(&temp_dir);

    let assert = CommandBuilder::validate(temp_dir.path())
        .command()
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "0 validation errors, 1 validation warning",
        ))
        .stdout(predicate::str::contains(
            "OPTIONAL - report has stale (> 1 hour(s)) timestamps",
        ));

    println!("{assert}");
}

#[test]
fn validate_missing_filepath_suboptimal_junits() {
    let temp_dir = tempdir().unwrap();
    generate_mock_missing_filepath_suboptimal_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let assert = CommandBuilder::validate(temp_dir.path())
        .command()
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "0 validation errors, 2 validation warning",
        ))
        .stdout(predicate::str::contains(
            "OPTIONAL - report has test cases with missing file or filepath",
        ))
        .stdout(predicate::str::contains(
            "OPTIONAL - CODEOWNERS found but test cases are missing filepaths. We will not be able to correlate flaky tests with owners.",
        ));

    println!("{assert}");
}
