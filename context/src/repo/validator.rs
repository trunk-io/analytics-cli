use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::string_safety::{validate_field_len, FieldLen};

use super::BundleRepo;

pub const MAX_BRANCH_NAME_LEN: usize = 36;
pub const MAX_EMAIL_LEN: usize = 254;
pub const MAX_FIELD_LEN: usize = 1000;

const TIMESTAMP_OLD_DAYS: u32 = 30;
const TIMESTAMP_STALE_HOURS: u32 = 1;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RepoValidationLevel {
    Valid = 0,
    SubOptimal = 1,
    Invalid = 2,
}

impl Default for RepoValidationLevel {
    fn default() -> Self {
        Self::Valid
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoValidationIssue {
    SubOptimal(RepoValidationIssueSubOptimal),
    Invalid(RepoValidationIssueInvalid),
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum RepoValidationIssueSubOptimal {
    #[error("repo head commit author email too short")]
    RepoAuthorEmailTooShort(String),
    #[error(
        "repo head commit author email too long, truncated to {}",
        MAX_EMAIL_LEN
    )]
    RepoAuthorEmailTooLong(String),
    #[error("repo head commit author name too short")]
    RepoAuthorNameTooShort(String),
    #[error(
        "repo head commit author name too long, truncated to {}",
        MAX_FIELD_LEN
    )]
    RepoAuthorNameTooLong(String),
    #[error("repo head branch name too long, truncated to {}", MAX_BRANCH_NAME_LEN)]
    RepoBranchNameTooLong(String),
    #[error("repo head commit message too short")]
    RepoCommitMessageTooShort(String),
    #[error("repo head commit message too long, truncated to {}", MAX_FIELD_LEN)]
    RepoCommitMessageTooLong(String),
    #[error("repo head commit has future timestamp")]
    RepoCommitFutureTimestamp(DateTime<Utc>),
    #[error("repo head commit has old (> {} day(s)) timestamp", TIMESTAMP_OLD_DAYS)]
    RepoCommitOldTimestamp(DateTime<Utc>),
    #[error(
        "repo head commit has stale (> {} hour(s)) timestamp",
        TIMESTAMP_STALE_HOURS
    )]
    RepoCommitStaleTimestamp(DateTime<Utc>),
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum RepoValidationIssueInvalid {
    #[error("repo branch name too short")]
    RepoBranchNameTooShort(String),
}

impl From<&RepoValidationIssue> for RepoValidationLevel {
    fn from(value: &RepoValidationIssue) -> Self {
        match value {
            RepoValidationIssue::SubOptimal(..) => RepoValidationLevel::SubOptimal,
            RepoValidationIssue::Invalid(..) => RepoValidationLevel::Invalid,
        }
    }
}

pub fn validate(bundle_repo: &BundleRepo) -> RepoValidation {
    let mut repo_validation = RepoValidation::default();

    match validate_field_len::<MAX_EMAIL_LEN, _>(&bundle_repo.repo_head_author_email) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            repo_validation.add_issue(RepoValidationIssue::SubOptimal(
                RepoValidationIssueSubOptimal::RepoAuthorEmailTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            repo_validation.add_issue(RepoValidationIssue::SubOptimal(
                RepoValidationIssueSubOptimal::RepoAuthorEmailTooLong(s),
            ));
        }
    };

    match validate_field_len::<MAX_FIELD_LEN, _>(&bundle_repo.repo_head_author_name) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            repo_validation.add_issue(RepoValidationIssue::SubOptimal(
                RepoValidationIssueSubOptimal::RepoAuthorNameTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            repo_validation.add_issue(RepoValidationIssue::SubOptimal(
                RepoValidationIssueSubOptimal::RepoAuthorNameTooLong(s),
            ));
        }
    };

    match validate_field_len::<MAX_BRANCH_NAME_LEN, _>(&bundle_repo.repo_head_branch) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            repo_validation.add_issue(RepoValidationIssue::Invalid(
                RepoValidationIssueInvalid::RepoBranchNameTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            repo_validation.add_issue(RepoValidationIssue::SubOptimal(
                RepoValidationIssueSubOptimal::RepoBranchNameTooLong(s),
            ));
        }
    };

    match validate_field_len::<MAX_FIELD_LEN, _>(&bundle_repo.repo_head_commit_message) {
        FieldLen::Valid => (),
        FieldLen::TooShort(s) => {
            repo_validation.add_issue(RepoValidationIssue::SubOptimal(
                RepoValidationIssueSubOptimal::RepoCommitMessageTooShort(s),
            ));
        }
        FieldLen::TooLong(s) => {
            repo_validation.add_issue(RepoValidationIssue::SubOptimal(
                RepoValidationIssueSubOptimal::RepoCommitMessageTooLong(s),
            ));
        }
    };

    if let Some(timestamp) = DateTime::from_timestamp(bundle_repo.repo_head_commit_epoch, 0) {
        let now = Utc::now();
        let time_since_timestamp = now - timestamp;

        if timestamp > now {
            repo_validation.add_issue(RepoValidationIssue::SubOptimal(
                RepoValidationIssueSubOptimal::RepoCommitFutureTimestamp(timestamp),
            ));
        } else if time_since_timestamp.num_days() > i64::from(TIMESTAMP_OLD_DAYS) {
            repo_validation.add_issue(RepoValidationIssue::SubOptimal(
                RepoValidationIssueSubOptimal::RepoCommitOldTimestamp(timestamp),
            ));
        } else if time_since_timestamp.num_hours() > i64::from(TIMESTAMP_STALE_HOURS) {
            repo_validation.add_issue(RepoValidationIssue::SubOptimal(
                RepoValidationIssueSubOptimal::RepoCommitStaleTimestamp(timestamp),
            ));
        }
    }

    repo_validation
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoValidation {
    level: RepoValidationLevel,
    issues: Vec<RepoValidationIssue>,
}

impl RepoValidation {
    pub fn level(&self) -> RepoValidationLevel {
        self.level
    }

    pub fn issues(&self) -> &[RepoValidationIssue] {
        &self.issues
    }

    pub fn max_level(&self) -> RepoValidationLevel {
        self.level
    }

    fn add_issue(&mut self, issue: RepoValidationIssue) {
        self.level = self.level.max(RepoValidationLevel::from(&issue));
        self.issues.push(issue);
    }
}
