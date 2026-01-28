use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, TimeDelta};
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use quick_junit::TestSuite;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::junit::bindings::test_case::{BindingsProperty, BindingsTestCase};
use crate::junit::parser::extra_attrs;

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
    pub(crate) extra: HashMap<String, String>,
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
