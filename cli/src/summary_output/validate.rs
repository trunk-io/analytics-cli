use std::collections::HashSet;

use context::junit::validator::JunitReportValidationFlatIssue;
use serde::Serialize;

use super::{FileValidation, Notice, ParseError, upload::files_with_old_timestamps};
use crate::{
    exit_code::ExitCode,
    validate_command::{ParseIssues, TestIssues, ValidateRunResult},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidateReport {
    pub matched_files: Vec<MatchedFiles>,
    pub validations: Vec<FileValidation>,
    pub codeowners: Option<Codeowners>,
    pub counts: ValidationCounts,
    pub exit_code: ExitCode,
    pub notices: Vec<Notice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchedFiles {
    pub glob: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Codeowners {
    pub path: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationCounts {
    pub files: usize,
    pub valid_files: usize,
    pub files_with_warnings: usize,
    pub files_with_errors: usize,
    pub warnings: usize,
    pub errors: usize,
    /// Files whose results Trunk will drop for having old timestamps.
    pub files_with_old_timestamps: usize,
}

impl ValidateReport {
    pub fn new(result: &ValidateRunResult) -> Self {
        let matched_files: Vec<MatchedFiles> = result
            .file_sets
            .iter()
            .map(|file_set| MatchedFiles {
                glob: file_set.glob.clone(),
                files: file_set.file_paths.clone(),
            })
            .collect();
        let mut validations: Vec<FileValidation> = result
            .file_parse_issues
            .iter()
            .map(parse_validation)
            .chain(result.test_issues.iter().map(test_validation))
            .collect();
        validations.sort_by(|a, b| a.file.cmp(&b.file));
        let has_errors = validations
            .iter()
            .any(|validation| validation.parse_error.is_some() || !validation.errors.is_empty());
        let mut notices = result.notices.clone();
        if let Some(bep_result) = &result.bep_result {
            notices.extend(bep_result.errors.iter().map(Notice::bep_parse_error));
        }

        Self {
            counts: ValidationCounts::new(&matched_files, &validations),
            matched_files,
            validations,
            codeowners: result
                .codeowners_issues
                .as_ref()
                .map(|codeowners| Codeowners {
                    path: codeowners.file_path.to_string_lossy().into_owned(),
                    warnings: codeowners.warnings.clone(),
                }),
            exit_code: if has_errors {
                ExitCode::Failure
            } else {
                ExitCode::Success
            },
            notices,
        }
    }
}

impl ValidationCounts {
    /// `files` counts each file once, even when several globs match it. A file with both errors
    /// and warnings counts in both `files_with_errors` and `files_with_warnings`.
    fn new(matched_files: &[MatchedFiles], validations: &[FileValidation]) -> Self {
        let files: HashSet<&str> = matched_files
            .iter()
            .flat_map(|matched| matched.files.iter().map(String::as_str))
            .collect();
        let mut valid_files = files.clone();
        let mut files_with_warnings = HashSet::new();
        let mut files_with_errors = HashSet::new();
        let mut warnings = 0;
        let mut errors = 0;
        for validation in validations {
            let file = validation.file.as_str();
            let num_errors =
                validation.errors.len() + usize::from(validation.parse_error.is_some());
            if num_errors > 0 {
                errors += num_errors;
                files_with_errors.insert(file);
                valid_files.remove(file);
            }
            if !validation.warnings.is_empty() {
                warnings += validation.warnings.len();
                files_with_warnings.insert(file);
                valid_files.remove(file);
            }
        }
        Self {
            files: files.len(),
            valid_files: valid_files.len(),
            files_with_warnings: files_with_warnings.len(),
            files_with_errors: files_with_errors.len(),
            warnings,
            errors,
            files_with_old_timestamps: files_with_old_timestamps(validations),
        }
    }
}

/// A file that wasn't read as exactly one report, so it has no test counts.
fn parse_validation(issues: &ParseIssues) -> FileValidation {
    FileValidation {
        file: issues.file_path.clone(),
        parse_error: issues.fatal_error.as_ref().map(ParseError::new),
        errors: issues.errors.iter().map(ToString::to_string).collect(),
        warnings: issues.warnings.iter().map(ToString::to_string).collect(),
        test_suite_count: None,
        test_case_count: None,
        old_timestamps: false,
    }
}

fn test_validation(issues: &TestIssues) -> FileValidation {
    let messages = |issues: &[JunitReportValidationFlatIssue]| {
        issues
            .iter()
            .map(|issue| issue.error_message.clone())
            .collect()
    };
    FileValidation {
        file: issues.file_path.clone(),
        parse_error: None,
        errors: messages(&issues.errors),
        warnings: messages(&issues.warnings),
        test_suite_count: Some(issues.num_test_suites),
        test_case_count: Some(issues.num_test_cases),
        old_timestamps: issues.old_timestamps,
    }
}
