use cli_tests::{
    command_builder::CommandBuilder,
    utils::{
        generate_mock_codeowners, generate_mock_invalid_junit_xmls,
        generate_mock_missing_filepath_suboptimal_junit_xmls, generate_mock_suboptimal_junit_xmls,
        generate_mock_valid_junit_xmls, write_junit_xml_to_dir,
    },
};
use predicates::prelude::*;
use superconsole::{
    Line, Span,
    style::{Color, Stylize, style},
};
use tempfile::tempdir;

#[test]
fn validate_success() {
    let temp_dir = tempdir().unwrap();
    generate_mock_valid_junit_xmls(&temp_dir);
    generate_mock_codeowners(&temp_dir);

    let assert = CommandBuilder::validate(temp_dir.path())
        .command()
        .assert()
        .success()
        .stderr(predicate::str::contains(
            Line::from_iter([
                Span::new_styled(style(String::from("0")).with(Color::Red)).unwrap(),
                Span::new_unstyled(String::from(" errors")).unwrap(),
            ])
            .render(),
        ))
        .stderr(predicate::str::contains(
            Line::from_iter([
                Span::new_styled(style(String::from("0")).with(Color::Green)).unwrap(),
                Span::new_unstyled(String::from(" valid files, ")).unwrap(),
                Span::new_styled(style(String::from("1")).with(Color::Yellow)).unwrap(),
                Span::new_unstyled(String::from(" file with warnings, and ")).unwrap(),
                Span::new_styled(style(String::from("0")).with(Color::Red)).unwrap(),
                Span::new_unstyled(String::from(" files with errors, with 1 file total")).unwrap(),
            ])
            .render(),
        ))
        .stderr(predicate::str::contains("Checking for codeowners file..."))
        .stderr(predicate::str::contains("Found codeowners path:"));

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
        .stderr(predicate::str::contains(
            Line::from_iter([
                Span::new_styled(style(String::from("0")).with(Color::Yellow)).unwrap(),
                Span::new_unstyled(String::from(" warnings, and ")).unwrap(),
                Span::new_styled(style(String::from("1")).with(Color::Red)).unwrap(),
                Span::new_unstyled(String::from(" error")).unwrap(),
            ])
            .render(),
        ))
        .stderr(predicate::str::contains("test suite names are missing"))
        .stderr(predicate::str::contains("Checking for codeowners file..."))
        .stderr(predicate::str::contains("No codeowners file found"));

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
        .stderr(predicate::str::contains(
            Line::from_iter([
                Span::new_styled(style(String::from("1")).with(Color::Yellow)).unwrap(),
                Span::new_unstyled(String::from(" warning, and ")).unwrap(),
                Span::new_styled(style(String::from("0")).with(Color::Red)).unwrap(),
                Span::new_unstyled(String::from(" errors")).unwrap(),
            ])
            .render(),
        ))
        .stderr(predicate::str::contains("no reports found"));

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
        .stderr(predicate::str::contains(
            Line::from_iter([
                Span::new_styled(style(String::from("0")).with(Color::Yellow)).unwrap(),
                Span::new_unstyled(String::from(" warnings, and ")).unwrap(),
                Span::new_styled(style(String::from("1")).with(Color::Red)).unwrap(),
                Span::new_unstyled(String::from(" error")).unwrap(),
            ])
            .render(),
        ))
        .stderr(predicate::str::contains("syntax error: tag not closed"));

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
        .stderr(predicate::str::contains(
            Line::from_iter([
                Span::new_styled(style(String::from("1")).with(Color::Yellow)).unwrap(),
                Span::new_unstyled(String::from(" warning, and ")).unwrap(),
                Span::new_styled(style(String::from("0")).with(Color::Red)).unwrap(),
                Span::new_unstyled(String::from(" errors")).unwrap(),
            ])
            .render(),
        ))
        .stderr(predicate::str::contains(
            "report has stale (> 1 hour(s)) timestamps",
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
        .stderr(predicate::str::contains(
            Line::from_iter([
                Span::new_styled(style(String::from("2")).with(Color::Yellow)).unwrap(),
                Span::new_unstyled(String::from(" warnings, and ")).unwrap(),
                Span::new_styled(style(String::from("0")).with(Color::Red)).unwrap(),
                Span::new_unstyled(String::from(" errors")).unwrap(),
            ]).render(),
        ))
        .stderr(predicate::str::contains(
            "report has test cases with missing file or filepath",
        ))
        .stderr(predicate::str::contains(
            "CODEOWNERS found but test cases are missing filepaths. We will not be able to correlate flaky tests with owners.",
        ));

    println!("{assert}");
}
