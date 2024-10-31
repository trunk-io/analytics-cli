use chrono::{DateTime, FixedOffset, Utc};
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum};
use quick_junit::Report;
use thiserror::Error;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::string_safety::{validate_field_len, FieldLen};

use super::parser::extra_attrs;

pub const MAX_FIELD_LEN: usize = 1_000;

const TIMESTAMP_OLD_DAYS: u32 = 30;
const TIMESTAMP_STALE_HOURS: u32 = 1;

#[cfg_attr(feature = "pyo3", gen_stub_pyclass_enum, pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum JunitValidationLevel {
    Valid = 0,
    SubOptimal = 1,
    Invalid = 2,
}

impl Default for JunitValidationLevel {
    fn default() -> Self {
        Self::Valid
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JunitValidationIssue<SO, I> {
    SubOptimal(SO),
    Invalid(I),
}

impl<SO, I> From<&JunitValidationIssue<SO, I>> for JunitValidationLevel {
    fn from(value: &JunitValidationIssue<SO, I>) -> Self {
        match value {
            JunitValidationIssue::SubOptimal(..) => JunitValidationLevel::SubOptimal,
            JunitValidationIssue::Invalid(..) => JunitValidationLevel::Invalid,
        }
    }
}

pub fn validate(report: &Report) -> JunitReportValidation {
    let mut report_validation = JunitReportValidation::default();

    for test_suite in report.test_suites.iter() {
        let mut test_suite_validation = JunitTestSuiteValidation::default();

        match validate_field_len::<MAX_FIELD_LEN, _>(test_suite.name.as_str()) {
            FieldLen::Valid => (),
            FieldLen::TooShort(s) => {
                test_suite_validation.add_issue(JunitValidationIssue::Invalid(
                    JunitTestSuiteValidationIssueInvalid::TestSuiteNameTooShort(s),
                ));
            }
            FieldLen::TooLong(s) => {
                test_suite_validation.add_issue(JunitValidationIssue::SubOptimal(
                    JunitTestSuiteValidationIssueSubOptimal::TestSuiteNameTooLong(s),
                ));
            }
        };

        if let Some(raw_test_suite_id) = test_suite.extra.get("id") {
            let test_case_id = uuid::Uuid::parse_str(raw_test_suite_id).unwrap_or_default();
            if test_case_id.get_version() != Some(uuid::Version::Sha1) {
                test_suite_validation.add_issue(JunitValidationIssue::Invalid(
                    JunitTestSuiteValidationIssueInvalid::TestSuiteInvalidId(
                        raw_test_suite_id.to_string().clone(),
                    ),
                ));
            }
        }

        for test_case in test_suite.test_cases.iter() {
            let mut test_case_validation = JunitTestCaseValidation::default();

            match validate_field_len::<MAX_FIELD_LEN, _>(test_case.name.as_str()) {
                FieldLen::Valid => (),
                FieldLen::TooShort(s) => {
                    test_case_validation.add_issue(JunitValidationIssue::Invalid(
                        JunitTestCaseValidationIssueInvalid::TestCaseNameTooShort(s),
                    ));
                }
                FieldLen::TooLong(s) => {
                    test_case_validation.add_issue(JunitValidationIssue::SubOptimal(
                        JunitTestCaseValidationIssueSubOptimal::TestCaseNameTooLong(s),
                    ));
                }
            };

            if let Some(raw_test_case_id) = test_case.extra.get("id") {
                let test_case_id = uuid::Uuid::parse_str(raw_test_case_id).unwrap_or_default();
                if test_case_id.get_version() != Some(uuid::Version::Sha1) {
                    test_case_validation.add_issue(JunitValidationIssue::Invalid(
                        JunitTestCaseValidationIssueInvalid::TestCaseInvalidId(
                            raw_test_case_id.to_string().clone(),
                        ),
                    ));
                }
            }

            match validate_field_len::<MAX_FIELD_LEN, _>(
                test_case
                    .extra
                    .get(extra_attrs::FILE)
                    .or(test_case.extra.get(extra_attrs::FILEPATH))
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or_default(),
            ) {
                FieldLen::Valid => (),
                FieldLen::TooShort(s) => {
                    test_case_validation.add_issue(JunitValidationIssue::SubOptimal(
                        JunitTestCaseValidationIssueSubOptimal::TestCaseFileOrFilepathTooShort(s),
                    ));
                }
                FieldLen::TooLong(s) => {
                    test_case_validation.add_issue(JunitValidationIssue::SubOptimal(
                        JunitTestCaseValidationIssueSubOptimal::TestCaseFileOrFilepathTooLong(s),
                    ));
                }
            };

            match validate_field_len::<MAX_FIELD_LEN, _>(
                test_case
                    .classname
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or_default(),
            ) {
                FieldLen::Valid => (),
                FieldLen::TooShort(s) => {
                    test_case_validation.add_issue(JunitValidationIssue::SubOptimal(
                        JunitTestCaseValidationIssueSubOptimal::TestCaseClassnameTooShort(s),
                    ));
                }
                FieldLen::TooLong(s) => {
                    test_case_validation.add_issue(JunitValidationIssue::SubOptimal(
                        JunitTestCaseValidationIssueSubOptimal::TestCaseClassnameTooLong(s),
                    ));
                }
            };

            if test_case.time.or(test_suite.time).or(report.time).is_none() {
                test_case_validation.add_issue(JunitValidationIssue::SubOptimal(
                    JunitTestCaseValidationIssueSubOptimal::TestCaseNoTimeDuration,
                ));
            }

            if let Some(timestamp) = test_case
                .timestamp
                .or(test_suite.timestamp)
                .or(report.timestamp)
            {
                let now = Utc::now().fixed_offset();
                let time_since_timestamp = now - timestamp;

                if timestamp > now {
                    test_case_validation.add_issue(JunitValidationIssue::SubOptimal(
                        JunitTestCaseValidationIssueSubOptimal::TestCaseFutureTimestamp(timestamp),
                    ));
                } else if time_since_timestamp.num_days() > i64::from(TIMESTAMP_OLD_DAYS) {
                    test_case_validation.add_issue(JunitValidationIssue::SubOptimal(
                        JunitTestCaseValidationIssueSubOptimal::TestCaseOldTimestamp(timestamp),
                    ));
                } else if time_since_timestamp.num_hours() > i64::from(TIMESTAMP_STALE_HOURS) {
                    test_case_validation.add_issue(JunitValidationIssue::SubOptimal(
                        JunitTestCaseValidationIssueSubOptimal::TestCaseStaleTimestamp(timestamp),
                    ));
                }
            } else {
                test_case_validation.add_issue(JunitValidationIssue::SubOptimal(
                    JunitTestCaseValidationIssueSubOptimal::TestCaseNoTimestamp,
                ));
            }

            test_suite_validation.test_cases.push(test_case_validation);
        }

        report_validation.test_suites.push(test_suite_validation);
    }

    report_validation
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(eq, get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JunitReportValidation {
    test_suites: Vec<JunitTestSuiteValidation>,
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JunitReportValidationFlatIssue {
    pub level: JunitValidationLevel,
    pub error_message: String,
}

#[cfg_attr(feature = "pyo3", pymethods)]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
impl JunitReportValidation {
    pub fn test_suites_owned(&self) -> Vec<JunitTestSuiteValidation> {
        self.test_suites.clone()
    }

    pub fn max_level(&self) -> JunitValidationLevel {
        self.test_suites
            .iter()
            .map(|test_suite| test_suite.max_level())
            .max()
            .unwrap_or(JunitValidationLevel::Valid)
    }

    pub fn test_suites_max_level(&self) -> Option<JunitValidationLevel> {
        self.test_suites
            .iter()
            .map(|test_suite| test_suite.level)
            .max()
    }
}

impl JunitReportValidation {
    pub fn test_suites(&self) -> &[JunitTestSuiteValidation] {
        &self.test_suites
    }

    pub fn test_cases_flat(&self) -> Vec<&JunitTestCaseValidation> {
        self.test_suites
            .iter()
            .flat_map(|test_suite| test_suite.test_cases())
            .collect()
    }

    pub fn test_suite_validation_issues_flat(&self) -> Vec<&JunitTestSuiteValidationIssue> {
        self.test_suites
            .iter()
            .flat_map(|test_suite| test_suite.issues())
            .collect()
    }

    pub fn test_case_validation_issues_flat(&self) -> Vec<&JunitTestCaseValidationIssue> {
        self.test_suites
            .iter()
            .flat_map(|test_suite| test_suite.test_cases())
            .flat_map(|test_case| test_case.issues())
            .collect()
    }

    fn test_suite_variant_validation_issues_flat(
        &self,
        level: JunitValidationLevel,
    ) -> Vec<&JunitTestSuiteValidationIssue> {
        self.test_suites
            .iter()
            .flat_map(|test_suite| test_suite.issues())
            .filter(|test_suite_validation_issue| {
                JunitValidationLevel::from(*test_suite_validation_issue) == level
            })
            .collect()
    }

    pub fn test_suite_suboptimal_validation_issues_flat(
        &self,
    ) -> Vec<&JunitTestSuiteValidationIssue> {
        self.test_suite_variant_validation_issues_flat(JunitValidationLevel::SubOptimal)
    }

    pub fn test_suite_invalid_validation_issues_flat(&self) -> Vec<&JunitTestSuiteValidationIssue> {
        self.test_suite_variant_validation_issues_flat(JunitValidationLevel::Invalid)
    }

    fn test_case_variant_validation_issues_flat(
        &self,
        level: JunitValidationLevel,
    ) -> Vec<&JunitTestCaseValidationIssue> {
        self.test_suites
            .iter()
            .flat_map(|test_suite| test_suite.test_cases())
            .flat_map(|test_case| test_case.issues())
            .filter(|test_case_validation_issue| {
                JunitValidationLevel::from(*test_case_validation_issue) == level
            })
            .collect()
    }

    pub fn test_case_suboptimal_validation_issues_flat(
        &self,
    ) -> Vec<&JunitTestCaseValidationIssue> {
        self.test_case_variant_validation_issues_flat(JunitValidationLevel::SubOptimal)
    }

    pub fn test_case_invalid_validation_issues_flat(&self) -> Vec<&JunitTestCaseValidationIssue> {
        self.test_case_variant_validation_issues_flat(JunitValidationLevel::Invalid)
    }
}

pub type JunitTestSuiteValidationIssue = JunitValidationIssue<
    JunitTestSuiteValidationIssueSubOptimal,
    JunitTestSuiteValidationIssueInvalid,
>;

impl ToString for JunitTestSuiteValidationIssue {
    fn to_string(&self) -> String {
        match self {
            Self::SubOptimal(i) => i.to_string(),
            Self::Invalid(i) => i.to_string(),
        }
    }
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(eq))]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JunitTestSuiteValidation {
    level: JunitValidationLevel,
    issues: Vec<JunitTestSuiteValidationIssue>,
    test_cases: Vec<JunitTestCaseValidation>,
}

#[cfg_attr(feature = "pyo3", pymethods)]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
impl JunitTestSuiteValidation {
    pub fn level(&self) -> JunitValidationLevel {
        self.level
    }

    pub fn issues_flat(&self) -> Vec<JunitReportValidationFlatIssue> {
        self.issues
            .iter()
            .map(|i| JunitReportValidationFlatIssue {
                level: JunitValidationLevel::from(i),
                error_message: i.to_string(),
            })
            .collect()
    }

    pub fn test_cases_owned(&self) -> Vec<JunitTestCaseValidation> {
        self.test_cases.clone()
    }

    pub fn max_level(&self) -> JunitValidationLevel {
        self.test_cases
            .iter()
            .map(|test_suite| test_suite.level)
            .max()
            .map_or(self.level, |l| l.max(self.level))
    }

    pub fn test_cases_max_level(&self) -> Option<JunitValidationLevel> {
        self.test_cases
            .iter()
            .map(|test_case| test_case.level)
            .max()
    }
}

impl JunitTestSuiteValidation {
    pub fn issues(&self) -> &[JunitTestSuiteValidationIssue] {
        &self.issues
    }

    pub fn test_cases(&self) -> &[JunitTestCaseValidation] {
        &self.test_cases
    }

    fn add_issue(&mut self, issue: JunitTestSuiteValidationIssue) {
        self.level = self.level.max(JunitValidationLevel::from(&issue));
        self.issues.push(issue);
    }
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum JunitTestSuiteValidationIssueSubOptimal {
    #[error("test suite name too long, truncated to {}", MAX_FIELD_LEN)]
    TestSuiteNameTooLong(String),
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum JunitTestSuiteValidationIssueInvalid {
    #[error("test suite name too short")]
    TestSuiteNameTooShort(String),
    #[error("test suite id is not a valid uuidv5")]
    TestSuiteInvalidId(String),
}

pub type JunitTestCaseValidationIssue = JunitValidationIssue<
    JunitTestCaseValidationIssueSubOptimal,
    JunitTestCaseValidationIssueInvalid,
>;

impl ToString for JunitTestCaseValidationIssue {
    fn to_string(&self) -> String {
        match self {
            Self::SubOptimal(i) => i.to_string(),
            Self::Invalid(i) => i.to_string(),
        }
    }
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(eq))]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JunitTestCaseValidation {
    level: JunitValidationLevel,
    issues: Vec<JunitTestCaseValidationIssue>,
}

#[cfg_attr(feature = "pyo3", pymethods)]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
impl JunitTestCaseValidation {
    pub fn level(&self) -> JunitValidationLevel {
        self.level
    }

    pub fn issues_flat(&self) -> Vec<JunitReportValidationFlatIssue> {
        self.issues
            .iter()
            .map(|i| JunitReportValidationFlatIssue {
                level: JunitValidationLevel::from(i),
                error_message: i.to_string(),
            })
            .collect()
    }
}

impl JunitTestCaseValidation {
    pub fn issues(&self) -> &[JunitTestCaseValidationIssue] {
        &self.issues
    }

    fn add_issue(&mut self, issue: JunitTestCaseValidationIssue) {
        self.level = self.level.max(JunitValidationLevel::from(&issue));
        self.issues.push(issue);
    }
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum JunitTestCaseValidationIssueSubOptimal {
    #[error("test case name too long, truncated to {}", MAX_FIELD_LEN)]
    TestCaseNameTooLong(String),
    #[error("test case file or filepath too short")]
    TestCaseFileOrFilepathTooShort(String),
    #[error("test case file or filepath too long")]
    TestCaseFileOrFilepathTooLong(String),
    #[error("test case classname too short")]
    TestCaseClassnameTooShort(String),
    #[error("test case classname too long, truncated to {}", MAX_FIELD_LEN)]
    TestCaseClassnameTooLong(String),
    #[error("test case or parent has no time duration")]
    TestCaseNoTimeDuration,
    #[error("test case or parent has no timestamp")]
    TestCaseNoTimestamp,
    #[error("test case or parent has future timestamp")]
    TestCaseFutureTimestamp(DateTime<FixedOffset>),
    #[error(
        "test case or parent has old (> {} day(s)) timestamp",
        TIMESTAMP_OLD_DAYS
    )]
    TestCaseOldTimestamp(DateTime<FixedOffset>),
    #[error(
        "test case or parent has stale (> {} hour(s)) timestamp",
        TIMESTAMP_STALE_HOURS
    )]
    TestCaseStaleTimestamp(DateTime<FixedOffset>),
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum JunitTestCaseValidationIssueInvalid {
    #[error("test case name too short")]
    TestCaseNameTooShort(String),
    #[error("test case id is not a valid uuidv5")]
    TestCaseInvalidId(String),
}
