use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, TimeDelta};
use indexmap::IndexMap;
use proto::test_context::test_run::{TestBuildResult, TestResult};
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum};
use quick_junit::Report;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::junit::{
    bindings::{
        suite::BindingsTestSuite,
        test_case::{BindingsTestCase, BindingsTestCaseStatusStatus},
    },
    parser::JunitParseFlatIssue,
};

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BindingsParseResult {
    pub report: Option<BindingsReport>,
    pub issues: Vec<JunitParseFlatIssue>,
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass_enum, pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum BindingsTestBuildResult {
    Unspecified,
    Success,
    Failure,
    Skipped,
    Flaky,
}

// Ideally this would be an enum, but enums are not directly supportted by wasm conversions, so we have to manually map out the options. See: https://github.com/rustwasm/wasm-bindgen/issues/2407
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BazelBuildInformation {
    pub label: String,
    pub result: BindingsTestBuildResult,
    pub max_attempt_number: Option<i32>,
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BindingsReport {
    pub name: String,
    pub uuid: Option<String>,
    pub timestamp: Option<i64>,
    pub timestamp_micros: Option<i64>,
    pub time: Option<f64>,
    pub tests: usize,
    pub failures: usize,
    pub errors: usize,
    pub test_suites: Vec<BindingsTestSuite>,
    pub variant: Option<String>,
    pub bazel_build_information: Option<BazelBuildInformation>,
}

pub fn map_i32_to_bindings_test_build_result(
    result: i32,
) -> anyhow::Result<BindingsTestBuildResult> {
    if result == TestBuildResult::Unspecified as i32 {
        Ok(BindingsTestBuildResult::Unspecified)
    } else if result == TestBuildResult::Success as i32 {
        Ok(BindingsTestBuildResult::Success)
    } else if result == TestBuildResult::Failure as i32 {
        Ok(BindingsTestBuildResult::Failure)
    } else if result == TestBuildResult::Skipped as i32 {
        Ok(BindingsTestBuildResult::Skipped)
    } else if result == TestBuildResult::Flaky as i32 {
        Ok(BindingsTestBuildResult::Flaky)
    } else {
        Err(anyhow::anyhow!("Unknown TestBuildResult: {result}"))
    }
}

impl From<TestResult> for BindingsReport {
    fn from(
        TestResult {
            test_case_runs,
            // trunk-ignore(clippy/deprecated)
            uploader_metadata,
            test_build_information,
        }: TestResult,
    ) -> Self {
        // An `IndexMap` keeps suites in the order their first test case appears, so converting
        // the same `TestResult` twice gives the same suite order. Ingestion hashes each run's
        // position into its id, so a random order re-writes a redelivered upload's runs under
        // new ids.
        let parent_names = test_case_runs.into_iter().map(BindingsTestCase::from).fold(
            IndexMap::<String, Vec<BindingsTestCase>>::new(),
            |mut acc, testcase| {
                if let Some(parent_name) = testcase.extra.get("parent_name") {
                    acc.entry(parent_name.clone()).or_default().push(testcase);
                }
                acc
            },
        );
        let test_suites: Vec<BindingsTestSuite> = parent_names
            .into_iter()
            .map(|(name, testcases)| {
                let tests = testcases.len();
                let disabled = testcases
                    .iter()
                    .filter(|tc| tc.status.status == BindingsTestCaseStatusStatus::Skipped)
                    .count();
                let failures = testcases
                    .iter()
                    .filter(|tc| tc.status.status == BindingsTestCaseStatusStatus::NonSuccess)
                    .count();
                let timestamp = testcases.iter().map(|tc| tc.timestamp.unwrap_or(0)).max();
                let timestamp_micros = testcases
                    .iter()
                    .map(|tc| tc.timestamp_micros.unwrap_or(0))
                    .max();
                let time = testcases.iter().map(|tc| tc.time.unwrap_or(0.0)).sum();
                BindingsTestSuite {
                    name,
                    tests,
                    disabled,
                    errors: 0,
                    failures,
                    timestamp,
                    timestamp_micros,
                    time: Some(time),
                    test_cases: testcases,
                    properties: vec![],
                    system_out: None,
                    system_err: None,
                    extra: HashMap::new(),
                }
            })
            .collect();
        let (report_time, report_failures, report_tests) =
            test_suites.iter().fold((0.0, 0, 0), |acc, ts| {
                (
                    acc.0 + ts.time.unwrap_or(0.0),
                    acc.1 + ts.failures,
                    acc.2 + ts.tests,
                )
            });
        let (name, timestamp, timestamp_micros, variant) = match uploader_metadata {
            Some(t) => {
                let upload_time = t.upload_time.clone().unwrap_or_default();
                (
                    t.origin,
                    Some(upload_time.seconds),
                    Some(
                        chrono::Duration::nanoseconds(upload_time.nanos as i64)
                            .num_microseconds()
                            .unwrap_or_default(),
                    ),
                    Some(t.variant),
                )
            }
            None => ("Unknown".to_string(), None, None, None),
        };
        let bazel_build_information = match test_build_information {
            Some(proto::test_context::test_run::test_result::TestBuildInformation::BazelBuildInformation(
                bazel_build_information,
            )) => Some(BazelBuildInformation {
                label: bazel_build_information.label,
                result: map_i32_to_bindings_test_build_result(bazel_build_information.result)
                    .unwrap_or(BindingsTestBuildResult::Unspecified),
                max_attempt_number: bazel_build_information.max_attempt_number.map(|number| number.number),
            }),
            _ => None,
        };
        BindingsReport {
            name,
            test_suites,
            time: Some(report_time),
            uuid: None,
            timestamp,
            timestamp_micros,
            errors: 0,
            failures: report_failures,
            tests: report_tests,
            variant,
            bazel_build_information,
        }
    }
}

impl From<Report> for BindingsReport {
    fn from(
        Report {
            name,
            uuid,
            timestamp,
            time,
            tests,
            failures,
            errors,
            test_suites,
        }: Report,
    ) -> Self {
        Self {
            name: name.into_string(),
            uuid: uuid.map(|u| u.to_string()),
            timestamp: timestamp.map(|t| t.timestamp()),
            timestamp_micros: timestamp.map(|t| t.timestamp_micros()),
            time: time.map(|t| t.as_secs_f64()),
            tests,
            failures,
            errors,
            test_suites: test_suites
                .into_iter()
                .map(BindingsTestSuite::from)
                .collect(),
            variant: None,
            bazel_build_information: None,
        }
    }
}

impl From<BindingsReport> for Report {
    fn from(val: BindingsReport) -> Self {
        let BindingsReport {
            name,
            uuid,
            timestamp: _,
            timestamp_micros,
            time,
            tests,
            failures,
            errors,
            test_suites,
            variant: _,
            bazel_build_information: _,
        } = val;
        // NOTE: Cannot make a UUID without a `&'static str`
        let _ = uuid;
        Report {
            name: name.into(),
            uuid: None,
            timestamp: timestamp_micros
                .and_then(|micro_secs| {
                    let micros_delta = TimeDelta::microseconds(micro_secs);
                    DateTime::from_timestamp(
                        micros_delta.num_seconds(),
                        micros_delta.subsec_nanos() as u32,
                    )
                })
                .map(|dt| dt.fixed_offset()),
            time: time.map(Duration::from_secs_f64),
            tests,
            failures,
            errors,
            test_suites: test_suites
                .into_iter()
                .map(BindingsTestSuite::into)
                .collect(),
        }
    }
}
