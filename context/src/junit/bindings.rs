use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, TimeDelta};
use proto::test_context::test_run::{TestCaseRun, TestCaseRunStatus, TestResult};
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pymethods};
use quick_junit::{
    NonSuccessKind, Property, Report, TestCase, TestCaseStatus, TestRerun, TestSuite,
};
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use super::{
    parser::JunitParseFlatIssue,
    validator::{
        JunitReportValidation, JunitReportValidationFlatIssue, JunitTestSuiteValidation,
        JunitValidationLevel, JunitValidationType,
    },
};
use crate::junit::{parser::extra_attrs, validator::TestRunnerReportValidation};

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BindingsParseResult {
    pub report: Option<BindingsReport>,
    pub issues: Vec<JunitParseFlatIssue>,
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
}

impl From<TestCaseRunStatus> for BindingsTestCaseStatusStatus {
    fn from(value: TestCaseRunStatus) -> Self {
        match value {
            TestCaseRunStatus::Success => BindingsTestCaseStatusStatus::Success,
            TestCaseRunStatus::Failure => BindingsTestCaseStatusStatus::NonSuccess,
            TestCaseRunStatus::Skipped => BindingsTestCaseStatusStatus::Skipped,
            TestCaseRunStatus::Unspecified => BindingsTestCaseStatusStatus::Unspecified,
        }
    }
}

impl From<TestResult> for BindingsReport {
    fn from(
        TestResult {
            test_case_runs,
            uploader_metadata,
        }: TestResult,
    ) -> Self {
        let test_cases: Vec<BindingsTestCase> = test_case_runs
            .into_iter()
            .map(BindingsTestCase::from)
            .collect();
        let parent_name_map: HashMap<String, Vec<BindingsTestCase>> =
            test_cases.iter().fold(HashMap::new(), |mut acc, testcase| {
                if let Some(parent_name) = testcase.extra.get("parent_name") {
                    acc.entry(parent_name.clone())
                        .or_default()
                        .push(testcase.to_owned());
                }
                acc
            });
        let test_suites: Vec<BindingsTestSuite> = parent_name_map
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
        }
    }
}

impl From<TestCaseRun> for BindingsTestCase {
    fn from(
        TestCaseRun {
            name,
            parent_name,
            classname,
            started_at,
            finished_at,
            status,
            status_output_message,
            id,
            file,
            line,
            attempt_number,
            is_quarantined,
            codeowners,
        }: TestCaseRun,
    ) -> Self {
        let started_at = started_at.unwrap_or_default();
        let timestamp = chrono::DateTime::from(started_at.clone());
        let timestamp_micros = chrono::DateTime::from(started_at).timestamp_micros();
        let time = (chrono::DateTime::from(finished_at.unwrap_or_default()) - timestamp)
            .to_std()
            .unwrap_or_default();
        let classname = if classname.is_empty() {
            None
        } else {
            Some(classname)
        };
        let typed_status =
            TestCaseRunStatus::try_from(status).unwrap_or(TestCaseRunStatus::Unspecified);
        Self {
            name,
            classname,
            codeowners: Some(codeowners.iter().map(|c| c.name.to_owned()).collect()),
            assertions: None,
            timestamp: Some(timestamp.timestamp()),
            timestamp_micros: Some(timestamp_micros),
            time: Some(time.as_secs_f64()),
            status: BindingsTestCaseStatus {
                status: typed_status.into(),
                success: {
                    if typed_status == TestCaseRunStatus::Success {
                        Some(BindingsTestCaseStatusSuccess { flaky_runs: vec![] })
                    } else {
                        None
                    }
                },
                non_success: {
                    if typed_status == TestCaseRunStatus::Failure {
                        Some(BindingsTestCaseStatusNonSuccess {
                            kind: BindingsNonSuccessKind::Failure,
                            message: Some(status_output_message.clone()),
                            ty: None,
                            description: None,
                            reruns: vec![],
                        })
                    } else {
                        None
                    }
                },
                skipped: {
                    if typed_status == TestCaseRunStatus::Skipped {
                        Some(BindingsTestCaseStatusSkipped {
                            message: Some(status_output_message.clone()),
                            ty: None,
                            description: None,
                        })
                    } else {
                        None
                    }
                },
            },
            system_err: None,
            system_out: None,
            extra: HashMap::from([
                ("id".to_string(), id.to_string()),
                ("file".to_string(), file),
                ("line".to_string(), line.to_string()),
                ("attempt_number".to_string(), attempt_number.to_string()),
                ("parent_name".to_string(), parent_name),
                ("is_quarantined".to_string(), is_quarantined.to_string()),
            ]),
            properties: vec![],
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

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BindingsTestSuite {
    pub name: String,
    pub tests: usize,
    pub disabled: usize,
    pub errors: usize,
    pub failures: usize,
    pub timestamp: Option<i64>,
    pub timestamp_micros: Option<i64>,
    pub time: Option<f64>,
    pub test_cases: Vec<BindingsTestCase>,
    pub properties: Vec<BindingsProperty>,
    pub system_out: Option<String>,
    pub system_err: Option<String>,
    extra: HashMap<String, String>,
}

#[cfg(feature = "pyo3")]
#[gen_stub_pymethods]
#[pymethods]
impl BindingsTestSuite {
    fn py_extra(&self) -> HashMap<String, String> {
        self.extra.clone()
    }
}

impl BindingsTestSuite {
    pub fn extra(&self) -> HashMap<String, String> {
        self.extra.clone()
    }
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl BindingsTestSuite {
    pub fn js_extra(&self) -> Result<js_sys::Object, wasm_bindgen::JsValue> {
        let entries = self
            .extra
            .iter()
            .fold(js_sys::Array::new(), |acc, (key, value)| {
                let entry = js_sys::Array::new();
                entry.push(&js_sys::JsString::from(key.as_str()));
                entry.push(&js_sys::JsString::from(value.as_str()));
                acc.push(&entry);
                acc
            });
        js_sys::Object::from_entries(&entries)
    }
}

impl From<TestSuite> for BindingsTestSuite {
    fn from(
        TestSuite {
            name,
            tests,
            disabled,
            errors,
            failures,
            timestamp,
            time,
            test_cases,
            properties,
            system_out,
            system_err,
            extra,
            // NOTE: The above should be all fields, but here may be more added in the future due to
            // `#[non_exhaustive]`
            ..
        }: TestSuite,
    ) -> Self {
        let file = extra.get(extra_attrs::FILE);
        let filepath = extra.get(extra_attrs::FILEPATH);
        let test_cases = test_cases
            .into_iter()
            .map(|mut tc| {
                if let Some(file) = file {
                    tc.extra.insert(extra_attrs::FILE.into(), file.clone());
                }
                if let Some(filepath) = filepath {
                    tc.extra
                        .insert(extra_attrs::FILEPATH.into(), filepath.clone());
                }
                BindingsTestCase::from(tc)
            })
            .collect();
        Self {
            name: name.into_string(),
            tests,
            disabled,
            errors,
            failures,
            timestamp: timestamp.map(|t| t.timestamp()),
            timestamp_micros: timestamp.map(|t| t.timestamp_micros()),
            time: time.map(|t| t.as_secs_f64()),
            test_cases,
            properties: properties.into_iter().map(BindingsProperty::from).collect(),
            system_out: system_out.map(|s| s.to_string()),
            system_err: system_err.map(|s| s.to_string()),
            extra: HashMap::from_iter(
                extra
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string())),
            ),
        }
    }
}

impl From<BindingsTestSuite> for TestSuite {
    fn from(val: BindingsTestSuite) -> Self {
        let BindingsTestSuite {
            name,
            tests,
            disabled,
            errors,
            failures,
            timestamp: _,
            timestamp_micros,
            time,
            test_cases,
            properties,
            system_out,
            system_err,
            extra,
        } = val;
        let mut test_suite = TestSuite::new(name);
        test_suite.tests = tests;
        test_suite.disabled = disabled;
        test_suite.errors = errors;
        test_suite.failures = failures;
        test_suite.timestamp = timestamp_micros
            .and_then(|micro_secs| {
                let micros_delta = TimeDelta::microseconds(micro_secs);
                DateTime::from_timestamp(
                    micros_delta.num_seconds(),
                    micros_delta.subsec_nanos() as u32,
                )
            })
            .map(|dt| dt.fixed_offset());
        test_suite.time = time.map(Duration::from_secs_f64);
        let file = test_suite.extra.get(extra_attrs::FILE);
        let filepath = test_suite.extra.get(extra_attrs::FILEPATH);
        test_suite.test_cases = test_cases
            .into_iter()
            .map(|mut tc| {
                if let Some(file) = file {
                    tc.extra.insert(extra_attrs::FILE.into(), file.to_string());
                }
                if let Some(filepath) = filepath {
                    tc.extra
                        .insert(extra_attrs::FILEPATH.into(), filepath.to_string());
                }
                BindingsTestCase::try_into(tc)
            })
            .filter_map(|t| {
                // Removes any invalid test cases that could not be parsed correctly
                t.ok()
            })
            .collect();
        test_suite.properties = properties.into_iter().map(BindingsProperty::into).collect();
        test_suite.system_out = system_out.map(|s| s.into());
        test_suite.system_err = system_err.map(|s| s.into());
        test_suite.extra = extra
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        test_suite
    }
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BindingsProperty {
    pub name: String,
    pub value: String,
}

impl From<Property> for BindingsProperty {
    fn from(Property { name, value }: Property) -> Self {
        Self {
            name: name.to_string(),
            value: value.to_string(),
        }
    }
}

impl From<BindingsProperty> for Property {
    fn from(val: BindingsProperty) -> Self {
        let BindingsProperty { name, value } = val;
        Property {
            name: name.into(),
            value: value.into(),
        }
    }
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BindingsTestCase {
    pub name: String,
    pub classname: Option<String>,
    pub assertions: Option<usize>,
    pub timestamp: Option<i64>,
    pub timestamp_micros: Option<i64>,
    pub time: Option<f64>,
    pub status: BindingsTestCaseStatus,
    pub system_out: Option<String>,
    pub system_err: Option<String>,
    pub codeowners: Option<Vec<String>>,
    extra: HashMap<String, String>,
    pub properties: Vec<BindingsProperty>,
}

#[cfg(feature = "pyo3")]
#[gen_stub_pymethods]
#[pymethods]
impl BindingsTestCase {
    fn py_extra(&self) -> HashMap<String, String> {
        self.extra.clone()
    }
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
impl BindingsTestCase {
    pub fn js_extra(&self) -> Result<js_sys::Object, wasm_bindgen::JsValue> {
        let entries = self
            .extra
            .iter()
            .fold(js_sys::Array::new(), |acc, (key, value)| {
                let entry = js_sys::Array::new();
                entry.push(&js_sys::JsString::from(key.as_str()));
                entry.push(&js_sys::JsString::from(value.as_str()));
                acc.push(&entry);
                acc
            });
        js_sys::Object::from_entries(&entries)
    }
}

impl BindingsTestCase {
    pub fn extra(&self) -> HashMap<String, String> {
        self.extra.clone()
    }

    pub fn is_quarantined(&self) -> bool {
        self.extra
            .get("is_quarantined")
            .map_or(false, |v| v == "true")
    }
}

impl From<TestCase> for BindingsTestCase {
    fn from(
        TestCase {
            name,
            classname,
            assertions,
            timestamp,
            time,
            status,
            system_out,
            system_err,
            extra,
            properties,
            // NOTE: The above should be all fields, but here may be more added in the future due to
            // `#[non_exhaustive]`
            ..
        }: TestCase,
    ) -> Self {
        Self {
            name: name.into_string(),
            classname: classname.map(|c| c.to_string()),
            assertions,
            timestamp: timestamp.map(|t| t.timestamp()),
            timestamp_micros: timestamp.map(|t| t.timestamp_micros()),
            time: time.map(|t| t.as_secs_f64()),
            status: BindingsTestCaseStatus::from(status),
            system_out: system_out.map(|s| s.to_string()),
            system_err: system_err.map(|s| s.to_string()),
            extra: HashMap::from_iter(
                extra
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string())),
            ),
            properties: properties.into_iter().map(BindingsProperty::from).collect(),
            codeowners: None,
        }
    }
}

impl TryInto<TestCase> for BindingsTestCase {
    type Error = ();

    fn try_into(self) -> Result<TestCase, Self::Error> {
        let Self {
            name,
            classname,
            assertions,
            codeowners: _,
            timestamp: _,
            timestamp_micros,
            time,
            status,
            system_out,
            system_err,
            extra,
            properties,
        } = self;
        let mut test_case = TestCase::new(name, status.try_into()?);
        test_case.classname = classname.map(|c| c.into());
        test_case.assertions = assertions;
        test_case.timestamp = timestamp_micros
            .and_then(|micro_secs| {
                let micros_delta = TimeDelta::microseconds(micro_secs);
                DateTime::from_timestamp(
                    micros_delta.num_seconds(),
                    micros_delta.subsec_nanos() as u32,
                )
            })
            .map(|dt| dt.fixed_offset());
        test_case.time = time.map(Duration::from_secs_f64);
        test_case.system_out = system_out.map(|s| s.into());
        test_case.system_err = system_err.map(|s| s.into());
        test_case.extra = extra
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        test_case.properties = properties.into_iter().map(BindingsProperty::into).collect();
        Ok(test_case)
    }
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BindingsTestCaseStatus {
    pub status: BindingsTestCaseStatusStatus,
    pub success: Option<BindingsTestCaseStatusSuccess>,
    pub non_success: Option<BindingsTestCaseStatusNonSuccess>,
    pub skipped: Option<BindingsTestCaseStatusSkipped>,
}

impl From<TestCaseStatus> for BindingsTestCaseStatus {
    fn from(value: TestCaseStatus) -> Self {
        match value {
            TestCaseStatus::Success { flaky_runs } => Self {
                status: BindingsTestCaseStatusStatus::Success,
                success: Some(BindingsTestCaseStatusSuccess {
                    flaky_runs: flaky_runs
                        .into_iter()
                        .map(BindingsTestRerun::from)
                        .collect(),
                }),
                non_success: None,
                skipped: None,
            },
            TestCaseStatus::NonSuccess {
                kind,
                message,
                ty,
                description,
                reruns,
            } => Self {
                status: BindingsTestCaseStatusStatus::NonSuccess,
                success: None,
                non_success: Some(BindingsTestCaseStatusNonSuccess {
                    kind: BindingsNonSuccessKind::from(kind),
                    message: message.map(|m| m.into_string()),
                    ty: ty.map(|t| t.into_string()),
                    description: description.map(|d| d.into_string()),
                    reruns: reruns.into_iter().map(BindingsTestRerun::from).collect(),
                }),
                skipped: None,
            },
            TestCaseStatus::Skipped {
                message,
                ty,
                description,
            } => Self {
                status: BindingsTestCaseStatusStatus::Skipped,
                success: None,
                non_success: None,
                skipped: Some(BindingsTestCaseStatusSkipped {
                    message: message.map(|m| m.into_string()),
                    ty: ty.map(|t| t.into_string()),
                    description: description.map(|d| d.into_string()),
                }),
            },
        }
    }
}

impl TryInto<TestCaseStatus> for BindingsTestCaseStatus {
    type Error = ();

    fn try_into(self) -> Result<TestCaseStatus, Self::Error> {
        let Self {
            status,
            success,
            non_success,
            skipped,
        } = self;
        match (status, success, non_success, skipped) {
            (BindingsTestCaseStatusStatus::Success, Some(success_fields), None, None) => {
                Ok(success_fields.into())
            }
            (BindingsTestCaseStatusStatus::NonSuccess, None, Some(non_success_fields), None) => {
                Ok(non_success_fields.into())
            }
            (BindingsTestCaseStatusStatus::Skipped, None, None, Some(skipped_fields)) => {
                Ok(skipped_fields.into())
            }
            _ => Err(()),
        }
    }
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass_enum, pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BindingsTestCaseStatusStatus {
    Success,
    NonSuccess,
    Skipped,
    Unspecified,
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BindingsTestCaseStatusSuccess {
    pub flaky_runs: Vec<BindingsTestRerun>,
}

impl From<BindingsTestCaseStatusSuccess> for TestCaseStatus {
    fn from(val: BindingsTestCaseStatusSuccess) -> Self {
        let BindingsTestCaseStatusSuccess { flaky_runs } = val;
        TestCaseStatus::Success {
            flaky_runs: flaky_runs
                .into_iter()
                .map(BindingsTestRerun::into)
                .collect(),
        }
    }
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BindingsTestCaseStatusNonSuccess {
    pub kind: BindingsNonSuccessKind,
    pub message: Option<String>,
    pub ty: Option<String>,
    pub description: Option<String>,
    pub reruns: Vec<BindingsTestRerun>,
}

impl From<BindingsTestCaseStatusNonSuccess> for TestCaseStatus {
    fn from(val: BindingsTestCaseStatusNonSuccess) -> Self {
        let BindingsTestCaseStatusNonSuccess {
            kind,
            message,
            ty,
            description,
            reruns,
        } = val;
        TestCaseStatus::NonSuccess {
            kind: kind.into(),
            message: message.map(|m| m.into()),
            ty: ty.map(|t| t.into()),
            description: description.map(|d| d.into()),
            reruns: reruns.into_iter().map(BindingsTestRerun::into).collect(),
        }
    }
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BindingsTestCaseStatusSkipped {
    pub message: Option<String>,
    pub ty: Option<String>,
    pub description: Option<String>,
}

impl From<BindingsTestCaseStatusSkipped> for TestCaseStatus {
    fn from(val: BindingsTestCaseStatusSkipped) -> Self {
        let BindingsTestCaseStatusSkipped {
            message,
            ty,
            description,
        } = val;
        TestCaseStatus::Skipped {
            message: message.map(|m| m.into()),
            ty: ty.map(|t| t.into()),
            description: description.map(|d| d.into()),
        }
    }
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BindingsTestRerun {
    pub kind: BindingsNonSuccessKind,
    pub timestamp: Option<i64>,
    pub timestamp_micros: Option<i64>,
    pub time: Option<f64>,
    pub message: Option<String>,
    pub ty: Option<String>,
    pub stack_trace: Option<String>,
    pub system_out: Option<String>,
    pub system_err: Option<String>,
    pub description: Option<String>,
}

impl From<TestRerun> for BindingsTestRerun {
    fn from(
        TestRerun {
            kind,
            timestamp,
            time,
            message,
            ty,
            stack_trace,
            system_out,
            system_err,
            description,
        }: TestRerun,
    ) -> Self {
        Self {
            kind: BindingsNonSuccessKind::from(kind),
            timestamp: timestamp.map(|t| t.timestamp()),
            timestamp_micros: timestamp.map(|t| t.timestamp_micros()),
            time: time.map(|t| t.as_secs_f64()),
            message: message.map(|m| m.to_string()),
            ty: ty.map(|t| t.to_string()),
            stack_trace: stack_trace.map(|st| st.to_string()),
            system_out: system_out.map(|s| s.to_string()),
            system_err: system_err.map(|s| s.to_string()),
            description: description.map(|d| d.to_string()),
        }
    }
}

impl From<BindingsTestRerun> for TestRerun {
    fn from(val: BindingsTestRerun) -> Self {
        let BindingsTestRerun {
            kind,
            timestamp: _,
            timestamp_micros,
            time,
            message,
            ty,
            stack_trace,
            system_out,
            system_err,
            description,
        } = val;
        TestRerun {
            kind: kind.into(),
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
            message: message.map(|m| m.into()),
            ty: ty.map(|t| t.into()),
            stack_trace: stack_trace.map(|st| st.into()),
            system_out: system_out.map(|s| s.into()),
            system_err: system_err.map(|s| s.into()),
            description: description.map(|d| d.into()),
        }
    }
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass_enum, pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BindingsNonSuccessKind {
    Failure,
    Error,
}

impl From<NonSuccessKind> for BindingsNonSuccessKind {
    fn from(value: NonSuccessKind) -> Self {
        match value {
            NonSuccessKind::Failure => BindingsNonSuccessKind::Failure,
            NonSuccessKind::Error => BindingsNonSuccessKind::Error,
        }
    }
}

impl From<BindingsNonSuccessKind> for NonSuccessKind {
    fn from(val: BindingsNonSuccessKind) -> Self {
        match val {
            BindingsNonSuccessKind::Failure => NonSuccessKind::Failure,
            BindingsNonSuccessKind::Error => NonSuccessKind::Error,
        }
    }
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BindingsJunitReportValidation {
    all_issues: Vec<JunitReportValidationFlatIssue>,
    level: JunitValidationLevel,
    test_runner_report: TestRunnerReportValidation,
    test_suites: Vec<JunitTestSuiteValidation>,
    valid_test_suites: Vec<BindingsTestSuite>,
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
            valid_test_suites: valid_test_suites
                .into_iter()
                .map(BindingsTestSuite::from)
                .collect(),
            test_runner_report,
        }
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

#[cfg(test)]
mod tests {
    use std::io::BufReader;

    use proto::test_context::test_run::{CodeOwner, TestCaseRun, TestCaseRunStatus, TestResult};

    use crate::junit::bindings::BindingsReport;
    use crate::junit::parser::JunitParser;
    use crate::junit::validator::{JunitValidationLevel, JunitValidationType};

    #[cfg(feature = "bindings")]
    #[test]
    fn parse_quick_junit_to_bindings() {
        use std::io::BufReader;

        use crate::junit::parser::JunitParser;
        const INPUT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="my-test-run" tests="2" failures="1" errors="0">
    <testsuite name="my-test-suite" file="path/to/my/test.js" tests="2" disabled="0" errors="0" failures="1">
        <testcase name="success-case">
        </testcase>
        <testcase name="failure-case">
            <failure/>
        </testcase>
    </testsuite>
</testsuites>
"#;
        let mut junit_parser = JunitParser::new();
        junit_parser
            .parse(BufReader::new(INPUT_XML.as_bytes()))
            .unwrap();
        let reports = junit_parser.into_reports();
        assert_eq!(reports.len(), 1);
        let bindings_report = BindingsReport::from(reports[0].clone());
        assert_eq!(bindings_report.name, "my-test-run");
        assert_eq!(bindings_report.tests, 2);
        assert_eq!(bindings_report.failures, 1);
        assert_eq!(bindings_report.errors, 0);
        assert_eq!(bindings_report.test_suites.len(), 1);
        let test_suite = &bindings_report.test_suites[0];
        assert_eq!(test_suite.name, "my-test-suite");
        assert_eq!(test_suite.tests, 2);
        assert_eq!(test_suite.disabled, 0);
        assert_eq!(test_suite.errors, 0);
        assert_eq!(test_suite.failures, 1);
        assert_eq!(test_suite.test_cases.len(), 2);
        let test_case1 = &test_suite.test_cases[0];
        assert_eq!(test_case1.name, "success-case");
        assert_eq!(test_case1.classname, None);
        assert_eq!(test_case1.assertions, None);
        assert_eq!(test_case1.timestamp, None);
        assert_eq!(test_case1.timestamp_micros, None);
        assert_eq!(test_case1.time, None);
        assert_eq!(test_case1.system_out, None);
        assert_eq!(test_case1.system_err, None);
        assert_eq!(test_case1.extra.len(), 1);
        assert_eq!(test_case1.extra["file"], "path/to/my/test.js");
        assert_eq!(test_case1.properties.len(), 0);
        let test_case2 = &test_suite.test_cases[1];
        assert_eq!(test_case2.name, "failure-case");
        assert_eq!(test_case2.classname, None);
        assert_eq!(test_case2.assertions, None);
        assert_eq!(test_case2.timestamp, None);
        assert_eq!(test_case2.timestamp_micros, None);
        assert_eq!(test_case2.time, None);
        assert_eq!(test_case2.system_out, None);
        assert_eq!(test_case2.system_err, None);
        assert_eq!(test_case2.extra.len(), 1);
        assert_eq!(test_case2.extra["file"], "path/to/my/test.js");
        assert_eq!(test_case2.properties.len(), 0);
    }

    #[cfg(feature = "bindings")]
    #[test]
    fn parse_test_report_to_bindings() {
        use prost_wkt_types::Timestamp;
        use tempfile::TempDir;

        use crate::{junit::validator::validate, repo::BundleRepo};

        let temp_dir = TempDir::with_prefix("not-hidden").unwrap();
        let test_started_at = Timestamp {
            seconds: 1000,
            nanos: 0,
        };
        let test_finished_at = Timestamp {
            seconds: 2000,
            nanos: 0,
        };
        let codeowner1 = CodeOwner {
            name: "@user".into(),
        };
        let test_file = temp_dir.path().join("test_file");
        let file_str = String::from(test_file.as_os_str().to_str().unwrap());
        let test1 = TestCaseRun {
            id: "test_id1".into(),
            name: "test_name".into(),
            classname: "test_classname".into(),
            file: file_str.clone(),
            parent_name: "test_parent_name1".into(),
            line: 1,
            status: TestCaseRunStatus::Success.into(),
            attempt_number: 1,
            started_at: Some(test_started_at.clone()),
            finished_at: Some(test_finished_at.clone()),
            status_output_message: "test_status_output_message".into(),
            codeowners: vec![codeowner1],
            ..Default::default()
        };

        let test2 = TestCaseRun {
            id: "test_id2".into(),
            name: "test_name".into(),
            classname: "test_classname".into(),
            file: file_str,
            parent_name: "test_parent_name2".into(),
            line: 1,
            status: TestCaseRunStatus::Failure.into(),
            attempt_number: 1,
            started_at: Some(test_started_at.clone()),
            finished_at: Some(test_finished_at),
            status_output_message: "test_status_output_message".into(),
            ..Default::default()
        };

        let mut test_result = TestResult::default();
        test_result.test_case_runs.push(test1.clone());
        test_result.test_case_runs.push(test1.clone());
        test_result.test_case_runs.push(test2.clone());

        let converted_bindings: BindingsReport = test_result.into();
        assert_eq!(converted_bindings.test_suites.len(), 2);
        let mut test_suite1 = &converted_bindings.test_suites[0];
        let mut test_suite2 = &converted_bindings.test_suites[1];
        if test_suite1.name == "test_parent_name1" {
            assert_eq!(test_suite1.tests, 2);
            assert_eq!(test_suite2.tests, 1);
        } else {
            assert_eq!(test_suite1.tests, 1);
            assert_eq!(test_suite2.tests, 2);
            // swap them for convenience
            (test_suite1, test_suite2) = (test_suite2, test_suite1);
        }
        let test_case1 = &test_suite1.test_cases[0];
        assert_eq!(test_case1.name, test1.name);
        assert_eq!(test_case1.classname, Some(test1.classname));
        assert_eq!(test_case1.assertions, None);
        assert_eq!(
            test_case1.timestamp,
            Some(test1.started_at.clone().unwrap().seconds)
        );
        assert_eq!(
            test_case1.timestamp_micros,
            Some(
                test1.started_at.clone().unwrap().seconds * 1000000
                    + test1.started_at.unwrap().nanos as i64 / 1000
            )
        );
        assert_eq!(test_case1.time, Some(1000.0));
        assert_eq!(test_case1.system_out, None);
        assert_eq!(test_case1.system_err, None);
        assert_eq!(test_case1.extra["id"], test1.id);
        assert_eq!(test_case1.extra["file"], test1.file);
        assert_eq!(test_case1.extra["line"], test1.line.to_string());
        assert_eq!(
            test_case1.extra["attempt_number"],
            test1.attempt_number.to_string()
        );
        assert_eq!(test_case1.properties.len(), 0);
        assert_eq!(test_case1.codeowners.clone().unwrap().len(), 1);
        assert_eq!(test_case1.codeowners.clone().unwrap()[0], "@user");

        assert_eq!(test_suite2.test_cases.len(), 1);
        let test_case2 = &test_suite2.test_cases[0];
        assert_eq!(test_case2.name, test2.name);
        assert_eq!(test_case2.classname, Some(test2.classname));
        assert_eq!(test_case2.assertions, None);
        assert_eq!(
            test_case2.timestamp,
            Some(test2.started_at.clone().unwrap().seconds)
        );
        assert_eq!(
            test_case2.timestamp_micros,
            Some(
                test2.started_at.clone().unwrap().seconds * 1000000
                    + test2.started_at.unwrap().nanos as i64 / 1000
            )
        );
        assert_eq!(test_case2.time, Some(1000.0));
        assert_eq!(test_case2.system_out, None);
        assert_eq!(test_case2.system_err, None);
        assert_eq!(test_case2.extra["id"], test2.id);
        assert_eq!(test_case2.extra["file"], test2.file);
        assert_eq!(test_case2.extra["line"], test2.line.to_string());
        assert_eq!(
            test_case2.extra["attempt_number"],
            test2.attempt_number.to_string()
        );
        assert_eq!(test_case2.properties.len(), 0);
        assert_eq!(test_case2.codeowners.clone().unwrap().len(), 0);

        // verify that the test report is valid
        let results = validate(
            &converted_bindings.clone().into(),
            None,
            &BundleRepo::default(),
        );
        assert_eq!(results.all_issues_flat().len(), 1);
        results
            .all_issues_flat()
            .sort_by(|a, b| a.error_message.cmp(&b.error_message));
        results
            .all_issues_flat()
            .iter()
            .enumerate()
            .for_each(|issue| {
                assert_eq!(issue.1.level, JunitValidationLevel::SubOptimal);
                if issue.0 == 0 {
                    assert_eq!(issue.1.error_type, JunitValidationType::Report);
                    assert_eq!(
                        issue.1.error_message,
                        "report has old (> 24 hour(s)) timestamps"
                    );
                } else {
                    assert_eq!(issue.1.error_type, JunitValidationType::TestCase);
                    assert_eq!(issue.1.error_message, "test case id is not a valid uuidv5");
                }
            });
        assert_eq!(results.test_suites.len(), 2);
        assert_eq!(results.valid_test_suites.len(), 2);
        assert_eq!(
            results.valid_test_suites[0].test_cases.len(),
            converted_bindings.test_suites[0].tests
        );
        assert_eq!(
            results.valid_test_suites[1].test_cases.len(),
            converted_bindings.test_suites[1].tests
        );
    }
    #[cfg(feature = "bindings")]
    #[test]
    fn test_junit_conversion_paths() {
        use crate::repo::BundleRepo;

        let mut junit_parser = JunitParser::new();
        let file_contents = r#"
        <xml version="1.0" encoding="UTF-8"?>
        <testsuites>
            <testsuite name="testsuite" time="0.002">
                <testcase file="test.java" line="5" timestamp="2023-10-01T12:00:00Z" classname="test" name="test_variant_truncation1" time="0.001">
                    <failure message="Test failed" type="java.lang.AssertionError">
                        <![CDATA[Expected: <true> but was: <false>]]>
                    </failure>
                </testcase>
                <testcase file="test.java" name="test_variant_truncation2" timestamp="2023-10-01T12:00:00Z" time="0.001" />
            </testsuite>
        </testsuites>
        "#;
        let parsed_results = junit_parser.parse(BufReader::new(file_contents.as_bytes()));
        assert!(parsed_results.is_ok());

        // Get test case runs from parser
        let test_case_runs = junit_parser.into_test_case_runs(None, &BundleRepo::default());
        assert_eq!(test_case_runs.len(), 2);

        // Convert test case runs to bindings
        let bindings_from_runs: Vec<crate::junit::bindings::BindingsTestCase> =
            test_case_runs.into_iter().map(|run| run.into()).collect();

        // Get reports and convert directly to bindings
        let mut junit_parser = JunitParser::new();
        junit_parser
            .parse(BufReader::new(file_contents.as_bytes()))
            .unwrap();
        let reports = junit_parser.into_reports();
        assert_eq!(reports.len(), 1);

        let bindings_from_reports: Vec<crate::junit::bindings::BindingsTestCase> = reports[0]
            .test_suites
            .iter()
            .flat_map(|suite| suite.test_cases.iter().map(|case| case.clone().into()))
            .collect();

        // Compare the two conversion paths
        assert_eq!(bindings_from_runs.len(), bindings_from_reports.len());

        for (run_binding, report_binding) in
            bindings_from_runs.iter().zip(bindings_from_reports.iter())
        {
            assert_eq!(run_binding.name, report_binding.name);
            assert_eq!(run_binding.classname, report_binding.classname);
            assert_eq!(run_binding.status.status, report_binding.status.status);
            assert_eq!(run_binding.timestamp, report_binding.timestamp);
            assert_eq!(
                run_binding.timestamp_micros,
                report_binding.timestamp_micros
            );
            assert_eq!(run_binding.time, report_binding.time);
            assert_eq!(run_binding.system_out, report_binding.system_out);
            assert_eq!(run_binding.system_err, report_binding.system_err);
            // check that the properties match
            for property in run_binding.properties.iter() {
                if let Some(report_property) = report_binding
                    .properties
                    .iter()
                    .find(|p| p.name == property.name)
                {
                    assert_eq!(property.value, report_property.value);
                } else {
                    panic!("Property {} not found in report binding", property.name);
                }
            }
            assert_eq!(
                run_binding.extra().get("file"),
                report_binding.extra().get("file")
            );
        }
    }
}
