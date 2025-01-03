use std::{cell::RefCell, env, fs};

use chrono::prelude::*;
#[cfg(feature = "ruby")]
use magnus::{value::ReprValue, Module, Object};
use prost_wkt_types::Timestamp;
use proto::test_context::test_run::{TestCaseRun, TestCaseRunStatus, TestResult, UploaderMetadata};
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::wasm_bindgen;

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
    Unspecified,
}

const SUCCESS: &str = "success";
const FAILURE: &str = "failure";
const SKIPPED: &str = "skipped";
const UNSPECIFIED: &str = "unspecified";

#[cfg(feature = "ruby")]
impl Status {
    fn new(status: String) -> Self {
        match status.as_str() {
            SUCCESS => Status::Success,
            FAILURE => Status::Failure,
            SKIPPED => Status::Skipped,
            _ => Status::Unspecified,
        }
    }
}

impl Into<&str> for Status {
    fn into(self) -> &'static str {
        match self {
            Status::Success => SUCCESS,
            Status::Failure => FAILURE,
            Status::Skipped => SKIPPED,
            Status::Unspecified => UNSPECIFIED,
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
            version: env!("CARGO_PKG_VERSION").to_string(),
            upload_time: None,
        });
        Self(RefCell::new(TestReport { test_result }))
    }

    fn serialize_test_result(&self) -> Vec<u8> {
        prost::Message::encode_to_vec(&self.0.borrow().test_result)
    }

    // sends out to the trunk api
    pub fn publish(&self, repo_root: String) -> bool {
        let path = self.save();
        if path.is_err() {
            return false;
        }
        let resolved_path = if let Ok(path) = self.save() {
            path
        } else {
            return false;
        };
        let resolved_path_str = resolved_path.path().to_str().unwrap_or_default();
        let token = env::var("TRUNK_API_TOKEN").unwrap_or_default();
        let org_url_slug = env::var("TRUNK_ORG_URL_SLUG").unwrap_or_default();
        if token.is_empty() || org_url_slug.is_empty() {
            println!("Token or org url slug not set");
            return false;
        }
        if let Some(uploader_metadata) = &mut self.0.borrow_mut().test_result.uploader_metadata {
            uploader_metadata.upload_time = Some(Timestamp {
                seconds: Utc::now().timestamp(),
                nanos: Utc::now().timestamp_subsec_nanos() as i32,
            });
        }
        // TODO - handle finding the repo root automatically
        let upload_args = trunk_analytics_cli::upload::UploadArgs::new(
            token,
            org_url_slug,
            vec![resolved_path_str.into()],
            repo_root,
        );
        match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(trunk_analytics_cli::upload::run_upload(
                upload_args,
                None,
                None,
                None,
                None,
            )) {
            Ok(_) => return true,
            Err(e) => {
                println!("Error uploading: {:?}", e);
                return false;
            }
        }
    }

    // saves to local fs and returns the path
    fn save(&self) -> Result<tempfile::NamedTempFile, anyhow::Error> {
        let buf = self.serialize_test_result();
        let named_temp_file = tempfile::Builder::new().suffix(".bin").tempfile()?;
        fs::write(&named_temp_file, buf).unwrap_or_default();
        Ok(named_temp_file)
    }

    // adds a test to the test report
    pub fn add_test(
        &self,
        id: Option<String>,
        name: String,
        classname: String,
        file: String,
        parent_name: String,
        line: Option<i32>,
        status: Status,
        attempt_number: i32,
        started_at: i64,
        finished_at: i64,
        output: String,
    ) {
        let mut test = TestCaseRun::default();
        if let Some(id) = id {
            test.id = id;
        }
        test.name = name;
        test.classname = classname;
        test.file = file;
        test.parent_name = parent_name;
        if let Some(line) = line {
            test.line = line;
        }
        match status {
            Status::Success => test.status = TestCaseRunStatus::Success.into(),
            Status::Failure => test.status = TestCaseRunStatus::Failure.into(),
            Status::Skipped => test.status = TestCaseRunStatus::Skipped.into(),
            Status::Unspecified => test.status = TestCaseRunStatus::Unspecified.into(),
        }
        // test.status = status;
        test.attempt_number = attempt_number;
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
        test.status_output_message = output;
        self.0
            .borrow_mut()
            .test_result
            .test_case_runs
            .push(test.clone());
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
    test_report.define_method("publish", magnus::method!(MutTestReport::publish, 1))?;
    test_report.define_method("add_test", magnus::method!(MutTestReport::add_test, 11))?;
    Ok(())
}
