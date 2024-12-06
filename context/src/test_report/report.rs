use chrono::prelude::*;
#[cfg(feature = "ruby")]
use magnus::{Module, Object};
use prost_wkt_types::Timestamp;
use proto::test_context::test_run::{TestCaseRun, TestCaseRunStatus, TestResult};
#[cfg(feature = "pyo3")]
use pyo3::{pyclass, pymethods};
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use std::cell::RefCell;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Debug, Clone, PartialEq)]
pub struct TestReport {
    test_result: TestResult,
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(eq))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[cfg_attr(feature = "ruby", magnus::wrap(class = "TestReport"))]
#[derive(Debug, Clone, PartialEq)]
pub struct MutTestReport(RefCell<TestReport>);

#[cfg_attr(feature = "pyo3", gen_stub_pymethods, pymethods)]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
impl MutTestReport {
    pub fn new() -> Self {
        Self(RefCell::new(TestReport {
            test_result: TestResult::default(),
        }))
    }

    // sends out to the trunk api
    pub fn publish(&self) {
        println!("Publish");
    }

    // saves to local fs and prints the path
    pub fn save(&self) {}

    // adds a test to the test report
    pub fn add_test(
        &self,
        name: String,
        classname: String,
        file: String,
        parent_name: String,
        line: i32,
        status: String,
        attempt: i32,
        started_at: i64,
        ended_at: i64,
        output: String,
    ) {
        let mut test = TestCaseRun::default();
        test.name = name;
        test.classname = classname;
        test.file = file;
        test.parent_name = parent_name;
        test.line = line;
        match status.as_str() {
            "success" => test.status = TestCaseRunStatus::Success.into(),
            "failure" => test.status = TestCaseRunStatus::Failure.into(),
            "skipped" => test.status = TestCaseRunStatus::Skipped.into(),
            _ => test.status = TestCaseRunStatus::Unspecified.into(),
        }
        // test.status = status;
        test.attempt = attempt;
        let started_at_date_time = DateTime::from_timestamp(started_at, 0).unwrap_or_default();
        let test_started_at = Timestamp {
            seconds: started_at_date_time.timestamp(),
            nanos: started_at_date_time.timestamp_subsec_nanos() as i32,
        };
        test.started_at = Some(test_started_at);
        let ended_at_date_time = DateTime::from_timestamp(ended_at, 0).unwrap_or_default();
        let test_ended_at = Timestamp {
            seconds: ended_at_date_time.timestamp(),
            nanos: ended_at_date_time.timestamp_subsec_nanos() as i32,
        };
        test.ended_at = Some(test_ended_at);
        test.output_message = output;
        self.0
            .borrow_mut()
            .test_result
            .test_case_runs
            .push(test.clone());
    }

    // lists the quarantined tests in the test report
    pub fn list_quarantined_tests(&self) {
        println!("List quarantined");
    }

    // validates the env is set for CI
    pub fn valid_env(&self) {
        println!("Valid env");
    }

    // validates that we are in a git repo
    pub fn valid_git(&self) {
        println!("Valid git");
    }

    pub fn to_string(&self) -> String {
        self.clone().into()
    }
}

impl Into<String> for MutTestReport {
    fn into(self) -> String {
        serde_json::to_string(&self.0.borrow().test_result).unwrap_or_default()
    }
}

#[cfg(feature = "ruby")]
pub fn ruby_init(ruby: &magnus::Ruby) -> Result<(), magnus::Error> {
    let test_report = ruby.define_class("TestReport", ruby.class_object())?;
    test_report.define_singleton_method("new", magnus::function!(MutTestReport::new, 0))?;
    test_report.define_method("to_s", magnus::method!(MutTestReport::to_string, 0))?;
    test_report.define_method("publish", magnus::method!(MutTestReport::publish, 0))?;
    test_report.define_method("save", magnus::method!(MutTestReport::save, 0))?;
    test_report.define_method("add_test", magnus::method!(MutTestReport::add_test, 10))?;
    test_report.define_method(
        "list_quarantined_tests",
        magnus::method!(MutTestReport::list_quarantined_tests, 0),
    )?;
    test_report.define_method("valid_env", magnus::method!(MutTestReport::valid_env, 0))?;
    test_report.define_method("valid_git", magnus::method!(MutTestReport::valid_git, 0))?;
    Ok(())
}
