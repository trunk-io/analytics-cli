use std::{error, fmt, path::PathBuf};

use thiserror::Error;

/// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/master/ee/lib/gitlab/code_owners/error.rb
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ErrorType {
    #[error("invalid section owner format")]
    InvalidSectionOwnerFormat,
    #[error("missing entry owner")]
    MissingEntryOwner,
    #[error("invalid entry owner format")]
    InvalidEntryOwnerFormat,
    #[error("missing section name")]
    MissingSectionName,
    #[error("invalid approval requirement")]
    InvalidApprovalRequirement,
    #[error("invalid section format")]
    InvalidSectionFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    error_type: ErrorType,
    line_number: usize,
    path: PathBuf,
}

impl Error {
    pub fn new(error_type: ErrorType, line_number: usize, path: PathBuf) -> Self {
        Self {
            error_type,
            line_number,
            path,
        }
    }

    pub fn is_fatal(&self) -> bool {
        matches!(
            self.error_type,
            ErrorType::InvalidSectionOwnerFormat
                | ErrorType::MissingSectionName
                | ErrorType::InvalidApprovalRequirement
                | ErrorType::InvalidSectionFormat
        )
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Self {
            error_type,
            line_number,
            path,
        } = &self;
        let inner = format!("{}:{line_number}\t{error_type}", path.to_string_lossy());
        f.write_str(inner.as_str())
    }
}

impl error::Error for Error {}
