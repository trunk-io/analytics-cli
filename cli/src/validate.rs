use crate::constants::{EXIT_FAILURE, EXIT_SUCCESS};
use crate::runner::build_filesets;
use crate::scanner::{FileSet, FileSetCounter};
use colored::{ColoredString, Colorize};
use console::Emoji;
use context::junit::parser::{JunitParseError, JunitParser};
use context::junit::validator::{
    validate as validate_report, JunitReportValidation, JunitReportValidationIssue,
    JunitTestCaseValidationIssue, JunitTestSuiteValidationIssue, JunitValidationLevel,
};
use quick_junit::Report;
use std::collections::{BTreeMap, BTreeSet};
use std::io::BufReader;

type JunitFileToReportAndErrors = BTreeMap<String, (anyhow::Result<Report>, Vec<JunitParseError>)>;
type JunitFileToValidation = BTreeMap<String, anyhow::Result<JunitReportValidation>>;

pub async fn validate(junit_paths: Vec<String>, show_warnings: bool) -> anyhow::Result<i32> {
    // scan files
    let current_dir = std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_default();
    let (file_sets, file_counter) = build_filesets(&current_dir, &junit_paths, None, &None, None)?;
    if file_counter.get_count() == 0 || file_sets.is_empty() {
        return Err(anyhow::anyhow!("No JUnit files found to validate."));
    }
    print_matched_files(&file_sets, file_counter);

    // parse and validate
    let parse_results = parse_file_sets(file_sets);
    if show_warnings {
        print_parse_errors(&parse_results);
    }
    let report_validations: JunitFileToValidation = parse_results
        .into_iter()
        .map(|parse_result| {
            return (
                parse_result.0,
                match parse_result.1 .0 {
                    Ok(report) => Ok(validate_report(&report)),
                    Err(e) => Err(e),
                },
            );
        })
        .collect();

    // print results
    let (num_invalid_reports, num_suboptimal_reports) =
        print_validation_errors(&report_validations);
    if num_invalid_reports == 0 {
        print_summary_success(report_validations.len(), num_suboptimal_reports);
        Ok(EXIT_SUCCESS)
    } else {
        print_summary_failure(
            report_validations.len(),
            num_invalid_reports,
            num_suboptimal_reports,
        );
        Ok(EXIT_FAILURE)
    }
}

fn parse_file_sets(file_sets: Vec<FileSet>) -> JunitFileToReportAndErrors {
    file_sets.iter().flat_map(|file_set| &file_set.files).fold(
        JunitFileToReportAndErrors::new(),
        |mut parse_results, bundled_file| -> JunitFileToReportAndErrors {
            let path = std::path::Path::new(&bundled_file.original_path);
            let file = match std::fs::File::open(path) {
                Ok(file) => file,
                Err(e) => {
                    parse_results.insert(
                        bundled_file.original_path_rel.clone(),
                        (Err(anyhow::anyhow!(e)), Vec::new()),
                    );
                    return parse_results;
                }
            };

            let file_buf_reader = BufReader::new(file);
            let mut junit_parser = JunitParser::new();
            match junit_parser.parse(file_buf_reader) {
                Err(e) => {
                    parse_results.insert(
                        bundled_file.original_path_rel.clone(),
                        (Err(anyhow::anyhow!(e)), Vec::new()),
                    );
                    return parse_results;
                }
                _ => (),
            };

            let parse_errors = junit_parser.errors().to_vec();
            for report in junit_parser.into_reports() {
                parse_results.insert(
                    bundled_file.original_path_rel.clone(),
                    (Ok(report), parse_errors.clone()),
                );
            }

            parse_results
        },
    )
}

fn print_matched_files(file_sets: &[FileSet], file_counter: FileSetCounter) {
    log::info!("");
    log::info!(
        "Validating the following {} files:",
        file_counter.get_count()
    );
    for file_set in file_sets {
        log::info!("  File set matching {}:", file_set.glob);
        for file in &file_set.files {
            log::info!("\t{}", file.original_path_rel);
        }
    }
}

fn print_parse_errors(parse_results: &JunitFileToReportAndErrors) {
    let num_parse_errors = parse_results
        .iter()
        .fold(0, |mut num_parse_errors, parse_result| {
            num_parse_errors += parse_result.1 .1.len();
            num_parse_errors
        });

    if num_parse_errors == 0 {
        return;
    }

    log::info!("");
    log::warn!(
        "Encountered the following {} non-fatal errors while parsing files:",
        num_parse_errors.to_string().yellow()
    );

    for parse_result in parse_results {
        if parse_result.1 .1.is_empty() {
            continue;
        }

        log::warn!("  File: {}", parse_result.0);

        for parse_error in &parse_result.1 .1 {
            log::warn!("\t{}", parse_error);
        }
    }
}

fn print_summary_failure(
    num_reports: usize,
    num_invalid_reports: usize,
    num_suboptimal_reports: usize,
) {
    log::info!("");
    let num_validation_warnings_str = if num_suboptimal_reports > 0 {
        format!(
            ", {} files have validation warnings",
            num_suboptimal_reports.to_string().yellow()
        )
    } else {
        String::from("")
    };
    log::info!(
        "{} files are valid, {} files are not valid{}{}",
        (num_reports - num_invalid_reports).to_string().green(),
        num_invalid_reports.to_string().red(),
        num_validation_warnings_str,
        Emoji(" ❌", ""),
    );
}

fn print_summary_success(num_reports: usize, num_suboptimal_reports: usize) {
    log::info!("");
    let num_validation_warnings_str = if num_suboptimal_reports > 0 {
        format!(
            " ({} files with validation warnings)",
            num_suboptimal_reports.to_string().yellow()
        )
    } else {
        String::from("")
    };

    log::info!(
        "All {} files are valid!{}{}",
        num_reports.to_string().green(),
        num_validation_warnings_str,
        Emoji(" ✅", ""),
    );
    log::info!(
        "First time setting up Flaky Tests for this repo? Follow this link <link> to continue getting started.{}",
        Emoji(" 🚀🧪", ""),
    );
}

fn print_validation_errors(report_validations: &JunitFileToValidation) -> (usize, usize) {
    log::info!("");
    let mut num_invalid_reports: usize = 0;
    let mut num_suboptimal_reports: usize = 0;
    for report_validation in report_validations {
        let mut num_test_suites = 0;
        let mut num_test_cases = 0;
        let num_validation_errors: usize;
        let mut num_validation_warnings = 0;
        let mut report_parse_error: Option<&anyhow::Error> = None;
        let mut report_validation_issues: &BTreeSet<JunitReportValidationIssue> = &BTreeSet::new();
        let mut test_suite_validation_issues: Vec<&JunitTestSuiteValidationIssue> = Vec::new();
        let mut test_case_validation_issues: Vec<&JunitTestCaseValidationIssue> = Vec::new();

        match report_validation.1 {
            Ok(validation) => {
                num_test_suites = validation.test_suites().len();
                num_test_cases = validation.test_cases_flat().len();

                let num_report_validation_errors =
                    validation.report_invalid_validation_issues().len();
                let num_test_suite_validation_errors =
                    validation.test_suite_invalid_validation_issues_flat().len();
                let num_test_case_validation_errors =
                    validation.test_case_invalid_validation_issues_flat().len();
                num_validation_errors = num_report_validation_errors
                    + num_test_suite_validation_errors
                    + num_test_case_validation_errors;

                let num_report_validation_warnings =
                    validation.report_suboptimal_validation_issues().len();
                let num_test_suite_validation_warnings = validation
                    .test_suite_suboptimal_validation_issues_flat()
                    .len();
                let num_test_case_validation_warnings = validation
                    .test_case_suboptimal_validation_issues_flat()
                    .len();
                num_validation_warnings = num_report_validation_warnings
                    + num_test_suite_validation_warnings
                    + num_test_case_validation_warnings;

                report_validation_issues = validation.report_validation_issues();
                test_suite_validation_issues = validation.test_suite_validation_issues_flat();
                test_case_validation_issues = validation.test_case_validation_issues_flat();
            }
            Err(e) => {
                report_parse_error = Some(e);
                num_validation_errors = 1;
            }
        }

        let num_validation_errors_str = if num_validation_errors > 0 {
            num_validation_errors.to_string().red()
        } else {
            num_validation_errors.to_string().green()
        };
        let num_validation_warnings_str = if num_validation_warnings > 0 {
            format!(
                ", {} validation warnings",
                num_validation_warnings.to_string().yellow()
            )
        } else {
            String::from("")
        };
        log::info!(
            "{} - {} test suites, {} test cases, {} validation errors{}",
            report_validation.0,
            num_test_suites,
            num_test_cases,
            num_validation_errors_str,
            num_validation_warnings_str,
        );

        match report_parse_error {
            Some(parse_error) => {
                log::info!(
                    "  {} - {}",
                    print_validation_level(JunitValidationLevel::Invalid),
                    parse_error,
                );
            }
            _ => (),
        }

        for report_validation_error in report_validation_issues {
            log::info!(
                "  {} - {}",
                print_validation_level(JunitValidationLevel::from(report_validation_error)),
                report_validation_error.to_string(),
            );
        }

        for test_suite_validation_error in test_suite_validation_issues {
            log::info!(
                "  {} - {}",
                print_validation_level(JunitValidationLevel::from(test_suite_validation_error)),
                test_suite_validation_error.to_string(),
            );
        }

        for test_case_validation_error in test_case_validation_issues {
            log::info!(
                "  {} - {}",
                print_validation_level(JunitValidationLevel::from(test_case_validation_error)),
                test_case_validation_error.to_string(),
            );
        }

        if num_validation_errors > 0 {
            num_invalid_reports += 1;
        }
        if num_validation_warnings > 0 {
            num_suboptimal_reports += 1;
        }
    }

    return (num_invalid_reports, num_suboptimal_reports);
}

fn print_validation_level(level: JunitValidationLevel) -> ColoredString {
    match level {
        JunitValidationLevel::SubOptimal => "OPTIONAL".yellow(),
        JunitValidationLevel::Invalid => "INVALID".red(),
        JunitValidationLevel::Valid => "VALID".green(),
    }
}
