#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
use thiserror::Error;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::string_safety::{validate_field_len, FieldLen};

use super::parser::{BranchClass, CIInfo};

pub const MAX_BRANCH_NAME_LEN: usize = 36;
pub const MAX_EMAIL_LEN: usize = 254;
pub const MAX_FIELD_LEN: usize = 1000;

#[cfg_attr(feature = "pyo3", pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum EnvValidationLevel {
    Valid = 0,
    SubOptimal = 1,
    Invalid = 2,
}

impl Default for EnvValidationLevel {
    fn default() -> Self {
        Self::Valid
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvValidationIssue {
    SubOptimal(EnvValidationIssueSubOptimal),
    Invalid(EnvValidationIssueInvalid),
}

impl ToString for EnvValidationIssue {
    fn to_string(&self) -> String {
        match self {
            Self::SubOptimal(i) => i.to_string(),
            Self::Invalid(i) => i.to_string(),
        }
    }
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum EnvValidationIssueSubOptimal {
    #[error("CI info actor too short")]
    CIInfoActorTooShort(String),
    #[error("CI info actor too long, truncated to {}", MAX_FIELD_LEN)]
    CIInfoActorTooLong(String),
    #[error("CI info author email too short")]
    CIInfoAuthorEmailTooShort(String),
    #[error("CI info author email too long, truncated to {}", MAX_EMAIL_LEN)]
    CIInfoAuthorEmailTooLong(String),
    #[error("CI info author name too short")]
    CIInfoAuthorNameTooShort(String),
    #[error("CI info author name too long, truncated to {}", MAX_FIELD_LEN)]
    CIInfoAuthorNameTooLong(String),
    #[error("CI info branch name too long, truncated to {}", MAX_BRANCH_NAME_LEN)]
    CIInfoBranchNameTooLong(String),
    #[error("CI info commit message too short")]
    CIInfoCommitMessageTooShort(String),
    #[error("CI info commit message too long, truncated to {}", MAX_FIELD_LEN)]
    CIInfoCommitMessageTooLong(String),
    #[error("CI info committer email too short")]
    CIInfoCommitterEmailTooShort(String),
    #[error("CI info committer email too long, truncated to {}", MAX_EMAIL_LEN)]
    CIInfoCommitterEmailTooLong(String),
    #[error("CI info committer name too short")]
    CIInfoCommitterNameTooShort(String),
    #[error("CI info committer name too long, truncated to {}", MAX_FIELD_LEN)]
    CIInfoCommitterNameTooLong(String),
    #[error("CI info job URL too short")]
    CIInfoJobURLTooShort(String),
    #[error("CI info job URL too long, truncated to {}", MAX_FIELD_LEN)]
    CIInfoJobURLTooLong(String),
    #[error("CI info title too short")]
    CIInfoTitleTooShort(String),
    #[error("CI info title too long, truncated to {}", MAX_FIELD_LEN)]
    CIInfoTitleTooLong(String),
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum EnvValidationIssueInvalid {
    #[error("CI info branch name too short")]
    CIInfoBranchNameTooShort(String),
    #[error("CI info is classified as a PR, but has not PR number")]
    CIInfoPRNumberMissing,
    #[error("CI info has a PR number, but branch is not classified as a PR")]
    CIInfoPRNumberConflictsWithBranchClass,
}

impl From<&EnvValidationIssue> for EnvValidationLevel {
    fn from(value: &EnvValidationIssue) -> Self {
        match value {
            EnvValidationIssue::SubOptimal(..) => EnvValidationLevel::SubOptimal,
            EnvValidationIssue::Invalid(..) => EnvValidationLevel::Invalid,
        }
    }
}

pub fn validate(ci_info: &CIInfo) -> EnvValidation {
    let mut env_validation = EnvValidation::default();

    match validate_field_len::<MAX_FIELD_LEN, _>(optional_string_to_empty_str(&ci_info.actor)) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            env_validation.add_issue(EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoActorTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            env_validation.add_issue(EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoActorTooLong(s),
            ));
        }
    };

    match validate_field_len::<MAX_EMAIL_LEN, _>(optional_string_to_empty_str(
        &ci_info.author_email,
    )) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            env_validation.add_issue(EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoAuthorEmailTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            env_validation.add_issue(EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoAuthorEmailTooLong(s),
            ));
        }
    };

    match validate_field_len::<MAX_FIELD_LEN, _>(optional_string_to_empty_str(&ci_info.author_name))
    {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            env_validation.add_issue(EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoAuthorNameTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            env_validation.add_issue(EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoAuthorNameTooLong(s),
            ));
        }
    };

    match validate_field_len::<MAX_BRANCH_NAME_LEN, _>(optional_string_to_empty_str(
        &ci_info.branch,
    )) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            env_validation.add_issue(EnvValidationIssue::Invalid(
                EnvValidationIssueInvalid::CIInfoBranchNameTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            env_validation.add_issue(EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoBranchNameTooLong(s),
            ));
        }
    };

    match validate_field_len::<MAX_FIELD_LEN, _>(optional_string_to_empty_str(
        &ci_info.commit_message,
    )) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            env_validation.add_issue(EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoCommitMessageTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            env_validation.add_issue(EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoCommitMessageTooLong(s),
            ));
        }
    };

    match validate_field_len::<MAX_EMAIL_LEN, _>(optional_string_to_empty_str(
        &ci_info.committer_email,
    )) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            env_validation.add_issue(EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoCommitterEmailTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            env_validation.add_issue(EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoCommitterEmailTooLong(s),
            ));
        }
    };

    match validate_field_len::<MAX_FIELD_LEN, _>(optional_string_to_empty_str(
        &ci_info.committer_name,
    )) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            env_validation.add_issue(EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoCommitterNameTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            env_validation.add_issue(EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoCommitterNameTooLong(s),
            ));
        }
    };

    match validate_field_len::<MAX_FIELD_LEN, _>(optional_string_to_empty_str(&ci_info.job_url)) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            env_validation.add_issue(EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoJobURLTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            env_validation.add_issue(EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoJobURLTooLong(s),
            ));
        }
    };

    match validate_field_len::<MAX_FIELD_LEN, _>(optional_string_to_empty_str(&ci_info.title)) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            env_validation.add_issue(EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoTitleTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            env_validation.add_issue(EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoTitleTooLong(s),
            ));
        }
    };

    if let Some(branch_class) = &ci_info.branch_class {
        match (branch_class, ci_info.pr_number) {
            (BranchClass::PullRequest, None) => {
                env_validation.add_issue(EnvValidationIssue::Invalid(
                    EnvValidationIssueInvalid::CIInfoPRNumberMissing,
                ));
            }
            (BranchClass::Merge | BranchClass::ProtectedBranch, Some(..)) => {
                env_validation.add_issue(EnvValidationIssue::Invalid(
                    EnvValidationIssueInvalid::CIInfoPRNumberConflictsWithBranchClass,
                ));
            }
            (BranchClass::PullRequest, Some(..))
            | (BranchClass::Merge | BranchClass::ProtectedBranch, None) => (),
        };
    }

    env_validation
}

#[cfg_attr(feature = "pyo3", pyclass)]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvValidation {
    level: EnvValidationLevel,
    issues: Vec<EnvValidationIssue>,
}

#[cfg_attr(feature = "pyo3", pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvValidationFlatIssue {
    pub level: EnvValidationLevel,
    pub error_message: String,
}

#[cfg_attr(feature = "pyo3", pymethods)]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
impl EnvValidation {
    pub fn level(&self) -> EnvValidationLevel {
        self.level
    }

    pub fn issues_flat(&self) -> Vec<EnvValidationFlatIssue> {
        self.issues
            .iter()
            .map(|i| EnvValidationFlatIssue {
                level: EnvValidationLevel::from(i),
                error_message: i.to_string(),
            })
            .collect()
    }

    pub fn max_level(&self) -> EnvValidationLevel {
        self.level
    }
}

impl EnvValidation {
    pub fn issues(&self) -> &[EnvValidationIssue] {
        &self.issues
    }

    fn add_issue(&mut self, issue: EnvValidationIssue) {
        self.level = self.level.max(EnvValidationLevel::from(&issue));
        self.issues.push(issue);
    }
}

fn optional_string_to_empty_str<'a>(optional_string: &'a Option<String>) -> &'a str {
    optional_string.as_ref().map_or("", |s| &s)
}
