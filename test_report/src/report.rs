use std::{cell::RefCell, env, fs, io::Write, time::SystemTime};

use bundle::BundleMetaDebugProps;
use chrono::prelude::*;
#[cfg(feature = "ruby")]
use magnus::{value::ReprValue, Module, Object};
use prost_wkt_types::Timestamp;
use proto::test_context::test_run::{TestCaseRun, TestCaseRunStatus, TestResult, UploaderMetadata};
use third_party::sentry;
use trunk_analytics_cli::{context::gather_pre_test_context, upload_command::run_upload};
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Debug, Clone, PartialEq)]
pub struct TestReport {
    test_result: TestResult,
    command: String,
    started_at: SystemTime,
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

impl From<Status> for &str {
    fn from(val: Status) -> Self {
        match val {
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
    pub fn new(origin: String, command: String) -> Self {
        let started_at = SystemTime::now();
        let mut test_result = TestResult::default();
        test_result.uploader_metadata = Some(UploaderMetadata {
            origin: origin.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            upload_time: None,
        });
        Self(RefCell::new(TestReport {
            test_result,
            command,
            started_at,
        }))
    }

    fn serialize_test_result(&self) -> Vec<u8> {
        prost::Message::encode_to_vec(&self.0.borrow().test_result)
    }

    fn setup_logger() -> anyhow::Result<()> {
        let mut builder = env_logger::Builder::new();
        builder
            .format(|buf, record| {
                writeln!(
                    buf,
                    "{} [{}] - {}",
                    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S"),
                    record.level(),
                    record.args()
                )
            })
            .filter_level(log::LevelFilter::Info);
        sentry::logger(builder, log::LevelFilter::Info)
    }

    // sends out to the trunk api
    pub fn publish(&self) -> bool {
        let _guard = sentry::init(None);
        let _logger_setup_res = MutTestReport::setup_logger();
        let resolved_path = if let Ok(path) = self.save() {
            path
        } else {
            return false;
        };
        let resolved_path_str = resolved_path.path().to_str().unwrap_or_default();
        let token = env::var("TRUNK_API_TOKEN").unwrap_or_default();
        let org_url_slug = env::var("TRUNK_ORG_URL_SLUG").unwrap_or_default();
        let repo_root = env::var("REPO_ROOT").ok();
        if token.is_empty() {
            tracing::warn!("Not publishing results because TRUNK_API_TOKEN is empty");
            return false;
        }
        if org_url_slug.is_empty() {
            tracing::warn!("Not publishing results because TRUNK_ORG_URL_SLUG is empty");
            return false;
        }
        if let Some(uploader_metadata) = &mut self.0.borrow_mut().test_result.uploader_metadata {
            uploader_metadata.upload_time = Some(Timestamp {
                seconds: Utc::now().timestamp(),
                nanos: Utc::now().timestamp_subsec_nanos() as i32,
            });
        }
        let upload_args = trunk_analytics_cli::upload_command::UploadArgs::new(
            token,
            org_url_slug,
            vec![resolved_path_str.into()],
            repo_root,
            false,
        );
        let debug_props = BundleMetaDebugProps {
            command_line: self.0.borrow().command.clone(),
        };
        let test_run_result = trunk_analytics_cli::test_command::TestRunResult {
            command: self.0.borrow().command.clone(),
            exec_start: Some(self.0.borrow().started_at),
            exit_code: 0,
            num_tests: Some(self.0.borrow().test_result.test_case_runs.len()),
        };
        if let Ok(pre_test_context) = gather_pre_test_context(upload_args.clone(), debug_props) {
            match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(run_upload(
                    upload_args,
                    Some(pre_test_context),
                    Some(test_run_result),
                )) {
                Ok(_) => true,
                Err(e) => {
                    tracing::warn!("Error uploading: {:?}", e);
                    false
                }
            }
        } else {
            tracing::warn!("Error gathering pre test context");
            false
        }
    }

    // saves to local fs and returns the path
    fn save(&self) -> Result<tempfile::NamedTempFile, anyhow::Error> {
        let buf = self.serialize_test_result();
        let named_temp_file = tempfile::Builder::new().suffix(".bin").tempfile()?;
        fs::write(&named_temp_file, buf)?;
        // file modification uses filetime which is less precise than systemTime
        // we need to update it to the current time to avoid race conditions later down the line
        // when the start time ends up being after the file modification time
        let file = fs::File::open(&named_temp_file).unwrap();
        file.set_modified(SystemTime::now())?;
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

impl From<MutTestReport> for String {
    fn from(val: MutTestReport) -> Self {
        serde_json::to_string(&val.0.borrow().test_result).unwrap_or_default()
    }
}

#[cfg(feature = "ruby")]
pub fn ruby_init(ruby: &magnus::Ruby) -> Result<(), magnus::Error> {
    let status = ruby.define_class("Status", ruby.class_object())?;
    status.define_singleton_method("new", magnus::function!(Status::new, 1))?;
    status.define_method("to_s", magnus::method!(Status::to_string, 0))?;
    let test_report = ruby.define_class("TestReport", ruby.class_object())?;
    test_report.define_singleton_method("new", magnus::function!(MutTestReport::new, 2))?;
    test_report.define_method("to_s", magnus::method!(MutTestReport::to_string, 0))?;
    test_report.define_method("publish", magnus::method!(MutTestReport::publish, 0))?;
    test_report.define_method("add_test", magnus::method!(MutTestReport::add_test, 11))?;
    Ok(())
}
