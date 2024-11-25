use std::{collections::HashMap, time::Duration};

use chrono::DateTime;
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum};
use quick_junit::{
    NonSuccessKind, Property, Report, TestCase, TestCaseStatus, TestRerun, TestSuite,
};
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

const MICROSECONDS_PER_SECOND: i64 = 1_000_000;

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
                    DateTime::from_timestamp(
                        micro_secs / MICROSECONDS_PER_SECOND,
                        (micro_secs % MICROSECONDS_PER_SECOND) as u32,
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
                DateTime::from_timestamp(
                    micro_secs / MICROSECONDS_PER_SECOND,
                    (micro_secs % MICROSECONDS_PER_SECOND) as u32,
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
                DateTime::from_timestamp(
                    micro_secs / MICROSECONDS_PER_SECOND,
                    (micro_secs % MICROSECONDS_PER_SECOND) as u32,
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
                    DateTime::from_timestamp(
                        micro_secs / MICROSECONDS_PER_SECOND,
                        (micro_secs % MICROSECONDS_PER_SECOND) as u32,
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
