#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pymethods};
use thiserror::Error;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::{
    env::{
        parser::BranchClass,
        validator::{MAX_BRANCH_NAME_LEN, MAX_EMAIL_LEN, MAX_FIELD_LEN},
    },
    string_safety::{optional_string_to_empty_str, validate_field_len, FieldLen},
};

use super::MetaContext;

#[cfg_attr(feature = "pyo3", gen_stub_pyclass_enum, pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MetaValidationLevel {
    Valid = 0,
    SubOptimal = 1,
    Invalid = 2,
}

impl Default for MetaValidationLevel {
    fn default() -> Self {
        Self::Valid
    }
}

impl ToString for MetaValidationLevel {
    fn to_string(&self) -> String {
        match self {
            MetaValidationLevel::Valid => "VALID".to_string(),
            MetaValidationLevel::SubOptimal => "SUBOPTIMAL".to_string(),
            MetaValidationLevel::Invalid => "INVALID".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetaValidationIssue {
    SubOptimal(MetaValidationIssueSubOptimal),
    Invalid(MetaValidationIssueInvalid),
}

impl ToString for MetaValidationIssue {
    fn to_string(&self) -> String {
        match self {
            Self::SubOptimal(i) => i.to_string(),
            Self::Invalid(i) => i.to_string(),
        }
    }
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum MetaValidationIssueInvalid {
    #[error("CI info branch name too short")]
    CIInfoBranchNameTooShort(String),
    #[error("CI info is classified as a PR, but has no PR number")]
    CIInfoPRNumberMissing,
    #[error("CI info has a PR number, but branch is not classified as a PR")]
    CIInfoPRNumberConflictsWithBranchClass,
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum MetaValidationIssueSubOptimal {
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
}

impl From<&MetaValidationIssue> for MetaValidationLevel {
    fn from(value: &MetaValidationIssue) -> Self {
        match value {
            MetaValidationIssue::SubOptimal(..) => MetaValidationLevel::SubOptimal,
            MetaValidationIssue::Invalid(..) => MetaValidationLevel::Invalid,
        }
    }
}

pub fn validate(meta_context: &MetaContext) -> MetaValidation {
    let mut meta_context_validation = MetaValidation::default();

    match validate_field_len::<MAX_FIELD_LEN, _>(optional_string_to_empty_str(
        &meta_context.ci_info.actor,
    )) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            meta_context_validation.add_issue(MetaValidationIssue::SubOptimal(
                MetaValidationIssueSubOptimal::CIInfoActorTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            meta_context_validation.add_issue(MetaValidationIssue::SubOptimal(
                MetaValidationIssueSubOptimal::CIInfoActorTooLong(s),
            ));
        }
    };

    match validate_field_len::<MAX_EMAIL_LEN, _>(optional_string_to_empty_str(
        &meta_context.ci_info.author_email,
    )) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            meta_context_validation.add_issue(MetaValidationIssue::SubOptimal(
                MetaValidationIssueSubOptimal::CIInfoAuthorEmailTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            meta_context_validation.add_issue(MetaValidationIssue::SubOptimal(
                MetaValidationIssueSubOptimal::CIInfoAuthorEmailTooLong(s),
            ));
        }
    };

    match validate_field_len::<MAX_FIELD_LEN, _>(optional_string_to_empty_str(
        &meta_context.ci_info.author_name,
    )) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            meta_context_validation.add_issue(MetaValidationIssue::SubOptimal(
                MetaValidationIssueSubOptimal::CIInfoAuthorNameTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            meta_context_validation.add_issue(MetaValidationIssue::SubOptimal(
                MetaValidationIssueSubOptimal::CIInfoAuthorNameTooLong(s),
            ));
        }
    };

    match validate_field_len::<MAX_BRANCH_NAME_LEN, _>(optional_string_to_empty_str(
        &meta_context.ci_info.branch,
    )) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            meta_context_validation.add_issue(MetaValidationIssue::Invalid(
                MetaValidationIssueInvalid::CIInfoBranchNameTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            meta_context_validation.add_issue(MetaValidationIssue::SubOptimal(
                MetaValidationIssueSubOptimal::CIInfoBranchNameTooLong(s),
            ));
        }
    };

    match validate_field_len::<MAX_FIELD_LEN, _>(optional_string_to_empty_str(
        &meta_context.ci_info.commit_message,
    )) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            meta_context_validation.add_issue(MetaValidationIssue::SubOptimal(
                MetaValidationIssueSubOptimal::CIInfoCommitMessageTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            meta_context_validation.add_issue(MetaValidationIssue::SubOptimal(
                MetaValidationIssueSubOptimal::CIInfoCommitMessageTooLong(s),
            ));
        }
    };

    match validate_field_len::<MAX_EMAIL_LEN, _>(optional_string_to_empty_str(
        &meta_context.ci_info.committer_email,
    )) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            meta_context_validation.add_issue(MetaValidationIssue::SubOptimal(
                MetaValidationIssueSubOptimal::CIInfoCommitterEmailTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            meta_context_validation.add_issue(MetaValidationIssue::SubOptimal(
                MetaValidationIssueSubOptimal::CIInfoCommitterEmailTooLong(s),
            ));
        }
    };

    match validate_field_len::<MAX_FIELD_LEN, _>(optional_string_to_empty_str(
        &meta_context.ci_info.committer_name,
    )) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            meta_context_validation.add_issue(MetaValidationIssue::SubOptimal(
                MetaValidationIssueSubOptimal::CIInfoCommitterNameTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            meta_context_validation.add_issue(MetaValidationIssue::SubOptimal(
                MetaValidationIssueSubOptimal::CIInfoCommitterNameTooLong(s),
            ));
        }
    };

    if let Some(branch_class) = &meta_context.ci_info.branch_class {
        match (branch_class, meta_context.ci_info.pr_number) {
            (BranchClass::PullRequest, None) => {
                meta_context_validation.add_issue(MetaValidationIssue::Invalid(
                    MetaValidationIssueInvalid::CIInfoPRNumberMissing,
                ));
            }
            (BranchClass::Merge | BranchClass::ProtectedBranch | BranchClass::None, Some(..)) => {
                meta_context_validation.add_issue(MetaValidationIssue::Invalid(
                    MetaValidationIssueInvalid::CIInfoPRNumberConflictsWithBranchClass,
                ));
            }
            (BranchClass::PullRequest, Some(..))
            | (BranchClass::Merge | BranchClass::ProtectedBranch | BranchClass::None, None) => (),
        };
    }

    meta_context_validation
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass)]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetaValidation {
    level: MetaValidationLevel,
    issues: Vec<MetaValidationIssue>,
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetaValidationFlatIssue {
    pub level: MetaValidationLevel,
    pub error_message: String,
}

#[cfg_attr(feature = "pyo3", gen_stub_pymethods, pymethods)]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
impl MetaValidation {
    pub fn level(&self) -> MetaValidationLevel {
        self.level
    }

    pub fn issues_flat(&self) -> Vec<MetaValidationFlatIssue> {
        self.issues
            .iter()
            .map(|i| MetaValidationFlatIssue {
                level: MetaValidationLevel::from(i),
                error_message: i.to_string(),
            })
            .collect()
    }

    pub fn max_level(&self) -> MetaValidationLevel {
        self.level
    }
}

impl MetaValidation {
    pub fn issues(&self) -> &[MetaValidationIssue] {
        &self.issues
    }

    fn add_issue(&mut self, issue: MetaValidationIssue) {
        self.level = self.level.max(MetaValidationLevel::from(&issue));
        self.issues.push(issue);
    }
}
