use crate::constants::{EXIT_FAILURE, EXIT_SUCCESS};
use crate::runner::build_filesets;
use crate::scanner::{FileSet, FileSetCounter};
use colored::{ColoredString, Colorize};
use console::Emoji;
use context::junit::parser::{JunitParseError, JunitParser};
use context::junit::validator::{
    validate as validate_report, JunitReportValidation, JunitReportValidationFlatIssue,
    JunitValidationLevel,
};
use quick_junit::Report;
use std::collections::BTreeMap;
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
            if let Err(e) = junit_parser.parse(file_buf_reader) {
                parse_results.insert(
                    bundled_file.original_path_rel.clone(),
                    (Err(anyhow::anyhow!(e)), Vec::new()),
                );
                return parse_results;
            }

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
    println!(
        "\nValidating the following {} files:",
        file_counter.get_count()
    );
    for file_set in file_sets {
        println!("  File set matching {}:", file_set.glob);
        for file in &file_set.files {
            println!("    {}", file.original_path_rel);
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

    println!(
        "\nEncountered the following {} non-fatal errors while parsing files:",
        num_parse_errors.to_string().yellow()
    );

    for parse_result in parse_results {
        if parse_result.1 .1.is_empty() {
            continue;
        }

        println!("  File: {}", parse_result.0);

        for parse_error in &parse_result.1 .1 {
            println!("    {}", parse_error);
        }
    }
}

fn print_summary_failure(
    num_reports: usize,
    num_invalid_reports: usize,
    num_suboptimal_reports: usize,
) {
    let num_validation_warnings_str = if num_suboptimal_reports > 0 {
        format!(
            ", {} files have validation warnings",
            num_suboptimal_reports.to_string().yellow()
        )
    } else {
        String::from("")
    };
    println!(
        "\n{} files are valid, {} files are not valid{}{}",
        (num_reports - num_invalid_reports).to_string().green(),
        num_invalid_reports.to_string().red(),
        num_validation_warnings_str,
        Emoji(" ❌", ""),
    );
}

fn print_summary_success(num_reports: usize, num_suboptimal_reports: usize) {
    let num_validation_warnings_str = if num_suboptimal_reports > 0 {
        format!(
            " ({} files with validation warnings)",
            num_suboptimal_reports.to_string().yellow()
        )
    } else {
        String::from("")
    };

    println!(
        "\nAll {} files are valid!{}{}",
        num_reports.to_string().green(),
        num_validation_warnings_str,
        Emoji(" ✅", ""),
    );
}

fn print_validation_errors(report_validations: &JunitFileToValidation) -> (usize, usize) {
    println!();
    let mut num_invalid_reports: usize = 0;
    let mut num_suboptimal_reports: usize = 0;
    for report_validation in report_validations {
        let mut num_test_suites = 0;
        let mut num_test_cases = 0;
        let num_validation_errors: usize;
        let mut num_validation_warnings = 0;
        let mut report_parse_error: Option<&anyhow::Error> = None;
        let mut all_issues: Vec<JunitReportValidationFlatIssue> = Vec::new();

        match report_validation.1 {
            Ok(validation) => {
                num_test_suites = validation.test_suites().len();
                num_test_cases = validation.test_cases_flat().len();

                num_validation_errors = validation.num_invalid_issues();
                num_validation_warnings = validation.num_suboptimal_issues();

                all_issues = validation.all_issues_owned();
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
        println!(
            "{} - {} test suites, {} test cases, {} validation errors{}",
            report_validation.0,
            num_test_suites,
            num_test_cases,
            num_validation_errors_str,
            num_validation_warnings_str,
        );

        if let Some(parse_error) = report_parse_error {
            println!(
                "  {} - {}",
                print_validation_level(JunitValidationLevel::Invalid),
                parse_error,
            );
        }

        for issue in all_issues {
            println!(
                "  {} - {}",
                print_validation_level(issue.level),
                issue.error_message,
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
