use chrono::prelude::*;
#[cfg(feature = "ruby")]
use magnus::{value::ReprValue, Module, Object};
use prost_wkt_types::Timestamp;
use proto::test_context::test_run::{TestCaseRun, TestCaseRunStatus, TestResult, UploaderMetadata};
use std::cell::RefCell;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::wasm_bindgen;

const VERSION: &str = "0.0.1";

#[derive(Debug, Clone, PartialEq)]
pub struct TestReport {
    test_result: TestResult,
}

#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[cfg_attr(feature = "ruby", magnus::wrap(class = "Status"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Success,
    Failure,
    Skipped,
}

#[cfg(feature = "ruby")]
impl Status {
    fn new(status: String) -> Self {
        match status.as_str() {
            "success" => Status::Success,
            "failure" => Status::Failure,
            "skipped" => Status::Skipped,
            _ => panic!("invalid Status: {}", status),
        }
    }
}

impl Into<&str> for Status {
    fn into(self) -> &'static str {
        match self {
            Status::Success => "success",
            Status::Failure => "failure",
            Status::Skipped => "skipped",
        }
    }
}

impl ToString for Status {
    fn to_string(&self) -> String {
        String::from(Into::<&str>::into(*self))
    }
}

#[cfg(feature = "ruby")]
impl Status {
    pub fn to_string(&self) -> &str {
        (*self).into()
    }
}

#[cfg(feature = "ruby")]
impl magnus::TryConvert for Status {
    fn try_convert(val: magnus::Value) -> Result<Self, magnus::Error> {
        let sval: String = val.funcall("to_s", ())?;
        Ok(Status::new(sval))
    }
}

#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[cfg_attr(feature = "ruby", magnus::wrap(class = "TestReport"))]
#[derive(Debug, Clone, PartialEq)]
pub struct MutTestReport(RefCell<TestReport>);

#[cfg_attr(feature = "wasm", wasm_bindgen)]
impl MutTestReport {
    pub fn new(origin: String) -> Self {
        let mut test_result = TestResult::default();
        test_result.uploader_metadata = Some(UploaderMetadata {
            origin,
            version: VERSION.to_string(),
        });
        Self(RefCell::new(TestReport {
            test_result: TestResult::default(),
        }))
    }

    fn serialize_test_result(&self) -> Vec<u8> {
        let test_result = self.0.borrow().test_result.clone();
        let buf: Vec<u8> = prost::Message::encode_to_vec(&test_result);
        buf
    }

    // sends out to the trunk api
    pub fn publish(&self) -> Vec<u8> {
        self.serialize_test_result()
    }

    // saves to local fs and prints the path
    pub fn save(&self) -> String {
        let buf = self.serialize_test_result();
        // TODO - make this random
        let path = "/tmp/test_report.bin";
        std::fs::write(path, buf).unwrap_or_default();
        path.to_string()
    }

    // adds a test to the test report
    pub fn add_test(
        &self,
        id: String,
        name: String,
        classname: String,
        file: String,
        parent_name: String,
        line: i32,
        status: Status,
        attempt: i32,
        started_at: i64,
        finished_at: i64,
        output: String,
    ) {
        let mut test = TestCaseRun::default();
        if !id.is_empty() {
            test.id = id;
        }
        test.name = name;
        test.classname = classname;
        test.file = file;
        test.parent_name = parent_name;
        test.line = line;
        match status {
            Status::Success => test.status = TestCaseRunStatus::Success.into(),
            Status::Failure => test.status = TestCaseRunStatus::Failure.into(),
            Status::Skipped => test.status = TestCaseRunStatus::Skipped.into(),
        }
        // test.status = status;
        test.attempt = attempt;
        let started_at_date_time = DateTime::from_timestamp(started_at, 0).unwrap_or_default();
        let test_started_at = Timestamp {
            seconds: started_at_date_time.timestamp(),
            nanos: started_at_date_time.timestamp_subsec_nanos() as i32,
        };
        test.started_at = Some(test_started_at);
        let finished_at_date_time = DateTime::from_timestamp(finished_at, 0).unwrap_or_default();
        let test_finished_at = Timestamp {
            seconds: finished_at_date_time.timestamp(),
            nanos: finished_at_date_time.timestamp_subsec_nanos() as i32,
        };
        test.finished_at = Some(test_finished_at);
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
    let status = ruby.define_class("Status", ruby.class_object())?;
    status.define_singleton_method("new", magnus::function!(Status::new, 1))?;
    status.define_method("to_s", magnus::method!(Status::to_string, 0))?;
    let test_report = ruby.define_class("TestReport", ruby.class_object())?;
    test_report.define_singleton_method("new", magnus::function!(MutTestReport::new, 1))?;
    test_report.define_method("to_s", magnus::method!(MutTestReport::to_string, 0))?;
    test_report.define_method("publish", magnus::method!(MutTestReport::publish, 0))?;
    test_report.define_method("save", magnus::method!(MutTestReport::save, 0))?;
    test_report.define_method("add_test", magnus::method!(MutTestReport::add_test, 11))?;
    test_report.define_method(
        "list_quarantined_tests",
        magnus::method!(MutTestReport::list_quarantined_tests, 0),
    )?;
    test_report.define_method("valid_env", magnus::method!(MutTestReport::valid_env, 0))?;
    test_report.define_method("valid_git", magnus::method!(MutTestReport::valid_git, 0))?;
    Ok(())
}
