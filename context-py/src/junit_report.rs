use std::{collections::HashMap, time::Duration};

use chrono::DateTime;
use pyo3::{exceptions::PyTypeError, prelude::*};
use quick_junit::{
    NonSuccessKind, Property, Report, TestCase, TestCaseStatus, TestRerun, TestSuite,
};

#[pyclass]
#[derive(Clone, Debug)]
pub struct PyReport {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub uuid: Option<String>,
    #[pyo3(get)]
    pub timestamp: Option<i64>,
    #[pyo3(get)]
    pub time: Option<f64>,
    #[pyo3(get)]
    pub tests: usize,
    #[pyo3(get)]
    pub failures: usize,
    #[pyo3(get)]
    pub errors: usize,
    #[pyo3(get)]
    pub test_suites: Vec<PyTestSuite>,
}

impl From<Report> for PyReport {
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
            time: time.map(|t| t.as_secs_f64()),
            tests,
            failures,
            errors,
            test_suites: test_suites.into_iter().map(PyTestSuite::from).collect(),
        }
    }
}

impl Into<Report> for PyReport {
    fn into(self) -> Report {
        let Self {
            name,
            uuid,
            timestamp,
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
            timestamp: timestamp
                .and_then(|secs| DateTime::from_timestamp(secs, 0))
                .map(|dt| dt.fixed_offset()),
            time: time.map(|secs| Duration::from_secs_f64(secs)),
            tests,
            failures,
            errors,
            test_suites: test_suites.into_iter().map(PyTestSuite::into).collect(),
        }
    }
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct PyTestSuite {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub tests: usize,
    #[pyo3(get)]
    pub disabled: usize,
    #[pyo3(get)]
    pub errors: usize,
    #[pyo3(get)]
    pub failures: usize,
    #[pyo3(get)]
    pub timestamp: Option<i64>,
    #[pyo3(get)]
    pub time: Option<f64>,
    #[pyo3(get)]
    pub test_cases: Vec<PyTestCase>,
    #[pyo3(get)]
    pub properties: Vec<PyProperty>,
    #[pyo3(get)]
    pub system_out: Option<String>,
    #[pyo3(get)]
    pub system_err: Option<String>,
    #[pyo3(get)]
    pub extra: HashMap<String, String>,
}

impl From<TestSuite> for PyTestSuite {
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
            time: time.map(|t| t.as_secs_f64()),
            test_cases: test_cases.into_iter().map(PyTestCase::from).collect(),
            properties: properties.into_iter().map(PyProperty::from).collect(),
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

impl Into<TestSuite> for PyTestSuite {
    fn into(self) -> TestSuite {
        let Self {
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
        } = self;
        let mut test_suite = TestSuite::new(name);
        test_suite.tests = tests;
        test_suite.disabled = disabled;
        test_suite.errors = errors;
        test_suite.failures = failures;
        test_suite.timestamp = timestamp
            .and_then(|secs| DateTime::from_timestamp(secs, 0))
            .map(|dt| dt.fixed_offset());
        test_suite.time = time.map(|secs| Duration::from_secs_f64(secs));
        test_suite.test_cases = test_cases
            .into_iter()
            .map(PyTestCase::try_into)
            .filter_map(|t| {
                // Removes any invalid test cases that could not be parsed correctly
                t.ok()
            })
            .collect();
        test_suite.properties = properties.into_iter().map(PyProperty::into).collect();
        test_suite.system_out = system_out.map(|s| s.into());
        test_suite.system_err = system_err.map(|s| s.into());
        test_suite.extra = extra
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        test_suite
    }
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct PyProperty {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub value: String,
}

impl From<Property> for PyProperty {
    fn from(Property { name, value }: Property) -> Self {
        Self {
            name: name.to_string(),
            value: value.to_string(),
        }
    }
}

impl Into<Property> for PyProperty {
    fn into(self) -> Property {
        let Self { name, value } = self;
        Property {
            name: name.into(),
            value: value.into(),
        }
    }
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct PyTestCase {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub classname: Option<String>,
    #[pyo3(get)]
    pub assertions: Option<usize>,
    #[pyo3(get)]
    pub timestamp: Option<i64>,
    #[pyo3(get)]
    pub time: Option<f64>,
    #[pyo3(get)]
    pub status: PyTestCaseStatus,
    #[pyo3(get)]
    pub system_out: Option<String>,
    #[pyo3(get)]
    pub system_err: Option<String>,
    #[pyo3(get)]
    pub extra: HashMap<String, String>,
    #[pyo3(get)]
    pub properties: Vec<PyProperty>,
}

impl From<TestCase> for PyTestCase {
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
            time: time.map(|t| t.as_secs_f64()),
            status: PyTestCaseStatus::from(status),
            system_out: system_out.map(|s| s.to_string()),
            system_err: system_err.map(|s| s.to_string()),
            extra: HashMap::from_iter(
                extra
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string())),
            ),
            properties: properties.into_iter().map(PyProperty::from).collect(),
        }
    }
}

impl TryInto<TestCase> for PyTestCase {
    type Error = PyErr;

    fn try_into(self) -> Result<TestCase, PyErr> {
        let Self {
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
        } = self;
        let mut test_case = TestCase::new(name, status.try_into()?);
        test_case.classname = classname.map(|c| c.into());
        test_case.assertions = assertions;
        test_case.timestamp = timestamp
            .and_then(|secs| DateTime::from_timestamp(secs, 0))
            .map(|dt| dt.fixed_offset());
        test_case.time = time.map(|secs| Duration::from_secs_f64(secs));
        test_case.system_out = system_out.map(|s| s.into());
        test_case.system_err = system_err.map(|s| s.into());
        test_case.extra = extra
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        test_case.properties = properties.into_iter().map(PyProperty::into).collect();
        Ok(test_case)
    }
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct PyTestCaseStatus {
    #[pyo3(get)]
    pub status: PyTestCaseStatusStatus,
    #[pyo3(get)]
    pub success: Option<PyTestCaseStatusSuccess>,
    #[pyo3(get)]
    pub non_success: Option<PyTestCaseStatusNonSuccess>,
    #[pyo3(get)]
    pub skipped: Option<PyTestCaseStatusSkipped>,
}

impl From<TestCaseStatus> for PyTestCaseStatus {
    fn from(value: TestCaseStatus) -> Self {
        match value {
            TestCaseStatus::Success { flaky_runs } => Self {
                status: PyTestCaseStatusStatus::Success,
                success: Some(PyTestCaseStatusSuccess {
                    flaky_runs: flaky_runs.into_iter().map(PyTestRerun::from).collect(),
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
                status: PyTestCaseStatusStatus::NonSuccess,
                success: None,
                non_success: Some(PyTestCaseStatusNonSuccess {
                    kind: PyNonSuccessKind::from(kind),
                    message: message.map(|m| m.into_string()),
                    ty: ty.map(|t| t.into_string()),
                    description: description.map(|d| d.into_string()),
                    reruns: reruns.into_iter().map(PyTestRerun::from).collect(),
                }),
                skipped: None,
            },
            TestCaseStatus::Skipped {
                message,
                ty,
                description,
            } => Self {
                status: PyTestCaseStatusStatus::Skipped,
                success: None,
                non_success: None,
                skipped: Some(PyTestCaseStatusSkipped {
                    message: message.map(|m| m.into_string()),
                    ty: ty.map(|t| t.into_string()),
                    description: description.map(|d| d.into_string()),
                }),
            },
        }
    }
}

impl TryInto<TestCaseStatus> for PyTestCaseStatus {
    type Error = PyErr;

    fn try_into(self) -> Result<TestCaseStatus, PyErr> {
        let Self {
            status,
            success,
            non_success,
            skipped,
        } = self;
        match (status, success, non_success, skipped) {
            (PyTestCaseStatusStatus::Success, Some(success_fields), None, None) => {
                Ok(success_fields.into())
            }
            (PyTestCaseStatusStatus::NonSuccess, None, Some(non_success_fields), None) => {
                Ok(non_success_fields.into())
            }
            (PyTestCaseStatusStatus::Skipped, None, None, Some(skipped_fields)) => {
                Ok(skipped_fields.into())
            }
            _ => Err(PyTypeError::new_err(
                "Could not convert PyTestCaseStatus into TestCaseStatus",
            )),
        }
    }
}

#[pyclass]
#[derive(Clone, Debug)]
pub enum PyTestCaseStatusStatus {
    Success,
    NonSuccess,
    Skipped,
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct PyTestCaseStatusSuccess {
    #[pyo3(get)]
    pub flaky_runs: Vec<PyTestRerun>,
}

impl Into<TestCaseStatus> for PyTestCaseStatusSuccess {
    fn into(self) -> TestCaseStatus {
        let Self { flaky_runs } = self;
        TestCaseStatus::Success {
            flaky_runs: flaky_runs.into_iter().map(PyTestRerun::into).collect(),
        }
    }
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct PyTestCaseStatusNonSuccess {
    #[pyo3(get)]
    pub kind: PyNonSuccessKind,
    #[pyo3(get)]
    pub message: Option<String>,
    #[pyo3(get)]
    pub ty: Option<String>,
    #[pyo3(get)]
    pub description: Option<String>,
    #[pyo3(get)]
    pub reruns: Vec<PyTestRerun>,
}

impl Into<TestCaseStatus> for PyTestCaseStatusNonSuccess {
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
            reruns: reruns.into_iter().map(PyTestRerun::into).collect(),
        }
    }
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct PyTestCaseStatusSkipped {
    #[pyo3(get)]
    message: Option<String>,
    #[pyo3(get)]
    ty: Option<String>,
    #[pyo3(get)]
    description: Option<String>,
}

impl Into<TestCaseStatus> for PyTestCaseStatusSkipped {
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

#[pyclass]
#[derive(Clone, Debug)]
pub struct PyTestRerun {
    #[pyo3(get)]
    pub kind: PyNonSuccessKind,
    #[pyo3(get)]
    pub timestamp: Option<i64>,
    #[pyo3(get)]
    pub time: Option<f64>,
    #[pyo3(get)]
    pub message: Option<String>,
    #[pyo3(get)]
    pub ty: Option<String>,
    #[pyo3(get)]
    pub stack_trace: Option<String>,
    #[pyo3(get)]
    pub system_out: Option<String>,
    #[pyo3(get)]
    pub system_err: Option<String>,
    #[pyo3(get)]
    pub description: Option<String>,
}

impl From<TestRerun> for PyTestRerun {
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
            kind: PyNonSuccessKind::from(kind),
            timestamp: timestamp.map(|t| t.timestamp()),
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

impl Into<TestRerun> for PyTestRerun {
    fn into(self) -> TestRerun {
        let Self {
            kind,
            timestamp,
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
            timestamp: timestamp
                .and_then(|secs| DateTime::from_timestamp(secs, 0))
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

#[pyclass]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PyNonSuccessKind {
    Failure,
    Error,
}

impl From<NonSuccessKind> for PyNonSuccessKind {
    fn from(value: NonSuccessKind) -> Self {
        match value {
            NonSuccessKind::Failure => PyNonSuccessKind::Failure,
            NonSuccessKind::Error => PyNonSuccessKind::Error,
        }
    }
}

impl Into<NonSuccessKind> for PyNonSuccessKind {
    fn into(self) -> NonSuccessKind {
        match self {
            PyNonSuccessKind::Failure => NonSuccessKind::Failure,
            PyNonSuccessKind::Error => NonSuccessKind::Error,
        }
    }
}
