use crate::constants::{EXIT_FAILURE, EXIT_SUCCESS};
use crate::runner::build_filesets;
use crate::scanner::{FileSet, FileSetCounter};
use anyhow::Context;
use colored::{ColoredString, Colorize};
use console::Emoji;
use context::junit::parser::{JunitParseError, JunitParser};
use context::junit::validator::{
    validate as validate_report, JunitReportValidation, JunitValidationLevel,
};
use quick_junit::Report;
use std::collections::BTreeMap;
use std::io::BufReader;

type JunitFileToReportAndErrors = BTreeMap<String, (Vec<Report>, Vec<JunitParseError>)>;
type JunitReportValidationsAndFilePaths = Vec<(JunitReportValidation, String)>;

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
    let parse_results = parse_file_sets(file_sets)?;
    if show_warnings {
        print_parse_errors(&parse_results);
    }
    let report_validations: JunitReportValidationsAndFilePaths = parse_results.into_iter().fold(
        JunitReportValidationsAndFilePaths::new(),
        |mut report_validations, parse_result| {
            report_validations.extend(
                parse_result
                    .1
                     .0
                    .iter()
                    .map(|report| (validate_report(report), parse_result.0.clone())),
            );
            report_validations
        },
    );

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

fn parse_file_sets(file_sets: Vec<FileSet>) -> anyhow::Result<JunitFileToReportAndErrors> {
    file_sets.iter().try_fold(
        JunitFileToReportAndErrors::new(),
        |mut file_sets_parse_results, file_set| -> anyhow::Result<JunitFileToReportAndErrors> {
            let file_set_parse_results = file_set.files.iter().try_fold(
                JunitFileToReportAndErrors::new(),
                |mut file_set_parse_results,
                 bundled_file|
                 -> anyhow::Result<JunitFileToReportAndErrors> {
                    let path = std::path::Path::new(&bundled_file.original_path);
                    let file = std::fs::File::open(path)?;
                    let file_buf_reader = BufReader::new(file);
                    let mut junit_parser = JunitParser::new();
                    junit_parser.parse(file_buf_reader).context(format!(
                        "Encountered unrecoverable error while parsing file: {}",
                        bundled_file.original_path_rel
                    ))?;

                    let mut cur_file_parse_results = file_set_parse_results
                        .get(&bundled_file.original_path_rel)
                        .cloned()
                        .unwrap_or((Vec::new(), Vec::new()));

                    cur_file_parse_results.1.extend(junit_parser.errors());
                    cur_file_parse_results.0.extend(junit_parser.into_reports());

                    file_set_parse_results.insert(
                        bundled_file.original_path_rel.clone(),
                        cur_file_parse_results,
                    );

                    Ok(file_set_parse_results)
                },
            )?;

            file_sets_parse_results.extend(file_set_parse_results);

            Ok(file_sets_parse_results)
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

fn print_validation_errors(
    report_validations: &JunitReportValidationsAndFilePaths,
) -> (usize, usize) {
    log::info!("");
    let mut num_invalid_reports: usize = 0;
    let mut num_suboptimal_reports: usize = 0;
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
        let num_validation_warnings = report_validation
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
            report_validation.1,
            report_validation.0.test_suites().len(),
            num_test_cases,
            num_validation_errors_str,
            num_validation_warnings_str,
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
