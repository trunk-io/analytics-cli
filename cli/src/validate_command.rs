use std::{collections::BTreeMap, io::BufReader};

use bundle::{FileSet, FileSetBuilder};
use clap::{arg, ArgAction, Args};
use codeowners::CodeOwners;
use colored::{ColoredString, Colorize};
use console::Emoji;
use constants::{EXIT_FAILURE, EXIT_SUCCESS};
use context::{
    bazel_bep::parser::BazelBepParser,
    junit::{
        junit_path::JunitReportFileWithStatus,
        parser::{JunitParseIssue, JunitParseIssueLevel, JunitParser},
        validator::{
            validate as validate_report, JunitReportValidation, JunitReportValidationFlatIssue,
            JunitReportValidationIssueSubOptimal, JunitValidationIssue, JunitValidationIssueType,
            JunitValidationLevel,
        },
    },
    repo::BundleRepo,
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
        help = "Comma-separated list of glob paths to junit files.",
    )]
    junit_paths: Vec<String>,
    #[arg(
        long,
        required_unless_present = "junit_paths",
        help = "Path to bazel build event protocol JSON file."
    )]
    bazel_bep_path: Option<String>,
    #[arg(long, help = "Show warning-level log messages in output.", hide = true)]
    show_warnings: bool,
    #[arg(long, help = "Value to override CODEOWNERS file or directory path.")]
    pub codeowners_path: Option<String>,
    #[arg(
        long,
        help = "Hide the top-level flaky tests banner",
        action = ArgAction::Set,
        required = false,
        require_equals = true,
        num_args = 0..=1,
        default_value = "false",
        default_missing_value = "true",
    )]
    pub hide_banner: bool,
}

impl ValidateArgs {
    pub fn hide_banner(&self) -> bool {
        self.hide_banner
    }
}

pub async fn run_validate(validate_args: ValidateArgs) -> anyhow::Result<i32> {
    let ValidateArgs {
        junit_paths,
        bazel_bep_path,
        show_warnings: _,
        codeowners_path,
        hide_banner: _,
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
    validate(junit_file_paths, codeowners_path).await
}

type JunitFileToReportAndParseIssues =
    BTreeMap<String, (anyhow::Result<Option<Report>>, Vec<JunitParseIssue>)>;
type JunitFileToReport = BTreeMap<String, Report>;
type JunitFileToParseIssues = BTreeMap<String, (anyhow::Result<()>, Vec<JunitParseIssue>)>;
type JunitFileToValidation = BTreeMap<String, JunitReportValidation>;

async fn validate(
    junit_paths: Vec<JunitReportFileWithStatus>,
    codeowners_path: Option<String>,
) -> anyhow::Result<i32> {
    // scan files
    let current_dir = std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_default();
    let file_set_builder =
        FileSetBuilder::build_file_sets(&current_dir, &junit_paths, &Option::<&str>::None, None)?;
    if file_set_builder.no_files_found() {
        let msg = "No test output files found to validate.";
        tracing::warn!(msg);
        return Err(anyhow::anyhow!(msg));
    }
    print_matched_files(&file_set_builder);

    // parse
    let parse_results = parse_file_sets(file_set_builder.file_sets());
    let num_reports = parse_results.len();
    let (parsed_reports, parse_issues) = parse_results.into_iter().fold(
        (JunitFileToReport::new(), JunitFileToParseIssues::new()),
        |(mut parsed_reports, mut parse_issues), (file, (parse_result, issues))| {
            match parse_result {
                Ok(report) => match report {
                    Some(report) => {
                        parsed_reports.insert(file, report);
                    }
                    None => {
                        parse_issues.insert(file, (Ok(()), issues));
                    }
                },
                Err(e) => {
                    parse_issues.insert(file, (Err(e), Vec::new()));
                }
            }
            (parsed_reports, parse_issues)
        },
    );
    let repo = BundleRepo::new(None, None, None, None, None, None, false).unwrap_or_default();
    // print parse issues
    let (num_unparsable_reports, num_suboptimally_parsable_reports) =
        print_parse_issues(&parse_issues);

    // validate
    let report_validations: JunitFileToValidation = parsed_reports
        .into_iter()
        .map(|(file, report)| (file, validate_report(&report, &repo)))
        .collect();
    // print validation results
    let (mut num_invalid_reports, mut num_suboptimal_reports) =
        print_validation_issues(&report_validations);

    // print summary
    num_invalid_reports += num_unparsable_reports;
    num_suboptimal_reports += num_suboptimally_parsable_reports;
    let exit = if num_invalid_reports == 0 {
        print_summary_success(num_reports, num_suboptimal_reports);
        EXIT_SUCCESS
    } else {
        print_summary_failure(num_reports, num_invalid_reports, num_suboptimal_reports);
        EXIT_FAILURE
    };

    let codeowners = CodeOwners::find_file(&current_dir, &codeowners_path);

    print_codeowners_validation(codeowners, &report_validations);

    Ok(exit)
}

fn parse_file_sets(file_sets: &[FileSet]) -> JunitFileToReportAndParseIssues {
    file_sets.iter().flat_map(|file_set| &file_set.files).fold(
        JunitFileToReportAndParseIssues::new(),
        |mut parse_results, bundled_file| -> JunitFileToReportAndParseIssues {
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

            let parse_issues = junit_parser.issues().to_vec();
            let mut parsed_reports = junit_parser.into_reports();
            if parsed_reports.len() != 1 {
                parse_results.insert(
                    bundled_file.get_print_path().to_string(),
                    (Ok(None), parse_issues),
                );
                return parse_results;
            }

            parse_results.insert(
                bundled_file.get_print_path().to_string(),
                (Ok(Some(parsed_reports.remove(0))), Vec::new()),
            );

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

fn print_parse_issues(parse_issues: &JunitFileToParseIssues) -> (usize, usize) {
    let mut num_unparsable_reports: usize = 0;
    let mut num_suboptimally_parsable_reports: usize = 0;
    for (i, (file, (parse_result, parse_issues))) in parse_issues.iter().enumerate() {
        if i == 0 {
            println!();
        }

        let (fatal_parse_error, issues, num_parse_errors, num_parse_warnings) =
            if let Err(e) = parse_result {
                (Some(e), &Vec::new(), 1, 0)
            } else {
                let (num_parse_errors, num_parse_warnings) =
                    parse_issues.iter().fold((0, 0), |mut acc, issue| {
                        match JunitParseIssueLevel::from(issue) {
                            JunitParseIssueLevel::Invalid => {
                                acc.0 += 1;
                            }
                            JunitParseIssueLevel::SubOptimal => {
                                acc.1 += 1;
                            }
                            _ => (),
                        }
                        acc
                    });
                (None, parse_issues, num_parse_errors, num_parse_warnings)
            };

        let num_parse_errors_str = if num_parse_errors > 0 {
            num_parse_errors.to_string().red()
        } else {
            num_parse_errors.to_string().green()
        };
        let num_parse_warnings_str = if num_parse_warnings > 0 {
            format!(
                ", {} validation warnings",
                num_parse_warnings.to_string().yellow()
            )
        } else {
            String::from("")
        };
        println!(
            "{} - {} validation errors{}",
            file, num_parse_errors_str, num_parse_warnings_str,
        );

        if let Some(parse_error) = fatal_parse_error {
            println!(
                "  {} - {}",
                print_parse_issue_level(JunitParseIssueLevel::Invalid),
                parse_error,
            );
        }

        for issue in issues {
            println!(
                "  {} - {}",
                print_parse_issue_level(JunitParseIssueLevel::from(issue)),
                issue,
            );
        }

        if num_parse_errors > 0 {
            num_unparsable_reports += 1;
        }
        if num_parse_warnings > 0 {
            num_suboptimally_parsable_reports += 1;
        }
    }

    (num_unparsable_reports, num_suboptimally_parsable_reports)
}

fn print_parse_issue_level(level: JunitParseIssueLevel) -> ColoredString {
    match level {
        JunitParseIssueLevel::SubOptimal => "OPTIONAL".yellow(),
        JunitParseIssueLevel::Invalid => "INVALID".red(),
        JunitParseIssueLevel::Valid => "VALID".green(),
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

fn print_validation_issues(report_validations: &JunitFileToValidation) -> (usize, usize) {
    let mut num_invalid_reports: usize = 0;
    let mut num_suboptimal_reports: usize = 0;
    for (i, (file, report_validation)) in report_validations.iter().enumerate() {
        if i == 0 {
            println!();
        }

        let num_test_suites = report_validation.test_suites().len();
        let num_test_cases = report_validation.test_cases().len();
        let num_validation_errors = report_validation.num_invalid_issues();
        let num_validation_warnings = report_validation.num_suboptimal_issues();
        let all_issues: Vec<JunitReportValidationFlatIssue> = report_validation.all_issues_flat();

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
            file,
            num_test_suites,
            num_test_cases,
            num_validation_errors_str,
            num_validation_warnings_str,
        );

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

    (num_invalid_reports, num_suboptimal_reports)
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
                .flat_map(|(_, report_validation)| report_validation.all_issues())
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
