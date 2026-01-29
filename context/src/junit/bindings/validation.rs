#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::junit::validator::TestRunnerReportValidation;
use crate::junit::{
    bindings::suite::BindingsTestSuite,
    validator::{
        JunitReportValidation, JunitReportValidationFlatIssue, JunitTestSuiteValidation,
        JunitValidationLevel, JunitValidationType,
    },
};

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BindingsJunitReportValidation {
    pub(crate) all_issues: Vec<JunitReportValidationFlatIssue>,
    pub(crate) level: JunitValidationLevel,
    pub(crate) test_runner_report: TestRunnerReportValidation,
    pub(crate) test_suites: Vec<JunitTestSuiteValidation>,
    pub(crate) valid_test_suites: Vec<BindingsTestSuite>,
}

impl From<JunitReportValidation> for BindingsJunitReportValidation {
    fn from(
        JunitReportValidation {
            all_issues,
            level,
            test_suites,
            valid_test_suites,
            test_runner_report,
        }: JunitReportValidation,
    ) -> Self {
        Self {
            all_issues: all_issues
                .into_iter()
                .map(|i| JunitReportValidationFlatIssue {
                    level: JunitValidationLevel::from(&i),
                    error_type: JunitValidationType::from(&i),
                    error_message: i.to_string(),
                })
                .collect(),
            level,
            test_suites,
            valid_test_suites: valid_test_suites.into_iter().collect(),
            test_runner_report,
        }
    }
}

impl From<BindingsJunitReportValidation> for JunitReportValidation {
    fn from(
        BindingsJunitReportValidation {
            all_issues: _,
            level,
            test_runner_report,
            test_suites,
            valid_test_suites,
        }: BindingsJunitReportValidation,
    ) -> Self {
        let mut validation = Self {
            all_issues: Vec::new(),
            level,
            test_runner_report,
            test_suites,
            valid_test_suites,
        };
        validation.derive_all_issues();
        validation
    }
}

#[cfg_attr(feature = "pyo3", gen_stub_pymethods, pymethods)]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
impl BindingsJunitReportValidation {
    pub fn all_issues_owned(&self) -> Vec<JunitReportValidationFlatIssue> {
        self.all_issues.clone()
    }

    pub fn max_level(&self) -> JunitValidationLevel {
        self.test_suites
            .iter()
            .map(|test_suite| test_suite.max_level())
            .max()
            .map_or(self.level, |l| l.max(self.level))
    }

    pub fn num_invalid_issues(&self) -> usize {
        self.all_issues
            .iter()
            .filter(|issue| issue.level == JunitValidationLevel::Invalid)
            .count()
    }

    pub fn num_suboptimal_issues(&self) -> usize {
        self.all_issues
            .iter()
            .filter(|issue| issue.level == JunitValidationLevel::SubOptimal)
            .count()
    }
}
