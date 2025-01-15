use std::{collections::BTreeMap, io::BufReader};

use bundle::{FileSet, FileSetBuilder};
use clap::{arg, Args};
use codeowners::CodeOwners;
use colored::{ColoredString, Colorize};
use console::Emoji;
use constants::{EXIT_FAILURE, EXIT_SUCCESS};
use context::{
    bazel_bep::parser::BazelBepParser,
    junit::{
        junit_path::JunitReportFileWithStatus,
        parser::{JunitParseError, JunitParser},
        validator::{
            validate as validate_report, JunitReportValidation, JunitReportValidationFlatIssue,
            JunitReportValidationIssueSubOptimal, JunitValidationIssue, JunitValidationIssueType,
            JunitValidationLevel,
        },
    },
};
use quick_junit::Report;

use crate::print::print_bep_results;

#[derive(Args, Clone, Debug)]
pub struct ValidateArgs {
    #[arg(
        long,
        required_unless_present = "bazel_bep_path",
        conflicts_with = "bazel_bep_path",
        value_delimiter = ',',
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        help = "Comma-separated list of glob paths to junit files."
    )]
    junit_paths: Vec<String>,
    #[arg(
        long,
        required_unless_present = "junit_paths",
        help = "Path to bazel build event protocol JSON file."
    )]
    bazel_bep_path: Option<String>,
    #[arg(long, help = "Show warning-level log messages in output.")]
    show_warnings: bool,
    #[arg(long, help = "Value to override CODEOWNERS file or directory path.")]
    pub codeowners_path: Option<String>,
}

pub async fn run_validate(validate_args: ValidateArgs) -> anyhow::Result<i32> {
    let ValidateArgs {
        junit_paths,
        bazel_bep_path,
        show_warnings,
        codeowners_path,
    } = validate_args;

    let junit_file_paths = match bazel_bep_path {
        Some(bazel_bep_path) => {
            let mut parser = BazelBepParser::new(bazel_bep_path);
            let bep_result = parser.parse()?;
            print_bep_results(&bep_result);
            bep_result.uncached_xml_files()
        }
        None => junit_paths
            .into_iter()
            .map(JunitReportFileWithStatus::from)
            .collect(),
    };
    validate(junit_file_paths, show_warnings, codeowners_path).await
}

type JunitFileToReportAndErrors = BTreeMap<String, (anyhow::Result<Report>, Vec<JunitParseError>)>;
type JunitFileToValidation = BTreeMap<String, anyhow::Result<JunitReportValidation>>;

async fn validate(
    junit_paths: Vec<JunitReportFileWithStatus>,
    show_warnings: bool,
    codeowners_path: Option<String>,
) -> anyhow::Result<i32> {
    // scan files
    let current_dir = std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_default();
    let file_set_builder = FileSetBuilder::build_file_sets(
        &current_dir,
        &junit_paths,
        &None,
        &Option::<&str>::None,
        None,
    )?;
    if file_set_builder.no_files_found() {
        return Err(anyhow::anyhow!("No JUnit files found to validate."));
    }
    print_matched_files(&file_set_builder);

    // parse and validate
    let parse_results = parse_file_sets(file_set_builder.file_sets());
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
    let exit = if num_invalid_reports == 0 {
        print_summary_success(report_validations.len(), num_suboptimal_reports);
        EXIT_SUCCESS
    } else {
        print_summary_failure(
            report_validations.len(),
            num_invalid_reports,
            num_suboptimal_reports,
        );
        EXIT_FAILURE
    };

    let codeowners = CodeOwners::find_file(&current_dir, &codeowners_path);

    print_codeowners_validation(codeowners, &report_validations);

    Ok(exit)
}

fn parse_file_sets(file_sets: &[FileSet]) -> JunitFileToReportAndErrors {
    file_sets.iter().flat_map(|file_set| &file_set.files).fold(
        JunitFileToReportAndErrors::new(),
        |mut parse_results, bundled_file| -> JunitFileToReportAndErrors {
            let path = std::path::Path::new(&bundled_file.original_path);
            let file = match std::fs::File::open(path) {
                Ok(file) => file,
                Err(e) => {
                    parse_results.insert(
                        bundled_file.get_print_path().to_string(),
                        (Err(anyhow::anyhow!(e)), Vec::new()),
                    );
                    return parse_results;
                }
            };

            let file_buf_reader = BufReader::new(file);
            let mut junit_parser = JunitParser::new();
            if let Err(e) = junit_parser.parse(file_buf_reader) {
                parse_results.insert(
                    bundled_file.get_print_path().to_string(),
                    (Err(anyhow::anyhow!(e)), Vec::new()),
                );
                return parse_results;
            }

            let parse_errors = junit_parser.errors().to_vec();
            for report in junit_parser.into_reports() {
                parse_results.insert(
                    bundled_file.get_print_path().to_string(),
                    (Ok(report), parse_errors.clone()),
                );
            }

            parse_results
        },
    )
}

fn print_matched_files(file_set_builder: &FileSetBuilder) {
    println!(
        "\nValidating the following {} files:",
        file_set_builder.count()
    );
    for file_set in file_set_builder.file_sets() {
        println!("  File set matching {}:", file_set.glob);
        for file in &file_set.files {
            println!("    {}", file.get_print_path());
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
                num_test_cases = validation.test_cases().len();

                num_validation_errors = validation.num_invalid_issues();
                num_validation_warnings = validation.num_suboptimal_issues();

                all_issues = validation.all_issues_flat();
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

fn print_codeowners_validation(
    codeowners: Option<CodeOwners>,
    report_validations: &JunitFileToValidation,
) {
    println!("\nChecking for codeowners file...");
    match codeowners {
        Some(owners) => {
            println!(
                "  {} - Found codeowners:",
                print_validation_level(JunitValidationLevel::Valid)
            );
            println!("    Path: {:?}", owners.path);

            let has_test_cases_without_matching_codeowners_paths = report_validations
                .iter()
                .filter_map(|(_, report_validation)| report_validation.as_ref().ok())
                .flat_map(|report_validation| report_validation.all_issues())
                .any(|issue| {
                    matches!(
                        issue,
                        JunitValidationIssueType::Report(JunitValidationIssue::SubOptimal(
                            JunitReportValidationIssueSubOptimal::TestCasesFileOrFilepathMissing
                        ))
                    )
                });

            if has_test_cases_without_matching_codeowners_paths {
                println!(
                    "    {} - CODEOWNERS found but test cases are missing filepaths. We will not be able to correlate flaky tests with owners.",
                    print_validation_level(JunitValidationLevel::SubOptimal)
                );
            }
        }
        None => println!(
            "  {} - No codeowners file found.",
            print_validation_level(JunitValidationLevel::SubOptimal)
        ),
    }
}
