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

use super::validator::{
    JunitReportValidation, JunitReportValidationFlatIssue, JunitTestSuiteValidation,
    JunitValidationLevel, JunitValidationType,
};

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
}

impl From<TestCaseRunStatus> for BindingsTestCaseStatusStatus {
    fn from(value: TestCaseRunStatus) -> Self {
        match value {
            TestCaseRunStatus::Success => BindingsTestCaseStatusStatus::Success,
            TestCaseRunStatus::Failure => BindingsTestCaseStatusStatus::NonSuccess,
            TestCaseRunStatus::Skipped => BindingsTestCaseStatusStatus::Skipped,
            TestCaseRunStatus::Unspecified => todo!(),
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
            .map(|testcase| BindingsTestCase::from(testcase))
            .collect();
        let parent_name_map: HashMap<String, Vec<BindingsTestCase>> =
            test_cases.iter().fold(HashMap::new(), |mut acc, testcase| {
                let parent_name = testcase.extra.get("parent_name").unwrap();
                acc.entry(parent_name.clone())
                    .or_insert_with(Vec::new)
                    .push(testcase.to_owned());
                acc
            });
        let mut report_time = 0.0;
        let mut report_failures = 0;
        let mut report_tests = 0;
        let test_suites = parent_name_map
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
                report_time += time;
                report_failures += failures;
                report_tests += tests;
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
        let (name, timestamp, timestamp_micros) = match uploader_metadata {
            Some(t) => (
                t.origin,
                Some(t.upload_time.unwrap_or_default().seconds),
                Some(t.upload_time.unwrap_or_default().nanos as i64 * 1000),
            ),
            None => ("Unknown".to_string(), None, None),
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
        }: TestCaseRun,
    ) -> Self {
        let (timestamp, timestamp_micros) = match started_at {
            Some(started_at) => (
                Some(started_at.seconds),
                Some(started_at.nanos as i64 * 1000),
            ),
            None => (None, None),
        };
        let time = match (started_at, finished_at) {
            (Some(started_at), Some(finished_at)) => Some(
                (finished_at.seconds - started_at.seconds) as f64
                    + (finished_at.nanos - started_at.nanos) as f64 / 1_000_000_000.0,
            ),
            _ => None,
        };
        let typed_status = TestCaseRunStatus::try_from(status).ok().unwrap();
        Self {
            name,
            classname: Some(classname),
            assertions: None,
            timestamp,
            timestamp_micros,
            time,
            status: BindingsTestCaseStatus {
                status: typed_status.into(),
                success: {
                    match typed_status == TestCaseRunStatus::Success {
                        true => Some(BindingsTestCaseStatusSuccess { flaky_runs: vec![] }),
                        false => None,
                    }
                },
                non_success: match typed_status == TestCaseRunStatus::Success {
                    false => Some(BindingsTestCaseStatusNonSuccess {
                        kind: BindingsNonSuccessKind::Failure,
                        message: Some(status_output_message.clone()),
                        ty: None,
                        description: None,
                        reruns: vec![],
                    }),
                    true => None,
                },
                skipped: match typed_status == TestCaseRunStatus::Skipped {
                    true => Some(BindingsTestCaseStatusSkipped {
                        message: Some(status_output_message.clone()),
                        ty: None,
                        description: None,
                    }),
                    false => None,
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
        }
    }
}

impl Into<Report> for BindingsReport {
    fn into(self) -> Report {
        let Self {
            name,
            uuid,
            timestamp: _,
            timestamp_micros,
            time,
            tests,
            failures,
            errors,
            test_suites,
        } = self;
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
            time: time.map(|secs| Duration::from_secs_f64(secs)),
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
        Self {
            name: name.into_string(),
            tests,
            disabled,
            errors,
            failures,
            timestamp: timestamp.map(|t| t.timestamp()),
            timestamp_micros: timestamp.map(|t| t.timestamp_micros()),
            time: time.map(|t| t.as_secs_f64()),
            test_cases: test_cases.into_iter().map(BindingsTestCase::from).collect(),
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

impl Into<TestSuite> for BindingsTestSuite {
    fn into(self) -> TestSuite {
        let Self {
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
        } = self;
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
        test_suite.time = time.map(|secs| Duration::from_secs_f64(secs));
        test_suite.test_cases = test_cases
            .into_iter()
            .map(BindingsTestCase::try_into)
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

impl Into<Property> for BindingsProperty {
    fn into(self) -> Property {
        let Self { name, value } = self;
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
        test_case.time = time.map(|secs| Duration::from_secs_f64(secs));
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
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BindingsTestCaseStatusSuccess {
    pub flaky_runs: Vec<BindingsTestRerun>,
}

impl Into<TestCaseStatus> for BindingsTestCaseStatusSuccess {
    fn into(self) -> TestCaseStatus {
        let Self { flaky_runs } = self;
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

impl Into<TestCaseStatus> for BindingsTestCaseStatusNonSuccess {
    fn into(self) -> TestCaseStatus {
        let Self {
            kind,
            message,
            ty,
            description,
            reruns,
        } = self;
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

impl Into<TestCaseStatus> for BindingsTestCaseStatusSkipped {
    fn into(self) -> TestCaseStatus {
        let Self {
            message,
            ty,
            description,
        } = self;
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

impl Into<TestRerun> for BindingsTestRerun {
    fn into(self) -> TestRerun {
        let Self {
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
        } = self;
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
            time: time.map(|secs| Duration::from_secs_f64(secs)),
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

impl Into<NonSuccessKind> for BindingsNonSuccessKind {
    fn into(self) -> NonSuccessKind {
        match self {
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

#[cfg(feature = "bindings")]
#[test]
fn parse_test_report_to_bindings() {
    use prost_wkt_types::Timestamp;
    let mut test1 = TestCaseRun::default();
    test1.id = "test_id1".into();
    test1.name = "test_name".into();
    test1.classname = "test_classname".into();
    test1.file = "test_file".into();
    test1.parent_name = "test_parent_name1".into();
    test1.line = 1;
    test1.status = TestCaseRunStatus::Success.into();
    test1.attempt_number = 1;
    let test_started_at = Timestamp {
        seconds: 1000,
        nanos: 0,
    };
    test1.started_at = Some(test_started_at);
    let test_finished_at = Timestamp {
        seconds: 2000,
        nanos: 0,
    };
    test1.finished_at = Some(test_finished_at);
    test1.status_output_message = "test_status_output_message".into();

    let mut test2 = TestCaseRun::default();
    test2.id = "test_id2".into();
    test2.name = "test_name".into();
    test2.classname = "test_classname".into();
    test2.file = "test_file".into();
    test2.parent_name = "test_parent_name2".into();
    test2.line = 1;
    test2.status = TestCaseRunStatus::Failure.into();
    test2.attempt_number = 1;
    let test_started_at = Timestamp {
        seconds: 1000,
        nanos: 0,
    };
    test2.started_at = Some(test_started_at);
    let test_finished_at = Timestamp {
        seconds: 2000,
        nanos: 0,
    };
    test2.finished_at = Some(test_finished_at);
    test2.status_output_message = "test_status_output_message".into();

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
        Some(test1.started_at.unwrap().seconds)
    );
    assert_eq!(
        test_case1.timestamp_micros,
        Some(test1.started_at.unwrap().nanos as i64)
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

    assert_eq!(test_suite2.test_cases.len(), 1);
    let test_case2 = &test_suite2.test_cases[0];
    assert_eq!(test_case2.name, test2.name);
    assert_eq!(test_case2.classname, Some(test2.classname));
    assert_eq!(test_case2.assertions, None);
    assert_eq!(
        test_case2.timestamp,
        Some(test2.started_at.unwrap().seconds)
    );
    assert_eq!(
        test_case2.timestamp_micros,
        Some(test2.started_at.unwrap().nanos as i64)
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
}
