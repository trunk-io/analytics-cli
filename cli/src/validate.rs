use crate::constants::{EXIT_FAILURE, EXIT_SUCCESS};
use crate::runner::build_filesets;
use crate::scanner::{FileSet, FileSetCounter};
use anyhow::Context;
use colored::{ColoredString, Colorize};
use context::junit::parser::{JunitParseError, JunitParser};
use context::junit::validator::{
    validate as validate_report, JunitReportValidation, JunitValidationLevel,
};
use quick_junit::Report;
use std::io::BufReader;

pub fn validate(junit_paths: Vec<String>, show_warnings: bool) -> anyhow::Result<i32> {
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
    let (reports, parse_errors) = parse_file_sets(file_sets)?;
    if !parse_errors.is_empty() && show_warnings {
        print_parse_errors(parse_errors);
    }
    let report_validations: Vec<(JunitReportValidation, String)> = reports
        .into_iter()
        .map(|report| (validate_report(&report.0), report.1))
        .collect();

    // print results
    let (num_invalid_reports, num_optionally_invalid_reports) =
        print_validation_errors(&report_validations);
    if num_invalid_reports == 0 {
        print_summary_success(report_validations.len(), num_optionally_invalid_reports);
        Ok(EXIT_SUCCESS)
    } else {
        print_summary_failure(
            report_validations.len(),
            num_invalid_reports,
            num_optionally_invalid_reports,
        );
        Ok(EXIT_FAILURE)
    }
}

fn parse_file_sets(
    file_sets: Vec<FileSet>,
) -> anyhow::Result<(Vec<(Report, String)>, Vec<(JunitParseError, String)>)> {
    file_sets.iter().try_fold(
        (
            Vec::<(Report, String)>::new(),          // Vec<(Report, file path)>
            Vec::<(JunitParseError, String)>::new(), // Vec<(JunitParseError, file path)>
        ),
        |mut file_sets_parse_results, file_set| {
            let file_set_parse_results = file_set.files.iter().try_fold(
                (
                    Vec::<(Report, String)>::new(),
                    Vec::<(JunitParseError, String)>::new(),
                ),
                |mut file_set_parse_results, bundled_file| {
                    let path = std::path::Path::new(&bundled_file.original_path);
                    let file = std::fs::File::open(path)?;
                    let file_buf_reader = BufReader::new(file);
                    let mut junit_parser = JunitParser::new();
                    junit_parser.parse(file_buf_reader).context(format!(
                        "Encountered unrecoverable error while parsing file: {}",
                        bundled_file.original_path_rel
                    ))?;
                    file_set_parse_results.1.extend(
                        junit_parser
                            .errors()
                            .iter()
                            .map(|e| (*e, bundled_file.original_path_rel.clone())),
                    );
                    file_set_parse_results.0.extend(
                        junit_parser
                            .into_reports()
                            .iter()
                            .map(|report| (report.clone(), bundled_file.original_path_rel.clone())),
                    );
                    Ok::<(Vec<(Report, String)>, Vec<(JunitParseError, String)>), anyhow::Error>(
                        file_set_parse_results,
                    )
                },
            )?;
            file_sets_parse_results.0.extend(file_set_parse_results.0);
            file_sets_parse_results.1.extend(file_set_parse_results.1);
            Ok::<(Vec<(Report, String)>, Vec<(JunitParseError, String)>), anyhow::Error>(
                file_sets_parse_results,
            )
        },
    )
}

fn print_matched_files(file_sets: &[FileSet], file_counter: FileSetCounter) {
    log::info!("");
    log::info!(
        "Validating the following {} files matching the provided globs:",
        file_counter.get_count()
    );
    for file_set in file_sets {
        log::info!(
            "  File set ({:?}): {}",
            file_set.file_set_type,
            file_set.glob
        );
        for file in &file_set.files {
            log::info!("\t{}", file.original_path_rel);
        }
    }
}

fn print_parse_errors(parse_errors: Vec<(JunitParseError, String)>) {
    log::info!("");
    log::warn!(
        "Encountered the following {} non-fatal errors while parsing files:",
        parse_errors.len().to_string().yellow()
    );

    let mut current_file_original_path = parse_errors[0].1.clone();
    log::warn!("  File: {}", current_file_original_path);

    for error in parse_errors {
        if error.1 != current_file_original_path {
            current_file_original_path = error.1;
            log::warn!("  File: {}", current_file_original_path);
        }

        log::warn!("\t{}", error.0);
    }
}

fn print_summary_failure(
    num_reports: usize,
    num_invalid_reports: usize,
    num_optionally_invalid_reports: usize,
) {
    log::info!("");
    let num_optional_validation_errors_str = if num_optionally_invalid_reports > 0 {
        format!(
            ", {} files have optional validation errors",
            num_optionally_invalid_reports.to_string().yellow()
        )
    } else {
        String::from("")
    };
    log::info!(
        "{} files are valid, {} files are not valid{}",
        (num_reports - num_invalid_reports).to_string().green(),
        num_invalid_reports.to_string().red(),
        num_optional_validation_errors_str,
    );
}

fn print_summary_success(num_reports: usize, num_optionally_invalid_reports: usize) {
    log::info!("");
    let num_optional_validation_errors_str = if num_optionally_invalid_reports > 0 {
        format!(
            " ({} files with optional validation errors)",
            num_optionally_invalid_reports.to_string().yellow()
        )
    } else {
        String::from("")
    };

    log::info!(
        "All {} files are valid!{}",
        num_reports.to_string().green(),
        num_optional_validation_errors_str
    );
    log::info!(
        "Navigate to <URL for next onboarding step> to continue getting started with Flaky Tests"
    );
}

fn print_validation_errors(
    report_validations: &[(JunitReportValidation, String)],
) -> (usize, usize) {
    log::info!("");
    let mut num_invalid_reports: usize = 0;
    let mut num_optionally_invalid_reports: usize = 0;
    for report_validation in report_validations {
        let num_test_cases = report_validation.0.test_cases_flat().len();
        let num_invalid_validation_errors = report_validation
            .0
            .test_suite_invalid_validation_issues_flat()
            .len()
            + report_validation
                .0
                .test_case_invalid_validation_issues_flat()
                .len();
        let num_optional_validation_errors = report_validation
            .0
            .test_suite_suboptimal_validation_issues_flat()
            .len()
            + report_validation
                .0
                .test_case_suboptimal_validation_issues_flat()
                .len();

        let num_validation_errors_str = if num_invalid_validation_errors > 0 {
            num_invalid_validation_errors.to_string().red()
        } else {
            num_invalid_validation_errors.to_string().green()
        };
        let num_optional_validation_errors_str = if num_optional_validation_errors > 0 {
            format!(
                ", {} optional validation errors",
                num_optional_validation_errors.to_string().yellow()
            )
        } else {
            String::from("")
        };
        log::info!(
            "{} - {} test suites, {} test cases, {} validation errors{}",
            report_validation.1,
            report_validation.0.test_suites().len(),
            num_test_cases,
            num_validation_errors_str,
            num_optional_validation_errors_str,
        );

        for test_suite_validation_error in report_validation.0.test_suite_validation_issues_flat() {
            log::info!(
                "  {} - {}",
                print_validation_level(JunitValidationLevel::from(test_suite_validation_error)),
                test_suite_validation_error.to_string(),
            );
        }

        for test_case_validation_error in report_validation.0.test_case_validation_issues_flat() {
            log::info!(
                "  {} - {}",
                print_validation_level(JunitValidationLevel::from(test_case_validation_error)),
                test_case_validation_error.to_string(),
            );
        }

        if num_invalid_validation_errors > 0 {
            num_invalid_reports += 1;
        }
        if num_optional_validation_errors > 0 {
            num_optionally_invalid_reports += 1;
        }
    }

    return (num_invalid_reports, num_optionally_invalid_reports);
}

fn print_validation_level(level: JunitValidationLevel) -> ColoredString {
    match level {
        JunitValidationLevel::SubOptimal => "OPTIONAL".yellow(),
        JunitValidationLevel::Invalid => "INVALID".red(),
        JunitValidationLevel::Valid => "VALID".green(),
    }
}
