use std::{
    collections::{BTreeMap, HashSet},
    io::BufReader,
    path::PathBuf,
};

use bundle::{FileSet, FileSetBuilder, FileSetTestRunnerReport};
use clap::{arg, ArgAction, Args};
use codeowners::CodeOwners;
use constants::{EXIT_FAILURE, EXIT_SUCCESS};
use context::{
    bazel_bep::{common::BepParseResult, parser::BazelBepParser},
    junit::{
        junit_path::{JunitReportFileWithTestRunnerReport, TestRunnerReport},
        parser::{JunitParseIssue, JunitParseIssueLevel, JunitParser},
        validator::{
            validate as validate_report, JunitReportValidation, JunitReportValidationFlatIssue,
            JunitReportValidationIssueSubOptimal, JunitValidationIssue, JunitValidationIssueType,
            JunitValidationLevel,
        },
    },
};
use display::end_output::EndOutput;
use pluralizer::pluralize;
use quick_junit::Report;
use superconsole::{
    style::{style, Attribute, Color, Stylize},
    Line, Lines, Span,
};

use crate::{context::fall_back_to_binary_parse, report_limiting::ValidationReport};

#[derive(Args, Clone, Debug)]
pub struct ValidateArgs {
    #[arg(
        long,
        required_unless_present_any = ["bazel_bep_path", "test_reports"],
        conflicts_with_all = ["bazel_bep_path", "test_reports"],
        value_delimiter = ',',
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        help = "Comma-separated list of glob paths to junit files.",
    )]
    junit_paths: Vec<String>,
    #[arg(
        long,
        required_unless_present_any = ["junit_paths", "test_reports"],
        help = "Path to bazel build event protocol JSON file."
    )]
    bazel_bep_path: Option<String>,
    #[arg(
        long,
        required_unless_present_any = ["junit_paths", "bazel_bep_path"],
        value_delimiter = ',',
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        help = "Comma-separated list of paths to test report files."
    )]
    pub test_reports: Vec<String>,
    #[arg(long, help = "Show warning-level log messages in output.", hide = true)]
    show_warnings: bool,
    #[arg(long, help = "Value to override CODEOWNERS file or directory path.")]
    pub codeowners_path: Option<String>,
    #[arg(
        long,
        help = "Deprecated (does nothing, left in to avoid breaking existing flows)",
        action = ArgAction::Set,
        required = false,
        require_equals = true,
        num_args = 0..=1,
        default_value = "false",
        default_missing_value = "true",
        hide = true,
    )]
    pub hide_banner: bool,
}

#[derive(Debug)]
pub struct BepValidateResult {
    errors: Vec<String>,
}

#[derive(Debug)]
pub struct FileSetResult {
    glob: String,
    file_paths: Vec<String>,
}

#[derive(Debug)]
pub struct ParseIssues {
    file_path: String,
    fatal_error: Option<anyhow::Error>,
    errors: Vec<JunitParseIssue>,
    warnings: Vec<JunitParseIssue>,
}

#[derive(Debug)]
pub struct TestIssues {
    file_path: String,
    num_test_suites: usize,
    num_test_cases: usize,
    errors: Vec<JunitReportValidationFlatIssue>,
    warnings: Vec<JunitReportValidationFlatIssue>,
}

#[derive(Debug)]
pub struct CodeownersIssues {
    file_path: PathBuf,
    warnings: Vec<String>,
}

#[derive(Debug)]
pub struct ValidateRunResult {
    bep_result: Option<BepValidateResult>,
    file_sets: Vec<FileSetResult>,
    file_parse_issues: Vec<ParseIssues>,
    test_issues: Vec<TestIssues>,
    codeowners_issues: Option<CodeownersIssues>,
}

impl ValidateRunResult {
    pub fn exit_code(&self) -> i32 {
        let has_errors = self.file_parse_issues.iter().any(|file_parse_issue| {
            file_parse_issue.fatal_error.is_some() || !file_parse_issue.errors.is_empty()
        }) || self
            .test_issues
            .iter()
            .any(|test_issue| !test_issue.errors.is_empty());
        if has_errors {
            EXIT_FAILURE
        } else {
            EXIT_SUCCESS
        }
    }

    fn num_files(&self) -> usize {
        let mut num = 0;
        for file_set in self.file_sets.iter() {
            num += file_set.file_paths.len();
        }
        num
    }
}

impl EndOutput for ValidateRunResult {
    fn output(&self) -> anyhow::Result<Vec<Line>> {
        let mut output: Vec<Line> = Vec::new();

        if let Some(bep_results) = &self.bep_result {
            if !bep_results.errors.is_empty() {
                output.push(Line::from_iter([
                    Span::new_unstyled("⚠️  BEP file had parse errors: ")?,
                    Span::new_unstyled(format!("{:?}", bep_results.errors))?,
                ]));
                output.push(Line::default());
            }
        }

        output.push(Line::from_iter([Span::new_unstyled(format!(
            "📚 Validating {} files:",
            self.num_files()
        ))?]));
        for file_set in self.file_sets.iter() {
            output.push(Line::from_iter([Span::new_unstyled(format!(
                "  Files matching: {}:",
                file_set.glob
            ))?]));
            for file_path in file_set.file_paths.iter() {
                output.push(Line::from_iter([Span::new_unstyled(format!(
                    "    {}",
                    file_path
                ))?]))
            }
            output.push(Line::default());
        }

        output.push(Line::from_iter([Span::new_styled(
            style(String::from("File parse issues")).attribute(Attribute::Bold),
        )?]));
        for file_parse_issue in self.file_parse_issues.iter() {
            let has_errors =
                file_parse_issue.fatal_error.is_some() || !file_parse_issue.errors.is_empty();
            let has_warnings = !file_parse_issue.warnings.is_empty();

            let title_colour = if has_errors {
                Color::Red
            } else if has_warnings {
                Color::Yellow
            } else {
                Color::Green
            };
            output.push(Line::from_iter([Span::new_styled(
                style(file_parse_issue.file_path.clone()).with(title_colour),
            )?]));

            let num_errors = if file_parse_issue.fatal_error.is_some() {
                file_parse_issue.errors.len() + 1
            } else {
                file_parse_issue.errors.len()
            };
            output.push(Line::from_iter([Span::new_styled(
                style(format!(
                    "{} errors, {} warnings",
                    num_errors,
                    file_parse_issue.warnings.len()
                ))
                .attribute(Attribute::Italic),
            )?]));

            if has_errors {
                output.push(Line::from_iter([Span::new_unstyled(" ❌ Errors:")?]));
                if let Some(e) = &file_parse_issue.fatal_error {
                    output.push(Line::from_iter([Span::new_unstyled_lossy(format!(
                        "  {:?}",
                        e
                    ))]));
                }
                for error in file_parse_issue.errors.iter() {
                    output.push(Line::from_iter([Span::new_unstyled(format!(
                        "  {}",
                        error
                    ))?]));
                }
            }

            if has_warnings {
                output.push(Line::from_iter([Span::new_unstyled(" ⚠️  Warnings:")?]));
                for warning in file_parse_issue.warnings.iter() {
                    output.push(Line::from_iter([Span::new_unstyled(format!(
                        "  {}",
                        warning.clone()
                    ))?]));
                }
            }

            output.push(Line::default());
        }

        output.push(Line::from_iter([Span::new_styled(
            style(String::from("Test Validation Issues")).attribute(Attribute::Bold),
        )?]));
        for test_issue in self.test_issues.iter() {
            let has_errors = !test_issue.errors.is_empty();
            let has_warnings = !test_issue.warnings.is_empty();

            let title_colour = if has_errors {
                Color::Red
            } else if has_warnings {
                Color::Yellow
            } else {
                Color::Green
            };
            output.push(Line::from_iter([Span::new_styled(
                style(test_issue.file_path.clone()).with(title_colour),
            )?]));

            output.push(Line::from_iter([Span::new_styled(
                style(format!(
                    "{} test suites, {} test cases, {} errors, {} warnings",
                    test_issue.num_test_suites,
                    test_issue.num_test_cases,
                    test_issue.errors.len(),
                    test_issue.warnings.len(),
                ))
                .attribute(Attribute::Italic),
            )?]));

            if has_errors {
                output.push(Line::from_iter([Span::new_unstyled(String::from(
                    " ❌ Errors:",
                ))?]));
                for error in test_issue.errors.iter() {
                    output.push(Line::from_iter([Span::new_unstyled(format!(
                        "  {}",
                        error.error_message
                    ))?]));
                }
            }

            if has_warnings {
                output.push(Line::from_iter([Span::new_unstyled(" ⚠️  Warnings:")?]));
                for warning in test_issue.warnings.iter() {
                    output.push(Line::from_iter([Span::new_unstyled(format!(
                        "  {}",
                        warning.error_message
                    ))?]));
                }
            }
        }

        output.push(Line::default());
        output.push(Line::from_iter([Span::new_styled(
            style(String::from("Checking for codeowners file...")).attribute(Attribute::Bold),
        )?]));
        match &self.codeowners_issues {
            None => {
                output.push(Line::from_iter([Span::new_unstyled(
                    "  No codeowners file found",
                )?]));
            }
            Some(codeowners_issues) => {
                output.push(Line::from_iter([Span::new_styled(
                    style(format!(
                        "  Found codeowners path: {:?}",
                        codeowners_issues.file_path
                    ))
                    .attribute(Attribute::Italic),
                )?]));
                for warning in codeowners_issues.warnings.iter() {
                    output.push(Line::from_iter([Span::new_unstyled(format!(
                        "  {}",
                        warning.clone()
                    ))?]));
                }
            }
        }
        output.push(Line::default());

        let mut num_warnings = 0;
        let mut num_errors = 0;

        let mut files_with_no_issues: HashSet<String> = HashSet::new();
        let mut files_with_warnings: HashSet<String> = HashSet::new();
        let mut files_with_errors: HashSet<String> = HashSet::new();

        for file_set in self.file_sets.iter() {
            for file_path in file_set.file_paths.iter() {
                files_with_no_issues.insert(file_path.clone());
            }
        }

        let num_files = files_with_no_issues.len();

        for parse_issues in self.file_parse_issues.iter() {
            if parse_issues.fatal_error.is_some() {
                num_errors += 1;
                files_with_errors.insert(parse_issues.file_path.clone());
                files_with_no_issues.remove(&parse_issues.file_path);
            }
            if !parse_issues.errors.is_empty() {
                num_errors += parse_issues.errors.len();
                files_with_errors.insert(parse_issues.file_path.clone());
                files_with_no_issues.remove(&parse_issues.file_path);
            }
            if !parse_issues.warnings.is_empty() {
                num_warnings += parse_issues.warnings.len();
                files_with_warnings.insert(parse_issues.file_path.clone());
                files_with_no_issues.remove(&parse_issues.file_path);
            }
        }

        for test_issues in self.test_issues.iter() {
            if !test_issues.errors.is_empty() {
                num_errors += test_issues.errors.len();
                files_with_errors.insert(test_issues.file_path.clone());
                files_with_no_issues.remove(&test_issues.file_path);
            }
            if !test_issues.warnings.is_empty() {
                num_warnings += test_issues.warnings.len();
                files_with_warnings.insert(test_issues.file_path.clone());
                files_with_no_issues.remove(&test_issues.file_path);
            }
        }

        let num_files_with_no_issues = files_with_no_issues.len();
        let num_files_with_warnings = files_with_warnings.len();
        let num_files_with_errors = files_with_errors.len();

        output.push(Line::from_iter([
            Span::new_styled(style(format!("{}", num_files_with_no_issues)).with(Color::Green))?,
            Span::new_unstyled(format!(
                " {}, ",
                pluralize("valid file", num_files_with_no_issues as isize, false)
            ))?,
            Span::new_styled(style(format!("{}", num_files_with_warnings)).with(Color::Yellow))?,
            Span::new_unstyled(format!(
                " {} with warnings, and ",
                pluralize("file", num_files_with_warnings as isize, false)
            ))?,
            Span::new_styled(style(format!("{}", num_files_with_errors)).with(Color::Red))?,
            Span::new_unstyled(format!(
                " {} with errors, ",
                pluralize("file", num_files_with_errors as isize, false)
            ))?,
            Span::new_unstyled(format!(
                "with {} total (a file is double counted if it has both errors and warnings)",
                pluralize("file", num_files as isize, true)
            ))?,
        ]));
        output.push(Line::from_iter([
            Span::new_styled(style(format!("{}", num_warnings)).with(Color::Yellow))?,
            Span::new_unstyled(format!(
                " {}, and ",
                pluralize("warning", num_warnings as isize, false)
            ))?,
            Span::new_styled(style(format!("{}", num_errors as isize)).with(Color::Red))?,
            Span::new_unstyled(format!(
                " {}",
                pluralize("error", num_errors as isize, false)
            ))?,
        ]));

        Ok(output)
    }
}

fn parse_test_report(
    test_report_path: String,
) -> (
    Vec<JunitReportFileWithTestRunnerReport>,
    Option<BepParseResult>,
) {
    let mut json_parser = BazelBepParser::new(test_report_path.clone());
    let bep_parse_result = fall_back_to_binary_parse(json_parser.parse(), &test_report_path);
    match bep_parse_result {
        Ok(result) if !result.errors.is_empty() => (
            vec![JunitReportFileWithTestRunnerReport::from(test_report_path)],
            Some(result),
        ),
        Err(_) => (
            vec![JunitReportFileWithTestRunnerReport::from(test_report_path)],
            None,
        ),
        Ok(valid_result) => (valid_result.uncached_xml_files(), Some(valid_result)),
    }
}

fn flatten_glob(glob_text: &str) -> Vec<String> {
    glob::glob(glob_text)
        .into_iter()
        .flat_map(|paths| {
            paths.flat_map(|path_result| {
                path_result
                    .into_iter()
                    .flat_map(|path| path.as_os_str().to_str().map(String::from).into_iter())
            })
        })
        .collect()
}

pub async fn run_validate(validate_args: ValidateArgs) -> anyhow::Result<ValidateRunResult> {
    let ValidateArgs {
        junit_paths,
        bazel_bep_path,
        test_reports,
        show_warnings: _,
        codeowners_path,
        ..
    } = validate_args;

    let (junit_file_paths, bep_validate_result): (
        Vec<JunitReportFileWithTestRunnerReport>,
        Option<BepValidateResult>,
    ) = if !test_reports.is_empty() {
        let mut parse_results = test_reports
            .iter()
            .flat_map(|test_report_glob| flatten_glob(test_report_glob.as_str()))
            .map(parse_test_report);

        let file_paths = parse_results.clone().flat_map(|(files, _)| files).collect();
        let bep_result = parse_results.find_map(|(_, bep_result)| {
            bep_result.map(|result| BepValidateResult {
                errors: result.errors,
            })
        });
        (file_paths, bep_result)
    } else {
        match bazel_bep_path {
            Some(bazel_bep_path) => {
                let mut parser = BazelBepParser::new(bazel_bep_path);
                let bep_result = parser.parse()?;
                (
                    bep_result.uncached_xml_files(),
                    Some(BepValidateResult {
                        errors: bep_result.errors,
                    }),
                )
            }
            None => (
                junit_paths
                    .into_iter()
                    .map(JunitReportFileWithTestRunnerReport::from)
                    .collect(),
                None,
            ),
        }
    };
    validate(junit_file_paths, codeowners_path, bep_validate_result).await
}

type JunitFileToReportAndParseIssues = BTreeMap<
    String,
    (
        anyhow::Result<Option<Report>>,
        Vec<JunitParseIssue>,
        Option<FileSetTestRunnerReport>,
    ),
>;
type JunitFileToReport = BTreeMap<String, (Report, Option<FileSetTestRunnerReport>)>;
type JunitFileToParseIssues = BTreeMap<String, (anyhow::Result<()>, Vec<JunitParseIssue>)>;
type JunitFileToValidation = BTreeMap<String, JunitReportValidation>;

async fn validate(
    junit_paths: Vec<JunitReportFileWithTestRunnerReport>,
    codeowners_path: Option<String>,
    bep_result: Option<BepValidateResult>,
) -> anyhow::Result<ValidateRunResult> {
    let current_dir = std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_default();
    let file_set_builder =
        FileSetBuilder::build_file_sets(&current_dir, &junit_paths, &Option::<&str>::None, None)?;
    if file_set_builder.no_files_found() {
        let msg = "No test output files found to validate";
        tracing::warn!(msg);
        return Err(anyhow::anyhow!(msg));
    }
    let file_set_results = gen_file_set_results(&file_set_builder);

    let parse_results = parse_file_sets(file_set_builder.file_sets());
    let (parsed_reports, parse_issues) = parse_results.into_iter().fold(
        (JunitFileToReport::new(), JunitFileToParseIssues::new()),
        |(mut parsed_reports, mut parse_issues),
         (file, (parse_result, issues, test_runner_report))| {
            match parse_result {
                Ok(report) => match report {
                    Some(report) => {
                        parsed_reports.insert(file, (report, test_runner_report));
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
    let file_parse_issues = gen_parse_issues(parse_issues);

    let report_validations: JunitFileToValidation = parsed_reports
        .into_iter()
        .map(|(file, (report, test_runner_report))| {
            (
                file,
                validate_report(&report, test_runner_report.map(TestRunnerReport::from)),
            )
        })
        .collect();
    let test_issues = gen_test_issues(&report_validations);

    let codeowners = CodeOwners::find_file(&current_dir, &codeowners_path);
    let codeowners_issues = gen_codeowners_issues(codeowners, &report_validations);

    Ok(ValidateRunResult {
        bep_result,
        file_sets: file_set_results,
        file_parse_issues,
        test_issues,
        codeowners_issues,
    })
}

fn parse_file_sets(file_sets: &[FileSet]) -> JunitFileToReportAndParseIssues {
    file_sets.iter().fold(
        JunitFileToReportAndParseIssues::new(),
        |parse_results, file_set| -> JunitFileToReportAndParseIssues {
            file_set
                .files
                .iter()
                .fold(parse_results, |mut parse_results, bundled_file| {
                    let path = std::path::Path::new(&bundled_file.original_path);
                    let file = match std::fs::File::open(path) {
                        Ok(file) => file,
                        Err(e) => {
                            parse_results.insert(
                                bundled_file.get_print_path().to_string(),
                                (
                                    Err(anyhow::anyhow!(e)),
                                    Vec::new(),
                                    file_set.test_runner_report,
                                ),
                            );
                            return parse_results;
                        }
                    };

                    let file_buf_reader = BufReader::new(file);
                    let mut junit_parser = JunitParser::new();
                    if let Err(e) = junit_parser.parse(file_buf_reader) {
                        parse_results.insert(
                            bundled_file.get_print_path().to_string(),
                            (
                                Err(anyhow::anyhow!(e)),
                                Vec::new(),
                                file_set.test_runner_report,
                            ),
                        );
                        return parse_results;
                    }

                    let parse_issues = junit_parser.issues().to_vec();
                    let mut parsed_reports = junit_parser.into_reports();
                    if parsed_reports.len() != 1 {
                        parse_results.insert(
                            bundled_file.get_print_path().to_string(),
                            (Ok(None), parse_issues, file_set.test_runner_report),
                        );
                        return parse_results;
                    }

                    parse_results.insert(
                        bundled_file.get_print_path().to_string(),
                        (
                            Ok(Some(parsed_reports.remove(0))),
                            Vec::new(),
                            file_set.test_runner_report,
                        ),
                    );

                    parse_results
                })
        },
    )
}

fn gen_file_set_results(file_set_builder: &FileSetBuilder) -> Vec<FileSetResult> {
    file_set_builder
        .file_sets()
        .iter()
        .map(|file_set| FileSetResult {
            glob: file_set.glob.clone(),
            file_paths: file_set
                .files
                .iter()
                .map(|file| String::from(file.get_print_path()))
                .collect(),
        })
        .collect()
}

fn gen_parse_issues(parse_issues: JunitFileToParseIssues) -> Vec<ParseIssues> {
    parse_issues
        .into_iter()
        .map(|(file_path, (parse_result, file_issues))| {
            if let Err(e) = parse_result {
                ParseIssues {
                    file_path,
                    fatal_error: Some(e),
                    errors: Vec::new(),
                    warnings: Vec::new(),
                }
            } else {
                let errors = file_issues
                    .clone()
                    .into_iter()
                    .filter(|issue| {
                        JunitParseIssueLevel::from(issue) == JunitParseIssueLevel::Invalid
                    })
                    .collect();

                let warnings = file_issues
                    .clone()
                    .into_iter()
                    .filter(|issue| {
                        JunitParseIssueLevel::from(issue) == JunitParseIssueLevel::SubOptimal
                    })
                    .collect();

                ParseIssues {
                    file_path: file_path.clone(),
                    fatal_error: None,
                    errors,
                    warnings,
                }
            }
        })
        .collect()
}

fn gen_test_issues(report_validations: &JunitFileToValidation) -> Vec<TestIssues> {
    report_validations
        .iter()
        .map(|(file_path, report_validation)| {
            let all_issues = report_validation.all_issues_flat();
            let errors = all_issues
                .clone()
                .into_iter()
                .filter(|issue| issue.level == JunitValidationLevel::Invalid)
                .collect();
            let warnings = all_issues
                .clone()
                .into_iter()
                .filter(|issue| issue.level == JunitValidationLevel::SubOptimal)
                .collect();

            TestIssues {
                file_path: file_path.clone(),
                num_test_suites: report_validation.test_suites().len(),
                num_test_cases: report_validation.test_cases().len(),
                errors,
                warnings,
            }
        })
        .collect()
}

fn gen_codeowners_issues(
    codeowners: Option<CodeOwners>,
    report_validations: &JunitFileToValidation,
) -> Option<CodeownersIssues> {
    codeowners.map(|owners| {
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
        let warnings = if has_test_cases_without_matching_codeowners_paths {
            vec![
                String::from("CODEOWNERS found but test cases are missing filepaths. We will not be able to correlate flaky tests with owners."),
            ]
        } else {
            Vec::new()
        };

        CodeownersIssues {
            file_path: owners.path,
            warnings,
        }
    })
}

#[derive(Debug)]
pub struct JunitReportValidations {
    pub validations: BTreeMap<String, anyhow::Result<JunitReportValidation>>,
    files: Vec<String>,
    files_without_issues: Vec<String>,
}

impl JunitReportValidations {
    pub fn new(validations: BTreeMap<String, anyhow::Result<JunitReportValidation>>) -> Self {
        let mut files: Vec<String> = validations.keys().cloned().collect();
        files.sort();
        let mut files_without_issues: Vec<String> = Vec::new();
        for (file_name, validation) in validations.iter() {
            if let Ok(report_validation) = validation {
                if report_validation.num_invalid_issues() == 0
                    && report_validation.num_suboptimal_issues() == 0
                {
                    files_without_issues.push(file_name.clone());
                }
            }
        }
        Self {
            validations,
            files,
            files_without_issues,
        }
    }

    pub fn output_with_report_limits(
        &self,
        limits: &ValidationReport,
    ) -> anyhow::Result<Vec<Line>> {
        let mut output: Vec<Line> = Vec::new();
        if limits == &ValidationReport::None {
            return Ok(output);
        }

        output.push(Line::from_iter([Span::new_styled(
            String::from("📂 File Validation").attribute(Attribute::Bold),
        )?]));
        output.push(Line::default());

        if self.files.is_empty() {
            output.push(Line::from_iter([Span::new_styled(
                "⚠️  No files found".to_string().attribute(Attribute::Bold),
            )?]));
            return Ok(output);
        } else if self.files_without_issues.len() != self.files.len() {
            // found x number of files with issues
            output.push(Line::from_iter([Span::new_styled(
                format!(
                    "❕ {} found, {} with issues",
                    pluralize("file", self.files.len() as isize, true),
                    self.files.len() - self.files_without_issues.len(),
                )
                .attribute(Attribute::Bold),
            )?]));
        } else {
            // all files are perfect
            output.push(Line::from_iter([Span::new_styled(
                format!(
                    "✅ {} found, all fully correct",
                    pluralize("file", self.files.len() as isize, true)
                )
                .attribute(Attribute::Bold),
            )?]));
        }
        output.push(Line::default());

        for (file_name, validation_reports) in limits.limit_files(self.validations.iter()) {
            let mut lines: Vec<Line> = vec![];
            match validation_reports {
                Err(e) => {
                    lines.extend([
                        Line::from_iter([
                            Span::new_unstyled("❌ ")?,
                            Span::new_styled(
                                format!("{file_name} Could Not Be Parsed")
                                    .attribute(Attribute::Bold),
                            )?,
                        ]),
                        Line::from_iter([
                            Span::new_unstyled(" ↪ ")?,
                            Span::new_unstyled_lossy(format!("{:?}", e)),
                        ]),
                    ]);
                }
                Ok(report) => {
                    let issues = report.all_issues_flat();
                    let sub_optimal_issues: Vec<&JunitReportValidationFlatIssue> = issues
                        .iter()
                        .filter(|issue| issue.level == JunitValidationLevel::SubOptimal)
                        .collect();
                    let invalid_issues: Vec<&JunitReportValidationFlatIssue> = issues
                        .iter()
                        .filter(|issue| issue.level == JunitValidationLevel::Invalid)
                        .collect();
                    match (sub_optimal_issues.is_empty(), invalid_issues.is_empty()) {
                        (false, false) => {
                            lines.push(Line::from_iter([
                                Span::new_unstyled("❌ ")?,
                                Span::new_styled(
                                    format!("{file_name} Has Errors And Warnings")
                                        .attribute(Attribute::Bold),
                                )?,
                            ]));
                            lines.push(Line::from_iter([
                                Span::new_unstyled(" ↪ ❌ ")?,
                                Span::new_styled(
                                    String::from("Errors").attribute(Attribute::Bold),
                                )?,
                            ]));
                            for error in limits.limit_issues(invalid_issues.iter()) {
                                lines.push(Line::from_iter([
                                    Span::new_unstyled("   ↪ ")?,
                                    Span::new_unstyled(error.error_message.clone())?,
                                ]));
                            }
                            lines.push(Line::from_iter([
                                Span::new_unstyled(" ↪ ⚠️  ")?,
                                Span::new_styled(
                                    String::from("Warnings").attribute(Attribute::Bold),
                                )?,
                            ]));
                            for warning in limits.limit_issues(sub_optimal_issues.iter()) {
                                lines.push(Line::from_iter([
                                    Span::new_unstyled("   ↪ ")?,
                                    Span::new_unstyled(warning.error_message.clone())?,
                                ]));
                            }
                        }
                        (true, false) => {
                            lines.push(Line::from_iter([
                                Span::new_unstyled("❌ ")?,
                                Span::new_styled(
                                    format!("{file_name} Has Errors").attribute(Attribute::Bold),
                                )?,
                            ]));
                            for issue in limits.limit_issues(invalid_issues.iter()) {
                                lines.push(Line::from_iter([
                                    Span::new_unstyled(" ↪ ")?,
                                    Span::new_unstyled(issue.error_message.clone())?,
                                ]));
                            }
                        }
                        (false, true) => {
                            lines.push(Line::from_iter([
                                Span::new_unstyled("⚠️  ")?,
                                Span::new_styled(
                                    format!("{file_name} Has Warnings").attribute(Attribute::Bold),
                                )?,
                            ]));
                            for warning in limits.limit_issues(sub_optimal_issues.iter()) {
                                lines.push(Line::from_iter([
                                    Span::new_unstyled(" ↪ ")?,
                                    Span::new_unstyled(warning.error_message.clone())?,
                                ]));
                            }
                        }
                        (true, true) => {
                            // pass
                        }
                    }
                    let mut output_lines = Lines::from_iter(lines);
                    output_lines.pad_lines_left(2);
                    output.extend(output_lines);
                }
            }
        }
        if let Some(extra_files) = limits.num_exceeding_files_limit(self.validations.len()) {
            output.push(Line::from_iter([Span::new_unstyled(format!(
                "…and {} more",
                extra_files
            ))?]));
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use context::junit::junit_path::{
        JunitReportFileWithTestRunnerReport, TestRunnerReport, TestRunnerReportStatus,
    };
    use test_utils::inputs::get_test_file_path;

    use super::*;

    #[test]
    fn test_flatten_glob_returns_all_matches() {
        let path = get_test_file_path("test_fixtures/junit0*");
        let mut actual = flatten_glob(&path);
        let mut expected = vec![
            get_test_file_path("test_fixtures/junit0_fail_suite_timestamp.xml"),
            get_test_file_path("test_fixtures/junit0_fail.xml"),
            get_test_file_path("test_fixtures/junit0_pass_suite_timestamp.xml"),
            get_test_file_path("test_fixtures/junit0_pass.xml"),
        ];
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_test_report_handles_json_bep() {
        let (actual, actual_bep) =
            parse_test_report(get_test_file_path("test_fixtures/bep_example"));
        let expected = vec![JunitReportFileWithTestRunnerReport {
            junit_path: String::from("/tmp/hello_test/test.xml"),
            test_runner_report: None,
        }];
        assert_eq!(actual, expected);
        assert_eq!(actual_bep.map(|bep| bep.errors), Some(Vec::new()));
    }

    #[test]
    fn test_parse_test_report_handles_binary_bep() {
        let (mut actual, actual_bep) =
            parse_test_report(get_test_file_path("test_fixtures/bep_binary_file.bin"));
        let mut expected = vec![
            JunitReportFileWithTestRunnerReport {
                junit_path: String::from("bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/37d45ccef587444393523741a3831f4a1acbeb010f74f33130ab9ba687477558/449"),
                test_runner_report: Some(TestRunnerReport {
                    status: TestRunnerReportStatus::Passed,
                    start_time: DateTime::parse_from_rfc3339("2025-05-16T19:27:25.037Z").unwrap().to_utc(),
                    end_time: DateTime::parse_from_rfc3339("2025-05-16T19:27:27.605Z").unwrap().to_utc(),
                }),
            },
            JunitReportFileWithTestRunnerReport {
                junit_path: String::from("bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/46bbeb038d6f1447f6224a7db4d8a109e133884f2ee6ee78487ca4ce7e073de8/507"),
                test_runner_report: Some(TestRunnerReport {
                    status: TestRunnerReportStatus::Passed,
                    start_time: DateTime::parse_from_rfc3339("2025-05-16T19:29:32.732Z").unwrap().to_utc(),
                    end_time: DateTime::parse_from_rfc3339("2025-05-16T19:29:32.853Z").unwrap().to_utc(),
                }),
            },
            JunitReportFileWithTestRunnerReport {
                junit_path: String::from("bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/d1f48dadf5679f09ce9b9c8f4778281ab25bc1dfdddec943e1255baf468630de/451"),
                test_runner_report: Some(TestRunnerReport {
                    status: TestRunnerReportStatus::Passed,
                    start_time: DateTime::parse_from_rfc3339("2025-05-16T19:32:32.180Z").unwrap().to_utc(),
                    end_time: DateTime::parse_from_rfc3339("2025-05-16T19:32:34.697Z").unwrap().to_utc(),
                }),
            },
            JunitReportFileWithTestRunnerReport {
                junit_path: String::from("bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/38f1d4ce43242ed3cb08aedf1cc0c3133a8aec8e8eee61f5b84b85a5ba718bc8/1204"),
                test_runner_report: Some(TestRunnerReport {
                    status: TestRunnerReportStatus::Passed,
                    start_time: DateTime::parse_from_rfc3339("2025-05-16T19:32:31.748Z").unwrap().to_utc(),
                    end_time: DateTime::parse_from_rfc3339("2025-05-16T19:32:34.797Z").unwrap().to_utc(),
                }),
            },
            JunitReportFileWithTestRunnerReport {
                junit_path: String::from("bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/ac23080b9bf5599b7781e3b62be9bf9a5b6685a8cbe76de4e9e1731a318e9283/607"),
                test_runner_report: Some(TestRunnerReport {
                    status: TestRunnerReportStatus::Passed,
                    start_time: DateTime::parse_from_rfc3339("2025-05-16T19:33:01.680Z").unwrap().to_utc(),
                    end_time: DateTime::parse_from_rfc3339("2025-05-16T19:33:01.806Z").unwrap().to_utc(),
                }),
            },
            JunitReportFileWithTestRunnerReport {
                junit_path: String::from("bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/9c1db1d25ca6a4268be4a8982784c525a4b0ca99cbc7614094ad36c56bb08f2a/463"),
                test_runner_report: Some(TestRunnerReport {
                    status: TestRunnerReportStatus::Passed,
                    start_time: DateTime::parse_from_rfc3339("2025-05-16T19:32:52.714Z").unwrap().to_utc(),
                    end_time: DateTime::parse_from_rfc3339("2025-05-16T19:33:17.945Z").unwrap().to_utc(),
                }),
            },
            JunitReportFileWithTestRunnerReport {
                junit_path: String::from("bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/7b3ed061a782496c7418be853caae863a9ada9618712f92346ea9e8169b8acf0/1120"),
                test_runner_report: Some(TestRunnerReport {
                    status: TestRunnerReportStatus::Passed,
                    start_time: DateTime::parse_from_rfc3339("2025-05-16T19:35:16.934Z").unwrap().to_utc(),
                    end_time: DateTime::parse_from_rfc3339("2025-05-16T19:35:19.361Z").unwrap().to_utc(),
                }),
            },
            JunitReportFileWithTestRunnerReport {
                junit_path: String::from("bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/45ca1eed26b3cf1aafdb51829e32312d3b48452cc144aa041c946e89fa9c6cf6/175"),
                test_runner_report: Some(TestRunnerReport {
                    status: TestRunnerReportStatus::Passed,
                    start_time: DateTime::parse_from_rfc3339("2025-05-16T19:35:16.929Z").unwrap().to_utc(),
                    end_time: DateTime::parse_from_rfc3339("2025-05-16T19:35:19.383Z").unwrap().to_utc(),
                }),
            }
        ];
        actual.sort_by_key(|item| item.junit_path.clone());
        expected.sort_by_key(|item| item.junit_path.clone());
        assert_eq!(actual, expected);
        assert_eq!(actual_bep.map(|bep| bep.errors), Some(Vec::new()));
    }

    #[test]
    fn test_parse_test_report_falls_back_to_junit() {
        let (actual, actual_bep) =
            parse_test_report(get_test_file_path("test_fixtures/junit0_pass.xml"));
        let expected = vec![JunitReportFileWithTestRunnerReport {
            junit_path: get_test_file_path("test_fixtures/junit0_pass.xml"),
            test_runner_report: None,
        }];
        assert_eq!(actual, expected);
        assert_eq!(
            actual_bep.map(|bep| bep.errors),
            Some(vec![String::from(
                "Error parsing build event: expected value at line 1 column 1"
            )])
        );
    }
}
