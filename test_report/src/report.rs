use std::{cell::RefCell, collections::HashMap, env, fs, path::Path, time::SystemTime};

use api::{client::ApiClient, message};
use bundle::BundleMetaDebugProps;
use bundle::Test;
use chrono::prelude::*;
use codeowners::CodeOwners;
use context::repo::{BundleRepo, RepoUrlParts};
#[cfg(feature = "ruby")]
use magnus::{value::ReprValue, Module, Object};
use prost_wkt_types::Timestamp;
use proto::test_context::test_run::{
    CodeOwner, TestCaseRun, TestCaseRunStatus, TestResult, UploaderMetadata,
};
use third_party::sentry;
use tracing_subscriber::{filter::FilterFn, prelude::*};
use trunk_analytics_cli::{context::gather_initial_test_context, upload_command::run_upload};
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Debug, Clone, PartialEq)]
pub struct TestReport {
    test_result: TestResult,
    command: String,
    started_at: SystemTime,
    quarantined_tests: Option<HashMap<String, Test>>,
    codeowners: Option<CodeOwners>,
    variant: Option<String>,
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
            UNSPECIFIED => Status::Unspecified,
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
    pub fn new(origin: String, command: String, variant: Option<String>) -> Self {
        let started_at = SystemTime::now();
        let mut test_result = TestResult::default();
        test_result.uploader_metadata = Some(UploaderMetadata {
            origin: origin.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            upload_time: None,
            variant: variant.clone().unwrap_or_default(),
        });
        let codeowners = BundleRepo::new(None, None, None, None, None, None, false)
            .ok()
            .map(|repo| repo.repo_root)
            .as_ref()
            .map(Path::new::<String>)
            .and_then(|repo_root| CodeOwners::find_file(repo_root, &None::<&Path>));
        Self(RefCell::new(TestReport {
            test_result,
            command,
            started_at,
            quarantined_tests: None,
            codeowners,
            variant: variant.clone(),
        }))
    }

    fn serialize_test_result(&self) -> Vec<u8> {
        prost::Message::encode_to_vec(&self.0.borrow().test_result)
    }

    fn setup_logger(&self, org_url_slug: String) {
        let sentry_layer = sentry_tracing::layer().event_mapper(move |event, context| {
            // trunk-ignore(clippy/match_ref_pats)
            match event.metadata().level() {
                &tracing::Level::ERROR => {
                    let mut event = sentry_tracing::event_from_event(event, context);
                    event
                        .tags
                        .insert(String::from("org_url_slug"), org_url_slug.clone());
                    sentry_tracing::EventMapping::Event(event)
                }
                &tracing::Level::WARN => sentry_tracing::EventMapping::Breadcrumb(
                    sentry_tracing::breadcrumb_from_event(event, context),
                ),
                &tracing::Level::INFO => sentry_tracing::EventMapping::Breadcrumb(
                    sentry_tracing::breadcrumb_from_event(event, context),
                ),
                &tracing::Level::DEBUG => sentry_tracing::EventMapping::Breadcrumb(
                    sentry_tracing::breadcrumb_from_event(event, context),
                ),
                _ => sentry_tracing::EventMapping::Ignore,
            }
        });
        let console_layer = tracing_subscriber::fmt::Layer::new()
            .without_time()
            .with_target(false)
            .with_level(false)
            .with_filter(FilterFn::new(|metadata| {
                !metadata
                    .fields()
                    .iter()
                    .any(|field| field.name() == "hidden_in_console")
            }));

        let trunk_log_level = match env::var("TRUNK_LOG_LEVEL").unwrap_or_default().as_str() {
            "debug" => tracing::level_filters::LevelFilter::DEBUG,
            "info" => tracing::level_filters::LevelFilter::INFO,
            "warn" => tracing::level_filters::LevelFilter::WARN,
            "error" => tracing::level_filters::LevelFilter::ERROR,
            _ => tracing::level_filters::LevelFilter::INFO,
        };
        tracing_subscriber::registry()
            .with(console_layer)
            .with(sentry_layer)
            .with(trunk_log_level)
            .init();
    }

    pub fn is_quarantined(
        &self,
        id: Option<String>,
        name: Option<String>,
        parent_name: Option<String>,
        classname: Option<String>,
        file: Option<String>,
    ) -> bool {
        let token = env::var("TRUNK_API_TOKEN").unwrap_or_default();
        let org_url_slug = env::var("TRUNK_ORG_URL_SLUG").unwrap_or_default();
        if token.is_empty() {
            tracing::warn!("Not checking quarantine status because TRUNK_API_TOKEN is empty");
            return false;
        }
        if org_url_slug.is_empty() {
            tracing::warn!("Not checking quarantine status because TRUNK_ORG_URL_SLUG is empty");
            return false;
        }
        let api_client = ApiClient::new(token, org_url_slug.clone(), None);
        let bundle_repo = BundleRepo::new(None, None, None, None, None, None, false);
        match (api_client, bundle_repo) {
            (Ok(api_client), Ok(bundle_repo)) => {
                let test_identifier = Test::new(
                    id.clone(),
                    name.unwrap_or_default(),
                    parent_name.unwrap_or_default(),
                    classname,
                    file,
                    org_url_slug.clone(),
                    &bundle_repo.repo,
                    None,
                );
                self.populate_quarantined_tests(&api_client, &bundle_repo.repo, org_url_slug);
                if let Some(quarantined_tests) = self.0.borrow().quarantined_tests.as_ref() {
                    return quarantined_tests.get(&test_identifier.id).is_some();
                }
                false
            }
            _ => {
                tracing::warn!("Unable to fetch quarantined tests");
                false
            }
        }
    }

    fn populate_quarantined_tests(
        &self,
        api_client: &ApiClient,
        repo: &RepoUrlParts,
        org_url_slug: String,
    ) {
        if self.0.borrow().quarantined_tests.as_ref().is_some() {
            // already fetched
            return;
        }
        let mut quarantined_tests = HashMap::new();
        let mut request = message::ListQuarantinedTestsRequest {
            org_url_slug: org_url_slug.clone(),
            page_query: message::PageQuery {
                page_size: 100,
                page_token: String::new(),
            },
            repo: repo.clone(),
        };
        loop {
            let response = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(api_client.list_quarantined_tests(&request));
            match response {
                Ok(response) => {
                    for test in response.quarantined_tests.iter() {
                        let test = Test::new(
                            Some(test.test_case_id.clone()),
                            test.name.clone(),
                            test.parent.clone().unwrap_or_default(),
                            test.classname.clone(),
                            test.file.clone(),
                            org_url_slug.clone(),
                            repo,
                            None,
                        );

                        quarantined_tests.insert(test.id.clone(), test);
                    }
                    if response.page.next_page_token.is_empty() {
                        break;
                    }
                    request.page_query.page_token = response.page.next_page_token;
                }
                Err(err) => {
                    tracing::warn!("Unable to fetch quarantined tests");
                    tracing::error!(
                        hidden_in_console = true,
                        "Error fetching quarantined tests: {:?}",
                        err
                    );
                    break;
                }
            }
        }
        self.0.borrow_mut().quarantined_tests = Some(quarantined_tests);
    }

    // sends out to the trunk api
    pub fn publish(&self) -> bool {
        let release_name = format!("rspec-flaky-tests@{}", env!("CARGO_PKG_VERSION"));
        let org_url_slug = env::var("TRUNK_ORG_URL_SLUG").unwrap_or_default();
        let guard = sentry::init(release_name.into(), None);
        self.setup_logger(org_url_slug.clone());
        let named_temp_file = match tempfile::Builder::new().suffix(".bin").tempfile() {
            Ok(tempfile) => tempfile,
            Err(e) => {
                tracing::error!("Error creating temp file: {:?}", e);
                return false;
            }
        };
        let desired_path = named_temp_file.path().to_path_buf();
        let resolved_path = match self.save(desired_path.clone()) {
            Ok(path) => path,
            Err(e) => {
                tracing::error!("Error saving test results: {:?}", e);
                return false;
            }
        };
        let resolved_path_str = resolved_path.as_path().to_str().unwrap_or_default();
        let token = env::var("TRUNK_API_TOKEN").unwrap_or_default();
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
            None,
            false,
            false,
        );
        let debug_props = BundleMetaDebugProps {
            command_line: self.0.borrow().command.clone(),
        };
        let test_run_result = trunk_analytics_cli::test_command::TestRunResult {
            command: self.0.borrow().command.clone(),
            exec_start: Some(self.0.borrow().started_at),
            exit_code: 0,
        };
        let result = match gather_initial_test_context(upload_args.clone(), debug_props) {
            Ok(pre_test_context) => {
                match tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(run_upload(
                        upload_args,
                        Some(pre_test_context),
                        Some(test_run_result),
                        None,
                    )) {
                    Ok(upload_result) => {
                        if let Some(upload_bundle_error) =
                            upload_result.error_report.map(|e| e.error)
                        {
                            tracing::error!(
                                hidden_in_console = true,
                                "Error uploading: {:?}",
                                upload_bundle_error
                            );
                            false
                        } else {
                            true
                        }
                    }
                    Err(e) => {
                        tracing::error!(hidden_in_console = true, "Error uploading: {:?}", e);
                        false
                    }
                }
            }
            Err(e) => {
                tracing::error!(
                    hidden_in_console = true,
                    "Error gathering initial test context: {:?}",
                    e
                );
                false
            }
        };
        guard.flush(None);
        result
    }

    // saves to local fs and returns the path
    fn save(&self, path_buf: std::path::PathBuf) -> Result<std::path::PathBuf, std::io::Error> {
        // create parent directory if it doesn't exist
        if let Some(parent) = path_buf.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        let buf = self.serialize_test_result();
        fs::write(&path_buf, buf)?;
        // file modification uses filetime which is less precise than systemTime
        // we need to update it to the current time to avoid race conditions later down the line
        // when the start time ends up being after the file modification time
        let file = fs::File::open(&path_buf).unwrap();
        file.set_modified(SystemTime::now())?;
        Ok(path_buf)
    }

    // saves to local fs and returns the path
    pub fn try_save(&self, path: String) -> bool {
        let desired_path = std::path::PathBuf::from(path).join("trunk_output.bin");
        match self.save(desired_path) {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!("Error saving test results: {:?}", e);
                false
            }
        }
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
        is_quarantined: bool,
    ) {
        let mut test = TestCaseRun::default();
        if let Some(id) = id {
            test.id = id;
        }
        test.name = name;
        test.classname = classname;
        // trunk-ignore(clippy/assigning_clones)
        test.file = file.clone();
        if !test.file.is_empty() {
            let codeowners: Option<Vec<String>> = self
                .0
                .borrow_mut()
                .codeowners
                .as_ref()
                .map(|co| codeowners::flatten_code_owners(co, &file));
            if let Some(codeowners) = codeowners {
                test.codeowners = codeowners
                    .iter()
                    .map(|name| CodeOwner { name: name.clone() })
                    .collect();
            }
        }
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
        test.is_quarantined = is_quarantined;
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
    status.define_singleton_method("new", magnus::function!(Status::new, 2))?;
    status.define_method("to_s", magnus::method!(Status::to_string, 0))?;
    let test_report = ruby.define_class("TestReport", ruby.class_object())?;
    test_report.define_singleton_method("new", magnus::function!(MutTestReport::new, 2))?;
    test_report.define_method("to_s", magnus::method!(MutTestReport::to_string, 0))?;
    test_report.define_method("publish", magnus::method!(MutTestReport::publish, 0))?;
    test_report.define_method("add_test", magnus::method!(MutTestReport::add_test, 12))?;
    test_report.define_method("try_save", magnus::method!(MutTestReport::try_save, 1))?;
    test_report.define_method(
        "is_quarantined",
        magnus::method!(MutTestReport::is_quarantined, 5),
    )?;
    Ok(())
}
