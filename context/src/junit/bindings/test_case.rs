use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, TimeDelta};
use proto::test_context::test_run::{TestCaseRun, TestCaseRunStatus};
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pymethods};
use quick_junit::{NonSuccessKind, Property, TestCase, TestCaseStatus, TestRerun};
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

fn non_empty_option(s: Option<&str>) -> Option<String> {
    s.filter(|s| !s.is_empty()).map(|s| s.to_string())
}

struct TimestampWrapper {
    datetime: chrono::DateTime<chrono::Utc>,
    timestamp: i64,
    timestamp_micros: i64,
}

impl From<prost_wkt_types::Timestamp> for TimestampWrapper {
    fn from(value: prost_wkt_types::Timestamp) -> Self {
        let datetime = chrono::DateTime::from(value.clone());
        TimestampWrapper {
            datetime,
            timestamp: datetime.timestamp(),
            timestamp_micros: datetime.timestamp_micros(),
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
            // trunk-ignore(clippy/deprecated)
            status_output_message,
            id,
            file,
            // trunk-ignore(clippy/deprecated)
            line,
            // trunk-ignore(clippy/deprecated)
            attempt_number,
            is_quarantined,
            codeowners,
            attempt_index,
            line_number,
            test_output,
            test_runner_information,
        }: TestCaseRun,
    ) -> Self {
        let started_at = started_at.unwrap_or_default();
        let started_at_wrapper = TimestampWrapper::from(started_at);
        let time = (chrono::DateTime::from(finished_at.unwrap_or_default())
            - started_at_wrapper.datetime)
            .to_std()
            .unwrap_or_default();
        let classname = if classname.is_empty() {
            None
        } else {
            Some(classname)
        };
        let typed_status =
            TestCaseRunStatus::try_from(status).unwrap_or(TestCaseRunStatus::Unspecified);

        let mut extra = HashMap::from([
            ("id".to_string(), id.to_string()),
            ("file".to_string(), file),
            ("parent_name".to_string(), parent_name),
            ("is_quarantined".to_string(), is_quarantined.to_string()),
        ]);

        if let Some(line_number) = &line_number {
            extra.insert("line".to_string(), line_number.number.to_string());
        } else if line != 0 {
            // Handle deprecated field
            extra.insert("line".to_string(), line.to_string());
        }

        if let Some(attempt_index) = &attempt_index {
            extra.insert(
                "attempt_number".to_string(),
                attempt_index.number.to_string(),
            );
        } else if attempt_number != 0 {
            // Handle deprecated field
            extra.insert("attempt_number".to_string(), attempt_number.to_string());
        }

        Self {
          name,
          classname,
          codeowners: Some(codeowners.iter().map(|c| c.name.to_owned()).collect()),
          assertions: None,
          timestamp: Some(started_at_wrapper.timestamp),
          timestamp_micros: Some(started_at_wrapper.timestamp_micros),
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
                          message: non_empty_option(
                              test_output
                                  .as_ref()
                                  .map(|fi| fi.message.as_str())
                                  .or(Some(status_output_message.as_str())),
                          ),
                          ty: None,
                          description: non_empty_option(
                              test_output.as_ref().map(|fi| fi.text.as_str()),
                          ),
                          reruns: vec![],
                      })
                  } else {
                      None
                  }
              },
              skipped: {
                  if typed_status == TestCaseRunStatus::Skipped {
                      Some(BindingsTestCaseStatusSkipped {
                          message: non_empty_option(
                              test_output
                                  .as_ref()
                                  .map(|fi| fi.message.as_str())
                                  .or(Some(status_output_message.as_str())),
                          ),
                          ty: None,
                          description: non_empty_option(
                              test_output.as_ref().map(|fi| fi.text.as_str()),
                          ),
                      })
                  } else {
                      None
                  }
              },
          },
          system_err: non_empty_option(test_output.as_ref().map(|fi| fi.system_err.as_str())),
          system_out: non_empty_option(test_output.as_ref().map(|fi| fi.system_out.as_str())),
          extra,
          properties: vec![],
          bazel_run_information: match test_runner_information {
              Some(proto::test_context::test_run::test_case_run::TestRunnerInformation::BazelRunInformation(
                  bazel_run_information,
              )) => {
                  let started_at_wrapper = TimestampWrapper::from(bazel_run_information.started_at.unwrap_or_default());
                  let finished_at_wrapper = TimestampWrapper::from(bazel_run_information.finished_at.unwrap_or_default());

                  Some(BindingsBazelRunInformation {
                      label: bazel_run_information.label,
                      attempt_number: bazel_run_information.attempt_number,
                      started_at: Some(started_at_wrapper.timestamp),
                      started_at_micros: Some(started_at_wrapper.timestamp_micros),
                      finished_at: Some(finished_at_wrapper.timestamp),
                      finished_at_micros: Some(finished_at_wrapper.timestamp_micros),
                  })
              },
              _ => None,
          },
      }
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
pub struct BindingsBazelRunInformation {
    pub label: String,
    pub attempt_number: i32,
    pub started_at: Option<i64>,
    pub started_at_micros: Option<i64>,
    pub finished_at: Option<i64>,
    pub finished_at_micros: Option<i64>,
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
    pub(crate) extra: HashMap<String, String>,
    pub properties: Vec<BindingsProperty>,
    pub bazel_run_information: Option<BindingsBazelRunInformation>,
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
            .is_some_and(|v| v == "true")
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
            bazel_run_information: None,
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
            bazel_run_information: _,
        } = self;
        // donotland: anything here?
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
