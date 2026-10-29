use std::{collections::BTreeMap, io::BufReader, path::PathBuf};

use bundle::{FileSet, FileSetBuilder, FileSetTestRunnerReport};
use clap::{ArgAction, Args, arg};
use codeowners::{CodeOwners, OwnersSource};
use context::{
    bazel_bep::{common::BepParseResult, parser::BazelBepParser},
    junit::{
        bindings::BindingsReport,
        junit_path::{JunitReportFileWithTestRunnerReport, TestRunnerReport},
        parser::{JunitParseIssue, JunitParseIssueLevel, JunitParser},
        validator::{
            JunitReportValidation, JunitReportValidationFlatIssue,
            JunitReportValidationIssueSubOptimal, JunitValidationIssue, JunitValidationIssueType,
            JunitValidationLevel,
        },
    },
};
use quick_junit::Report;

use crate::{
    context::fall_back_to_binary_parse, error_report::InterruptingError, summary_output::Notice,
    upload_command::CodeownersType,
};

#[derive(Args, Clone, Debug)]
pub struct ValidateArgs {
    #[arg(
        long,
        required_unless_present_any = ["bazel_bep_path", "test_reports"],
        conflicts_with_all = ["bazel_bep_path", "test_reports"],
        value_delimiter = ',',
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        help = "Comma-separated list of glob patterns to locate JUnit XML files (e.g., '**/test-results/**/*.xml').",
    )]
    junit_paths: Vec<String>,
    #[arg(
        long,
        required_unless_present_any = ["junit_paths", "test_reports"],
        help = "Path to Bazel Build Event Protocol JSON file. BEP files contain test results and build metadata."
    )]
    bazel_bep_path: Option<String>,
    #[arg(
        long,
        required_unless_present_any = ["junit_paths", "bazel_bep_path"],
        value_delimiter = ',',
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        help = "Comma-separated list of glob patterns to test report files. Supports JUnit XML, Bazel BEP, and XCResult formats."
    )]
    pub test_reports: Vec<String>,
    #[arg(long, help = "Show warning-level log messages in output.", hide = true)]
    show_warnings: bool,
    #[arg(
        long,
        help = "Override the path to a CODEOWNERS file. Used to validate code ownership associations."
    )]
    pub codeowners_path: Option<String>,
    #[arg(
        long,
        env = constants::TRUNK_CODEOWNERS_TYPE_ENV,
        value_enum,
        help = "Specify CODEOWNERS parser type ('github' or 'gitlab'). Optional; when unset, codeowners type will be inferred."
    )]
    pub codeowners_type: Option<CodeownersType>,
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
    pub(crate) errors: Vec<String>,
}

#[derive(Debug)]
pub struct FileSetResult {
    pub(crate) glob: String,
    pub(crate) file_paths: Vec<String>,
}

#[derive(Debug)]
pub struct ParseIssues {
    pub(crate) file_path: String,
    pub(crate) fatal_error: Option<anyhow::Error>,
    pub(crate) errors: Vec<JunitParseIssue>,
    pub(crate) warnings: Vec<JunitParseIssue>,
}

#[derive(Debug)]
pub struct TestIssues {
    pub(crate) file_path: String,
    pub(crate) num_test_suites: usize,
    pub(crate) num_test_cases: usize,
    pub(crate) errors: Vec<JunitReportValidationFlatIssue>,
    pub(crate) warnings: Vec<JunitReportValidationFlatIssue>,
    pub(crate) old_timestamps: bool,
}

#[derive(Debug)]
pub struct CodeownersIssues {
    pub(crate) file_path: PathBuf,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug)]
pub struct ValidateRunResult {
    pub(crate) bep_result: Option<BepValidateResult>,
    pub(crate) file_sets: Vec<FileSetResult>,
    pub(crate) file_parse_issues: Vec<ParseIssues>,
    pub(crate) test_issues: Vec<TestIssues>,
    pub(crate) codeowners_issues: Option<CodeownersIssues>,
    pub(crate) notices: Vec<Notice>,
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
    let mut notices = Vec::new();
    if validate_args.hide_banner {
        let message = "The --hide-banner flag is deprecated and does nothing. It will be removed in a future version.";
        tracing::error!("{message}");
        notices.push(Notice::deprecation(message));
    }

    let ValidateArgs {
        junit_paths,
        bazel_bep_path,
        test_reports,
        show_warnings: _,
        codeowners_path,
        codeowners_type,
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
    validate(
        junit_file_paths,
        codeowners_path,
        codeowners_type.map(OwnersSource::from),
        bep_validate_result,
        notices,
    )
    .await
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
    codeowners_type: Option<OwnersSource>,
    bep_result: Option<BepValidateResult>,
    notices: Vec<Notice>,
) -> anyhow::Result<ValidateRunResult> {
    let current_dir = std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_default();
    let file_set_builder = FileSetBuilder::build_file_sets(
        &current_dir,
        &junit_paths,
        &Option::<&str>::None,
        None,
        None,
    )?;
    if file_set_builder.no_files_found() {
        let msg = "No test output files found to validate";
        tracing::warn!(msg);
        return Err(anyhow::anyhow!(InterruptingError::new(msg)));
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

    let reference_timestamp = chrono::Utc::now().fixed_offset();
    let report_validations: JunitFileToValidation = parsed_reports
        .into_iter()
        .map(|(file, (report, test_runner_report))| {
            let bindings_report = BindingsReport::from(report);
            let validation = context::junit::validator::validate(
                &bindings_report,
                &test_runner_report
                    .as_ref()
                    .map(|trr| TestRunnerReport::from(trr.clone())),
                reference_timestamp,
            );
            (file, JunitReportValidation::from(validation))
        })
        .collect();
    let test_issues = gen_test_issues(&report_validations);

    let codeowners = CodeOwners::find_file(&current_dir, &codeowners_path, codeowners_type);
    let codeowners_issues = gen_codeowners_issues(codeowners, &report_validations);

    Ok(ValidateRunResult {
        bep_result,
        file_sets: file_set_results,
        file_parse_issues,
        test_issues,
        codeowners_issues,
        notices,
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
                                    file_set.test_runner_report.clone(),
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
                                file_set.test_runner_report.clone(),
                            ),
                        );
                        return parse_results;
                    }

                    let parse_issues = junit_parser.issues().to_vec();
                    let mut parsed_reports = junit_parser.into_reports();
                    if parsed_reports.len() != 1 {
                        parse_results.insert(
                            bundled_file.get_print_path().to_string(),
                            (Ok(None), parse_issues, file_set.test_runner_report.clone()),
                        );
                        return parse_results;
                    }

                    parse_results.insert(
                        bundled_file.get_print_path().to_string(),
                        (
                            Ok(Some(parsed_reports.remove(0))),
                            Vec::new(),
                            file_set.test_runner_report.clone(),
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
                old_timestamps: report_validation.has_old_timestamps(),
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
}

impl JunitReportValidations {
    pub fn new(validations: BTreeMap<String, anyhow::Result<JunitReportValidation>>) -> Self {
        Self { validations }
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
                junit_path: String::from(
                    "bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/37d45ccef587444393523741a3831f4a1acbeb010f74f33130ab9ba687477558/449",
                ),
                test_runner_report: Some(TestRunnerReport {
                    status: TestRunnerReportStatus::Passed,
                    start_time: DateTime::parse_from_rfc3339("2025-05-16T19:27:25.037Z")
                        .unwrap()
                        .to_utc(),
                    end_time: DateTime::parse_from_rfc3339("2025-05-16T19:27:27.605Z")
                        .unwrap()
                        .to_utc(),
                }),
            },
            JunitReportFileWithTestRunnerReport {
                junit_path: String::from(
                    "bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/46bbeb038d6f1447f6224a7db4d8a109e133884f2ee6ee78487ca4ce7e073de8/507",
                ),
                test_runner_report: Some(TestRunnerReport {
                    status: TestRunnerReportStatus::Passed,
                    start_time: DateTime::parse_from_rfc3339("2025-05-16T19:29:32.732Z")
                        .unwrap()
                        .to_utc(),
                    end_time: DateTime::parse_from_rfc3339("2025-05-16T19:29:32.853Z")
                        .unwrap()
                        .to_utc(),
                }),
            },
            JunitReportFileWithTestRunnerReport {
                junit_path: String::from(
                    "bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/d1f48dadf5679f09ce9b9c8f4778281ab25bc1dfdddec943e1255baf468630de/451",
                ),
                test_runner_report: Some(TestRunnerReport {
                    status: TestRunnerReportStatus::Passed,
                    start_time: DateTime::parse_from_rfc3339("2025-05-16T19:32:32.180Z")
                        .unwrap()
                        .to_utc(),
                    end_time: DateTime::parse_from_rfc3339("2025-05-16T19:32:34.697Z")
                        .unwrap()
                        .to_utc(),
                }),
            },
            JunitReportFileWithTestRunnerReport {
                junit_path: String::from(
                    "bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/38f1d4ce43242ed3cb08aedf1cc0c3133a8aec8e8eee61f5b84b85a5ba718bc8/1204",
                ),
                test_runner_report: Some(TestRunnerReport {
                    status: TestRunnerReportStatus::Passed,
                    start_time: DateTime::parse_from_rfc3339("2025-05-16T19:32:31.748Z")
                        .unwrap()
                        .to_utc(),
                    end_time: DateTime::parse_from_rfc3339("2025-05-16T19:32:34.797Z")
                        .unwrap()
                        .to_utc(),
                }),
            },
            JunitReportFileWithTestRunnerReport {
                junit_path: String::from(
                    "bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/ac23080b9bf5599b7781e3b62be9bf9a5b6685a8cbe76de4e9e1731a318e9283/607",
                ),
                test_runner_report: Some(TestRunnerReport {
                    status: TestRunnerReportStatus::Passed,
                    start_time: DateTime::parse_from_rfc3339("2025-05-16T19:33:01.680Z")
                        .unwrap()
                        .to_utc(),
                    end_time: DateTime::parse_from_rfc3339("2025-05-16T19:33:01.806Z")
                        .unwrap()
                        .to_utc(),
                }),
            },
            JunitReportFileWithTestRunnerReport {
                junit_path: String::from(
                    "bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/9c1db1d25ca6a4268be4a8982784c525a4b0ca99cbc7614094ad36c56bb08f2a/463",
                ),
                test_runner_report: Some(TestRunnerReport {
                    status: TestRunnerReportStatus::Passed,
                    start_time: DateTime::parse_from_rfc3339("2025-05-16T19:32:52.714Z")
                        .unwrap()
                        .to_utc(),
                    end_time: DateTime::parse_from_rfc3339("2025-05-16T19:33:17.945Z")
                        .unwrap()
                        .to_utc(),
                }),
            },
            JunitReportFileWithTestRunnerReport {
                junit_path: String::from(
                    "bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/7b3ed061a782496c7418be853caae863a9ada9618712f92346ea9e8169b8acf0/1120",
                ),
                test_runner_report: Some(TestRunnerReport {
                    status: TestRunnerReportStatus::Passed,
                    start_time: DateTime::parse_from_rfc3339("2025-05-16T19:35:16.934Z")
                        .unwrap()
                        .to_utc(),
                    end_time: DateTime::parse_from_rfc3339("2025-05-16T19:35:19.361Z")
                        .unwrap()
                        .to_utc(),
                }),
            },
            JunitReportFileWithTestRunnerReport {
                junit_path: String::from(
                    "bytestream://buildbarn2.build.trunk-staging.io:1986/blobs/45ca1eed26b3cf1aafdb51829e32312d3b48452cc144aa041c946e89fa9c6cf6/175",
                ),
                test_runner_report: Some(TestRunnerReport {
                    status: TestRunnerReportStatus::Passed,
                    start_time: DateTime::parse_from_rfc3339("2025-05-16T19:35:16.929Z")
                        .unwrap()
                        .to_utc(),
                    end_time: DateTime::parse_from_rfc3339("2025-05-16T19:35:19.383Z")
                        .unwrap()
                        .to_utc(),
                }),
            },
        ];
        actual.sort_by_key(|item| item.junit_path.clone());
        expected.sort_by_key(|item| item.junit_path.clone());
        println!("actual: {:#?}", actual);
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
