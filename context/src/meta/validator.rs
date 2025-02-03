#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pymethods};
use thiserror::Error;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use super::MetaContext;
use crate::env::validator::{
    validate as env_validate, EnvValidationIssue, EnvValidationIssueSubOptimal,
};

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
    #[error("{}", .0.to_string())]
    EnvValidationIssueSubOptimal(EnvValidationIssueSubOptimal),
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
    let env_validation = env_validate(&meta_context.ci_info);
    let meta_validation = env_validation
        .issues()
        .iter()
        .filter_map(|issue| match issue {
            EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoBranchNameTooShort(branch_name),
            ) => Some(MetaValidationIssue::Invalid(
                MetaValidationIssueInvalid::CIInfoBranchNameTooShort(branch_name.clone()),
            )),
            EnvValidationIssue::SubOptimal(EnvValidationIssueSubOptimal::CIInfoPRNumberMissing) => {
                Some(MetaValidationIssue::Invalid(
                    MetaValidationIssueInvalid::CIInfoPRNumberMissing,
                ))
            }
            EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoPRNumberConflictsWithBranchClass,
            ) => Some(MetaValidationIssue::Invalid(
                MetaValidationIssueInvalid::CIInfoPRNumberConflictsWithBranchClass,
            )),
            EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoTitleTooLong(..)
                | EnvValidationIssueSubOptimal::CIInfoTitleTooShort(..),
            ) => None,
            EnvValidationIssue::SubOptimal(env_validation_issue_suboptimal) => {
                Some(MetaValidationIssue::SubOptimal(
                    MetaValidationIssueSubOptimal::EnvValidationIssueSubOptimal(
                        env_validation_issue_suboptimal.clone(),
                    ),
                ))
            }
            _ => None,
        })
        .fold(
            MetaValidation::default(),
            |mut meta_context_validation: MetaValidation,
             meta_validation_issue: MetaValidationIssue| {
                meta_context_validation.add_issue(meta_validation_issue);
                meta_context_validation
            },
        );

    meta_validation
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
