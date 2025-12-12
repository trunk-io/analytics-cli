use std::{
    fmt::{Display, Formatter, Result},
    io::BufRead,
    mem,
};

use codeowners::CodeOwners;
#[cfg(feature = "bindings")]
use prost::Message;
use prost_wkt_types::Timestamp;
use proto::test_context::test_run::{CodeOwner, TestCaseRun, TestCaseRunStatus};
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum};
use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestRerun, TestSuite};
use quick_xml::{
    Reader,
    events::{BytesStart, BytesText, Event},
};
use thiserror::Error;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use super::date_parser::JunitDateParser;
#[cfg(feature = "bindings")]
use crate::junit::bindings::BindingsReport;
use crate::{meta::id::gen_info_id, repo::RepoUrlParts};

const TAG_REPORT: &[u8] = b"testsuites";
const TAG_TEST_SUITE: &[u8] = b"testsuite";
const TAG_TEST_CASE: &[u8] = b"testcase";
const TAG_TEST_CASE_STATUS_FAILURE: &[u8] = b"failure";
const TAG_TEST_CASE_STATUS_ERROR: &[u8] = b"error";
const TAG_TEST_CASE_STATUS_SKIPPED: &[u8] = b"skipped";
const TAG_TEST_RERUN_FAILURE: &[u8] = b"rerunFailure";
const TAG_TEST_RERUN_ERROR: &[u8] = b"rerunError";
const TAG_TEST_RERUN_FLAKY_FAILURE: &[u8] = b"flakyFailure";
const TAG_TEST_RERUN_FLAKY_ERROR: &[u8] = b"flakyError";
const TAG_TEST_RERUN_STACK_TRACE: &[u8] = b"stackTrace";
const TAG_SYSTEM_OUT: &[u8] = b"system-out";
const TAG_SYSTEM_ERR: &[u8] = b"system-err";

pub mod extra_attrs {
    pub const FILE: &str = "file";
    pub const FILEPATH: &str = "filepath";
    pub const LINE: &str = "line";
    pub const ID: &str = "id";
    pub const ATTEMPT_NUMBER: &str = "attempt_number";
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JunitParseIssue {
    SubOptimal(JunitParseIssueSubOptimal),
    Invalid(JunitParseIssueInvalid),
}

impl Display for JunitParseIssue {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            JunitParseIssue::SubOptimal(so) => write!(f, "{}", so),
            JunitParseIssue::Invalid(i) => write!(f, "{}", i),
        }
    }
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass_enum, pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum JunitParseIssueLevel {
    Valid = 0,
    SubOptimal = 1,
    Invalid = 2,
}

impl Default for JunitParseIssueLevel {
    fn default() -> Self {
        Self::Valid
    }
}

impl From<&JunitParseIssue> for JunitParseIssueLevel {
    fn from(value: &JunitParseIssue) -> Self {
        match value {
            JunitParseIssue::SubOptimal(..) => JunitParseIssueLevel::SubOptimal,
            JunitParseIssue::Invalid(..) => JunitParseIssueLevel::Invalid,
        }
    }
}

#[derive(Error, Debug, Copy, Clone, PartialEq, Eq)]
pub enum JunitParseIssueSubOptimal {
    #[error("no reports found")]
    ReportNotFound,
}

#[derive(Error, Debug, Copy, Clone, PartialEq, Eq)]
pub enum JunitParseIssueInvalid {
    #[error("multiple reports found")]
    ReportMultipleFound,
    #[error("report end tag found without start tag")]
    ReportStartTagNotFound,
    #[error("test suite end tag found without start tag")]
    TestSuiteStartTagNotFound,
    #[error("could not parse test case name")]
    TestCaseName,
    #[error("test case found without a test suite found")]
    TestCaseTestSuiteNotFound,
    #[error("test case end tag found without start tag")]
    TestCaseStartTagNotFound,
    #[error("test case status found without a test case found")]
    TestCaseStatusTestCaseNotFound,
    #[error("test rerun found without a test case found")]
    TestRerunStartTagNotFound,
    #[error("test rerun end tag found without start tag")]
    TestRerunTestCaseNotFound,
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JunitParseFlatIssue {
    pub level: JunitParseIssueLevel,
    pub error_message: String,
}

#[derive(Debug, Clone)]
enum Text {
    SystemOut(Option<String>),
    SystemErr(Option<String>),
    StackTrace(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CurrentReportState {
    Default,
    DefaultWithTestSuites,
    Opened,
}

#[derive(Debug, Clone)]
pub struct JunitParser {
    date_parser: JunitDateParser,
    issues: Vec<JunitParseIssue>,
    reports: Vec<Report>,
    current_report: Report,
    current_report_state: CurrentReportState,
    current_test_suite: Option<TestSuite>,
    current_test_suite_depth: usize,
    current_test_case: Option<TestCase>,
    current_test_rerun: Option<TestRerun>,
    current_text: Option<Text>,
}

impl Default for JunitParser {
    fn default() -> Self {
        Self::new()
    }
}

impl JunitParser {
    pub fn new() -> Self {
        Self {
            date_parser: Default::default(),
            issues: Default::default(),
            reports: Default::default(),
            current_report: Report::new(""),
            current_report_state: CurrentReportState::Default,
            current_test_suite: Default::default(),
            current_test_suite_depth: Default::default(),
            current_test_case: Default::default(),
            current_test_rerun: Default::default(),
            current_text: Default::default(),
        }
    }

    pub fn issues(&self) -> &Vec<JunitParseIssue> {
        &self.issues
    }

    pub fn issues_flat(&self) -> Vec<JunitParseFlatIssue> {
        return self
            .issues()
            .iter()
            .map(|i| JunitParseFlatIssue {
                level: JunitParseIssueLevel::from(i),
                error_message: i.to_string(),
            })
            .collect();
    }

    pub fn reports(&self) -> &Vec<Report> {
        &self.reports
    }

    pub fn into_reports(self) -> Vec<Report> {
        self.reports
    }

    pub fn into_test_case_runs<T: AsRef<str>>(
        self,
        codeowners: Option<&CodeOwners>,
        org_slug: &T,
        repo: &RepoUrlParts,
        quarantined_test_ids: &[String],
        variant: &str,
    ) -> Vec<TestCaseRun> {
        let mut test_case_runs = Vec::new();
        for report in self.reports {
            for test_suite in report.test_suites {
                for test_case in test_suite.test_cases {
                    let mut test_case_run = TestCaseRun {
                        name: test_case.name.into(),
                        parent_name: test_suite.name.to_string(),
                        ..Default::default()
                    };
                    test_case_run.classname = test_case
                        .classname
                        .clone()
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    if let Some(test_case_timestamp) = test_case.timestamp {
                        test_case_run.started_at = Some(Timestamp {
                            seconds: test_case_timestamp.timestamp(),
                            nanos: test_case_timestamp.timestamp_subsec_nanos() as i32,
                        });
                    } else if let Some(test_suite_timestamp) = test_suite.timestamp {
                        test_case_run.started_at = Some(Timestamp {
                            seconds: test_suite_timestamp.timestamp(),
                            nanos: test_suite_timestamp.timestamp_subsec_nanos() as i32,
                        });
                    }
                    if let Some(test_case_time) = test_case.time {
                        if test_case_run.started_at.is_some() {
                            // If we have started_at, calculate finished_at from it
                            test_case_run.finished_at =
                                test_case_run.started_at.clone().map(|mut v| {
                                    v.seconds += test_case_time.as_secs() as i64;
                                    v.nanos += test_case_time.subsec_nanos() as i32;
                                    v
                                });
                        } else {
                            // If we have time but no started_at, use epoch + time as finished_at and epoch as started_at
                            // This preserves the time duration while using a consistent reference point
                            test_case_run.started_at = Some(Timestamp {
                                seconds: 0,
                                nanos: 0,
                            });
                            test_case_run.finished_at = Some(Timestamp {
                                seconds: test_case_time.as_secs() as i64,
                                nanos: test_case_time.subsec_nanos() as i32,
                            });
                        }
                    } else if let Some(test_suite_time) = test_suite.time {
                        if test_case_run.started_at.is_some() {
                            // If we have started_at, calculate finished_at from it
                            test_case_run.finished_at =
                                test_case_run.started_at.clone().map(|mut v| {
                                    v.seconds += test_suite_time.as_secs() as i64;
                                    v.nanos += test_suite_time.subsec_nanos() as i32;
                                    v
                                });
                        } else {
                            // If we have time but no started_at, use epoch + time as finished_at and epoch as started_at
                            // This preserves the time duration while using a consistent reference point
                            test_case_run.started_at = Some(Timestamp {
                                seconds: 0,
                                nanos: 0,
                            });
                            test_case_run.finished_at = Some(Timestamp {
                                seconds: test_suite_time.as_secs() as i64,
                                nanos: test_suite_time.subsec_nanos() as i32,
                            });
                        }
                    }
                    test_case_run.status = match test_case.status {
                        TestCaseStatus::Success { .. } => TestCaseRunStatus::Success.into(),
                        TestCaseStatus::Skipped {
                            message,
                            description,
                            ..
                        } => {
                            if let Some(description) = description {
                                test_case_run.status_output_message = description.to_string();
                            } else if let Some(message) = message {
                                test_case_run.status_output_message = message.to_string();
                            }
                            TestCaseRunStatus::Skipped.into()
                        }
                        TestCaseStatus::NonSuccess {
                            message,
                            description,
                            ..
                        } => {
                            if let Some(description) = description {
                                test_case_run.status_output_message = description.to_string();
                            } else if let Some(message) = message {
                                test_case_run.status_output_message = message.to_string();
                            }
                            TestCaseRunStatus::Failure.into()
                        }
                    };
                    let file = test_case
                        .extra
                        .get(extra_attrs::FILE)
                        .or_else(|| test_case.extra.get(extra_attrs::FILEPATH))
                        .or_else(|| test_suite.extra.get(extra_attrs::FILE))
                        .or_else(|| test_suite.extra.get(extra_attrs::FILEPATH))
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    if !file.is_empty() && codeowners.is_some() {
                        let codeowners: Option<Vec<String>> = codeowners
                            .as_ref()
                            .map(|co| codeowners::flatten_code_owners(co, &file));
                        if let Some(codeowners) = codeowners {
                            test_case_run.codeowners = codeowners
                                .iter()
                                .map(|name| CodeOwner { name: name.clone() })
                                .collect();
                        }
                    }

                    let existing_id = test_case.extra.get(extra_attrs::ID).map(|v| v.as_str());

                    // trunk-ignore(clippy/unnecessary_unwrap)
                    let test_case_id = if existing_id.is_some() && variant.is_empty() {
                        existing_id.unwrap().to_string()
                    } else {
                        gen_info_id(
                            org_slug.as_ref(),
                            repo.repo_full_name().as_str(),
                            Some(file.as_str()),
                            test_case.classname.map(|v| v.to_string()).as_deref(),
                            Some(test_case_run.parent_name.as_str()),
                            Some(test_case_run.name.as_str()),
                            existing_id,
                            variant,
                        )
                    };
                    test_case_run.is_quarantined = quarantined_test_ids.contains(&test_case_id);
                    test_case_run.id = test_case_id;

                    test_case_run.file = file;
                    test_case_run.line = test_case
                        .extra
                        .get(extra_attrs::LINE)
                        .map(|v| v.to_string())
                        .and_then(|v| v.parse::<i32>().ok())
                        .unwrap_or_default();
                    test_case_run.attempt_number = test_case
                        .extra
                        .get(extra_attrs::ATTEMPT_NUMBER)
                        .map(|v| v.to_string())
                        .and_then(|v| v.parse::<i32>().ok())
                        .unwrap_or_default();

                    test_case_runs.push(test_case_run);
                }
            }
        }
        test_case_runs
    }

    pub fn parse<R: BufRead>(&mut self, xml: R) -> anyhow::Result<()> {
        let mut reader = Reader::from_reader(xml);
        reader.config_mut().trim_text(true);

        let mut buf = Vec::new();
        loop {
            if self
                .match_event(reader.read_event_into(&mut buf)?)
                .is_none()
            {
                break;
            }
            buf.clear();
        }

        match self.reports.len() {
            0 => self.issues.push(JunitParseIssue::SubOptimal(
                JunitParseIssueSubOptimal::ReportNotFound,
            )),
            1 => {
                // There should only be 1 report per JUnit.xml file
            }
            _ => self.issues.push(JunitParseIssue::Invalid(
                JunitParseIssueInvalid::ReportMultipleFound,
            )),
        };

        Ok(())
    }

    fn match_event(&mut self, event: Event) -> Option<()> {
        match event {
            Event::Eof => {
                self.close_default_report();
                return None;
            }
            Event::Start(e) => match e.name().as_ref() {
                TAG_REPORT => self.open_report(&e),
                TAG_TEST_SUITE => self.open_test_suite(&e),
                TAG_TEST_CASE => self.open_test_case(&e),
                TAG_TEST_CASE_STATUS_FAILURE
                | TAG_TEST_CASE_STATUS_ERROR
                | TAG_TEST_CASE_STATUS_SKIPPED => self.set_test_case_status(&e),
                TAG_TEST_RERUN_FAILURE
                | TAG_TEST_RERUN_ERROR
                | TAG_TEST_RERUN_FLAKY_FAILURE
                | TAG_TEST_RERUN_FLAKY_ERROR => {
                    self.open_test_rerun(&e);
                }
                TAG_TEST_RERUN_STACK_TRACE => self.open_text(Text::StackTrace(None)),
                TAG_SYSTEM_OUT => self.open_text(Text::SystemOut(None)),
                TAG_SYSTEM_ERR => self.open_text(Text::SystemErr(None)),
                _ => (),
            },
            Event::End(e) => match e.name().as_ref() {
                TAG_REPORT => self.close_report(),
                TAG_TEST_SUITE => self.close_test_suite(),
                TAG_TEST_CASE => self.close_test_case(),
                TAG_TEST_CASE_STATUS_FAILURE
                | TAG_TEST_CASE_STATUS_ERROR
                | TAG_TEST_CASE_STATUS_SKIPPED => {
                    // There's only 1 status per test case, so there's nothing to close
                }
                TAG_TEST_RERUN_FAILURE
                | TAG_TEST_RERUN_ERROR
                | TAG_TEST_RERUN_FLAKY_FAILURE
                | TAG_TEST_RERUN_FLAKY_ERROR => {
                    self.close_test_rerun();
                }
                TAG_TEST_RERUN_STACK_TRACE | TAG_SYSTEM_OUT | TAG_SYSTEM_ERR => {
                    self.close_system_text()
                }
                _ => (),
            },
            Event::Empty(e) => match e.name().as_ref() {
                TAG_REPORT => {
                    self.open_report(&e);
                    self.close_report();
                }
                TAG_TEST_SUITE => {
                    self.open_test_suite(&e);
                    self.close_test_suite();
                }
                TAG_TEST_CASE => {
                    self.open_test_case(&e);
                    self.close_test_case();
                }
                TAG_TEST_CASE_STATUS_FAILURE
                | TAG_TEST_CASE_STATUS_ERROR
                | TAG_TEST_CASE_STATUS_SKIPPED => {
                    self.set_test_case_status(&e);
                }
                TAG_TEST_RERUN_FAILURE
                | TAG_TEST_RERUN_ERROR
                | TAG_TEST_RERUN_FLAKY_FAILURE
                | TAG_TEST_RERUN_FLAKY_ERROR => {
                    self.open_test_rerun(&e);
                    self.close_test_rerun();
                }
                _ => (),
            },
            Event::CData(e) => {
                if let Ok(e) = e.minimal_escape() {
                    self.match_text(&e);
                }
            }
            Event::Text(e) => {
                self.match_text(&e);
            }
            _ => (),
        };
        Some(())
    }

    fn match_text(&mut self, e: &BytesText) {
        if self.current_text.is_some() {
            self.set_text_value(e);
        } else if self.current_test_rerun.is_some() {
            self.set_test_rerun_description(e);
        } else {
            self.set_test_case_status_description(e);
        }
    }

    fn take_report(&mut self) -> Report {
        let mut default_report = Report::new("");
        mem::swap(&mut self.current_report, &mut default_report);
        self.current_report_state = CurrentReportState::Default;
        default_report
    }

    fn set_report(&mut self, report: Report) {
        self.close_default_report();
        self.current_report_state = CurrentReportState::Opened;
        self.current_report = report;
    }

    fn open_report(&mut self, e: &BytesStart) {
        let report_name = parse_attr::name(e).unwrap_or_default();
        let mut report = Report::new(report_name);

        if let Some(timestamp) = parse_attr::timestamp(e, &mut self.date_parser) {
            report.set_timestamp(timestamp);
        }

        if let Some(time) = parse_attr::time(e) {
            report.set_time(time);
        }

        self.set_report(report);
    }

    fn close_report(&mut self) {
        if self.current_report_state != CurrentReportState::Opened {
            self.issues.push(JunitParseIssue::Invalid(
                JunitParseIssueInvalid::ReportStartTagNotFound,
            ));
            return;
        }

        let report = self.take_report();
        self.reports.push(report);
    }

    fn close_default_report(&mut self) {
        if self.current_report_state != CurrentReportState::DefaultWithTestSuites {
            return;
        }

        let report = self.take_report();
        self.reports.push(report);
    }

    fn open_test_suite(&mut self, e: &BytesStart) {
        self.current_test_suite_depth += 1;
        if self.current_test_suite_depth > 1 {
            return; // Ignore all but outermost test suite in set of nested test suites
        }

        let test_suite_name = parse_attr::name(e).unwrap_or_default();
        let mut test_suite = TestSuite::new(test_suite_name);

        if let Some(timestamp) = parse_attr::timestamp(e, &mut self.date_parser) {
            test_suite.set_timestamp(timestamp);
        }

        if let Some(time) = parse_attr::time(e) {
            test_suite.set_time(time);
        }

        if let Some(file) = parse_attr::file(e) {
            test_suite
                .extra
                .insert(extra_attrs::FILE.into(), file.into());
        }

        if let Some(filepath) = parse_attr::filepath(e) {
            test_suite
                .extra
                .insert(extra_attrs::FILEPATH.into(), filepath.into());
        }

        if let Some(id) = parse_attr::id(e) {
            test_suite.extra.insert(extra_attrs::ID.into(), id.into());
        }

        if let Some(line) = parse_attr::line(e) {
            test_suite
                .extra
                .insert(extra_attrs::LINE.into(), line.to_string().into());
        }

        self.current_test_suite = Some(test_suite);
    }

    fn close_test_suite(&mut self) {
        self.current_test_suite_depth -= 1;
        if self.current_test_suite_depth > 0 {
            return; // Ignore all but outermost test suite in set of nested test suites
        }

        if let Some(test_suite) = self.current_test_suite.take() {
            if self.current_report_state == CurrentReportState::Default {
                self.current_report_state = CurrentReportState::DefaultWithTestSuites
            }
            self.current_report.add_test_suite(test_suite);
        } else {
            self.issues.push(JunitParseIssue::Invalid(
                JunitParseIssueInvalid::TestSuiteStartTagNotFound,
            ));
        }
    }

    fn open_test_case(&mut self, e: &BytesStart) {
        let test_case_name = parse_attr::name(e).unwrap_or_default();
        if test_case_name.is_empty() {
            self.issues.push(JunitParseIssue::Invalid(
                JunitParseIssueInvalid::TestCaseName,
            ));
        };
        let mut test_case = TestCase::new(test_case_name, TestCaseStatus::success());

        if let Some(timestamp) = parse_attr::timestamp(e, &mut self.date_parser) {
            test_case.set_timestamp(timestamp);
        }

        if let Some(time) = parse_attr::time(e) {
            test_case.set_time(time);
        }

        if let Some(assertions) = parse_attr::assertions(e) {
            test_case.set_assertions(assertions);
        }

        if let Some(classname) = parse_attr::classname(e) {
            test_case.set_classname(classname);
        }

        if let Some(file) = parse_attr::file(e) {
            test_case
                .extra
                .insert(extra_attrs::FILE.into(), file.into());
        }

        if let Some(filepath) = parse_attr::filepath(e) {
            test_case
                .extra
                .insert(extra_attrs::FILEPATH.into(), filepath.into());
        }

        if let Some(id) = parse_attr::id(e) {
            test_case.extra.insert(extra_attrs::ID.into(), id.into());
        }

        if let Some(line) = parse_attr::line(e) {
            test_case
                .extra
                .insert(extra_attrs::LINE.into(), line.to_string().into());
        }

        self.current_test_case = Some(test_case);
    }

    fn close_test_case(&mut self) {
        if let Some(test_suite) = self.current_test_suite.as_mut() {
            if let Some(test_case) = self.current_test_case.take() {
                test_suite.add_test_case(test_case);
            } else {
                self.issues.push(JunitParseIssue::Invalid(
                    JunitParseIssueInvalid::TestCaseStartTagNotFound,
                ));
            }
        } else {
            self.issues.push(JunitParseIssue::Invalid(
                JunitParseIssueInvalid::TestCaseTestSuiteNotFound,
            ));
        }
    }

    fn set_test_case_status(&mut self, e: &BytesStart) {
        if let Some(test_case) = self.current_test_case.as_mut() {
            if !matches!(test_case.status, TestCaseStatus::Success { .. }) {
                return; // Only set status once
            }
            let tag = e.name();
            let mut test_case_status = if tag.as_ref() == TAG_TEST_CASE_STATUS_SKIPPED {
                TestCaseStatus::skipped()
            } else {
                let non_success_kind = if tag.as_ref() == TAG_TEST_CASE_STATUS_FAILURE {
                    NonSuccessKind::Failure
                } else {
                    NonSuccessKind::Error
                };
                TestCaseStatus::non_success(non_success_kind)
            };

            if let Some(message) = parse_attr::message(e) {
                test_case_status.set_message(message);
            }

            if let Some(r#type) = parse_attr::r#type(e) {
                test_case_status.set_type(r#type);
            }

            test_case.status = test_case_status;
        } else {
            self.issues.push(JunitParseIssue::Invalid(
                JunitParseIssueInvalid::TestCaseStatusTestCaseNotFound,
            ));
        }
    }

    fn set_test_case_status_description(&mut self, e: &BytesText) {
        if let (Some(test_case), Some(description)) =
            (&mut self.current_test_case, unescape_and_truncate::text(e))
        {
            test_case.status.set_description(description);
        }
    }

    fn open_test_rerun(&mut self, e: &BytesStart) {
        let mut test_rerun = match e.name().as_ref() {
            TAG_TEST_RERUN_FAILURE => TestRerun::new(NonSuccessKind::Failure),
            TAG_TEST_RERUN_ERROR => TestRerun::new(NonSuccessKind::Error),
            TAG_TEST_RERUN_FLAKY_FAILURE => TestRerun::new(NonSuccessKind::Failure),
            TAG_TEST_RERUN_FLAKY_ERROR => TestRerun::new(NonSuccessKind::Error),
            _ => return,
        };

        if let Some(timestamp) = parse_attr::timestamp(e, &mut self.date_parser) {
            test_rerun.set_timestamp(timestamp);
        }

        if let Some(time) = parse_attr::time(e) {
            test_rerun.set_time(time);
        }

        if let Some(message) = parse_attr::message(e) {
            test_rerun.set_message(message);
        }

        if let Some(r#type) = parse_attr::r#type(e) {
            test_rerun.set_type(r#type);
        }

        self.current_test_rerun = Some(test_rerun);
    }

    fn set_test_rerun_description(&mut self, e: &BytesText) {
        if let (Some(test_rerun), Some(description)) =
            (&mut self.current_test_rerun, unescape_and_truncate::text(e))
        {
            test_rerun.set_description(description);
        }
    }

    fn close_test_rerun(&mut self) {
        if let Some(test_case) = self.current_test_case.as_mut() {
            if let Some(test_rerun) = self.current_test_rerun.take() {
                test_case.status.add_rerun(test_rerun);
            } else {
                self.issues.push(JunitParseIssue::Invalid(
                    JunitParseIssueInvalid::TestRerunStartTagNotFound,
                ));
            }
        } else {
            self.issues.push(JunitParseIssue::Invalid(
                JunitParseIssueInvalid::TestRerunTestCaseNotFound,
            ));
        }
    }

    fn open_text(&mut self, text: Text) {
        self.current_text = Some(text);
    }

    fn set_text_value(&mut self, e: &BytesText) {
        if let (Some(text), Some(value)) = (&mut self.current_text, unescape_and_truncate::text(e))
        {
            let inner_value = match text {
                Text::SystemOut(v) => v,
                Text::SystemErr(v) => v,
                Text::StackTrace(v) => v,
            };
            *inner_value = Some(String::from(value));
        }
    }

    fn close_system_text(&mut self) {
        if let Some(test_rerun) = self.current_test_rerun.as_mut() {
            match self.current_text.take() {
                Some(Text::StackTrace(Some(s))) => {
                    test_rerun.set_stack_trace(s);
                }
                Some(Text::SystemOut(Some(s))) => {
                    test_rerun.set_system_out(s);
                }
                Some(Text::SystemErr(Some(s))) => {
                    test_rerun.set_system_err(s);
                }
                _ => (),
            };
        } else if let Some(test_case) = self.current_test_case.as_mut() {
            match self.current_text.take() {
                Some(Text::SystemOut(Some(s))) => {
                    test_case.set_system_out(s);
                }
                Some(Text::SystemErr(Some(s))) => {
                    test_case.set_system_err(s);
                }
                _ => (),
            };
        } else if let Some(test_suite) = self.current_test_suite.as_mut() {
            match self.current_text.take() {
                Some(Text::SystemOut(Some(s))) => {
                    test_suite.set_system_out(s);
                }
                Some(Text::SystemErr(Some(s))) => {
                    test_suite.set_system_err(s);
                }
                _ => (),
            };
        }
    }
}

mod parse_attr {
    use std::{borrow::Cow, str::FromStr, time::Duration};

    use chrono::{DateTime, FixedOffset};
    use quick_xml::events::BytesStart;

    use super::{extra_attrs, unescape_and_truncate};
    use crate::junit::date_parser::JunitDateParser;

    pub fn name<'a>(e: &'a BytesStart<'a>) -> Option<Cow<'a, str>> {
        parse_string_attr(e, "name")
    }

    fn is_legal_seconds(candidate: &f64) -> bool {
        candidate.is_finite() && (candidate.is_sign_positive() || *candidate == 0.0)
    }

    pub fn timestamp(
        e: &BytesStart,
        date_parser: &mut JunitDateParser,
    ) -> Option<DateTime<FixedOffset>> {
        parse_string_attr(e, "timestamp").and_then(|value| date_parser.parse_date(&value))
    }

    pub fn time(e: &BytesStart) -> Option<Duration> {
        parse_string_attr_into_other_type(e, "time")
            .iter()
            .filter_map(|seconds: &f64| {
                if is_legal_seconds(seconds) {
                    Some(Duration::from_secs_f64(*seconds))
                } else {
                    None
                }
            })
            .next()
    }

    pub fn assertions(e: &BytesStart) -> Option<usize> {
        parse_string_attr_into_other_type(e, "assertions")
    }

    pub fn classname<'a>(e: &'a BytesStart<'a>) -> Option<Cow<'a, str>> {
        parse_string_attr(e, "classname")
    }

    pub fn message<'a>(e: &'a BytesStart<'a>) -> Option<Cow<'a, str>> {
        parse_string_attr(e, "message")
    }

    pub fn r#type<'a>(e: &'a BytesStart<'a>) -> Option<Cow<'a, str>> {
        parse_string_attr(e, "type")
    }

    pub fn file<'a>(e: &'a BytesStart<'a>) -> Option<Cow<'a, str>> {
        parse_string_attr(e, extra_attrs::FILE)
    }

    pub fn filepath<'a>(e: &'a BytesStart<'a>) -> Option<Cow<'a, str>> {
        parse_string_attr(e, extra_attrs::FILEPATH)
    }

    pub fn id<'a>(e: &'a BytesStart<'a>) -> Option<Cow<'a, str>> {
        parse_string_attr(e, extra_attrs::ID)
    }

    pub fn line<'a>(e: &'a BytesStart<'a>) -> Option<usize> {
        parse_string_attr_into_other_type(e, extra_attrs::LINE)
    }

    fn parse_string_attr<'a>(
        e: &'a BytesStart<'a>,
        attr_name: &'static str,
    ) -> Option<Cow<'a, str>> {
        e.try_get_attribute(attr_name)
            .ok()
            .flatten()
            .and_then(|attr| unescape_and_truncate::attr(&attr))
    }

    fn parse_string_attr_into_other_type<'a, T: FromStr>(
        e: &'a BytesStart<'a>,
        attr_name: &'static str,
    ) -> Option<T> {
        parse_string_attr(e, attr_name).and_then(|value| value.parse::<T>().ok())
    }

    #[cfg(test)]
    mod tests {
        use std::{borrow::Cow, time::Duration};

        use quick_xml::{
            events::{BytesStart, attributes::Attribute},
            name::QName,
        };

        use super::{is_legal_seconds, time};

        #[test]
        fn test_is_legal_seconds() {
            assert!(is_legal_seconds(&2.0e64));
            assert!(is_legal_seconds(&2.0));

            assert!(is_legal_seconds(&0.0));
            assert!(is_legal_seconds(&-0.0));

            assert!(!is_legal_seconds(&-2.0));
            assert!(!is_legal_seconds(&-2.0e64));

            assert!(!is_legal_seconds(&f64::MIN));
            assert!(is_legal_seconds(&f64::MIN_POSITIVE));
            assert!(is_legal_seconds(&f64::MAX));

            assert!(!is_legal_seconds(&f64::NAN));

            assert!(!is_legal_seconds(&f64::INFINITY));
            assert!(!is_legal_seconds(&f64::NEG_INFINITY));
        }

        #[test]
        fn test_legal_time() {
            let mut legal_time = BytesStart::new(Cow::from("legal_time"));
            legal_time.push_attribute(Attribute {
                key: QName(b"time"),
                value: Cow::from(b"1.0"),
            });
            assert_eq!(time(&legal_time), Some(Duration::from_secs_f64(1.0)));
        }

        #[test]
        fn test_illegal_time() {
            let mut illegal_time = BytesStart::new(Cow::from("legal_time"));
            illegal_time.push_attribute(Attribute {
                key: QName(b"time"),
                value: Cow::from(b"-1.0"),
            });
            assert_eq!(time(&illegal_time), None);
        }
    }
}

mod unescape_and_truncate {
    use std::borrow::Cow;

    use quick_xml::events::{BytesText, attributes::Attribute};

    use crate::string_safety::safe_truncate_str;

    const MAX_TEXT_FIELD_SIZE: usize = 8_000;

    pub fn attr<'a>(v: &Attribute<'a>) -> Option<Cow<'a, str>> {
        v.unescape_value()
            .ok()
            .map(|b| safe_truncate_cow::<MAX_TEXT_FIELD_SIZE>(b))
    }

    pub fn text<'a>(v: &BytesText<'a>) -> Option<Cow<'a, str>> {
        v.unescape()
            .ok()
            .map(|b| safe_truncate_cow::<MAX_TEXT_FIELD_SIZE>(b))
    }

    fn safe_truncate_cow<const MAX_LEN: usize>(value: Cow<'_, str>) -> Cow<'_, str> {
        match value {
            Cow::Borrowed(b) => Cow::Borrowed(safe_truncate_str::<MAX_LEN>(b)),
            Cow::Owned(b) => Cow::Owned(String::from(safe_truncate_str::<MAX_LEN>(b.as_str()))),
        }
    }
}

#[cfg(feature = "bindings")]
pub fn bin_parse(bin: &[u8]) -> anyhow::Result<Vec<BindingsReport>> {
    if let Ok(test_report) = proto::test_context::test_run::TestReport::decode(bin) {
        Ok(test_report
            .test_results
            .into_iter()
            .map(BindingsReport::from)
            .collect())
    } else {
        let test_result = proto::test_context::test_run::TestResult::decode(bin)?;
        Ok(vec![BindingsReport::from(test_result)])
    }
}

#[cfg(test)]
mod tests {
    use std::io::BufReader;

    use prost_wkt_types::Timestamp;
    use proto::test_context::test_run::TestCaseRunStatus;

    use crate::{
        junit::parser::JunitParser,
        meta::id::{gen_info_id, gen_info_id_base},
        repo::RepoUrlParts,
    };
    #[test]
    fn test_into_test_case_runs() {
        let mut junit_parser = JunitParser::new();
        let file_contents = r#"
        <xml version="1.0" encoding="UTF-8"?>
        <testsuites>
            <testsuite name="testsuite" timestamp="2023-10-01T12:00:00Z" time="0.002">
                <testcase file="test.java" line="5" classname="test" name="test_variant_truncation1" time="0.001">
                    <failure message="Test failed" type="java.lang.AssertionError">
                        <![CDATA[Expected: <true> but was: <false>]]>
                    </failure>
                </testcase>
                <testcase file="test.java" name="test_variant_truncation2" time="0.001">
                    <failure message="Test failed"/>
                </testcase>
            </testsuite>
        </testsuites>
        "#;
        let parsed_results = junit_parser.parse(BufReader::new(file_contents.as_bytes()));
        assert!(parsed_results.is_ok());

        let org_slug = "org-url-slug".to_string();
        let repo = RepoUrlParts {
            host: "repo-host".into(),
            owner: "repo-owner".into(),
            name: "repo-name".into(),
        };

        let test_case_runs = junit_parser.into_test_case_runs(
            None,
            &org_slug,
            &repo,
            &[gen_info_id_base(
                org_slug.as_str(),
                repo.repo_full_name().as_str(),
                Some("test.java"),
                Some("test"),
                Some("testsuite"),
                Some("test_variant_truncation1"),
                None,
                "",
            )],
            "",
        );
        assert_eq!(test_case_runs.len(), 2);
        let test_case_run1 = &test_case_runs[0];
        assert_eq!(test_case_run1.name, "test_variant_truncation1");
        assert_eq!(test_case_run1.parent_name, "testsuite");
        assert_eq!(test_case_run1.classname, "test");
        assert_eq!(test_case_run1.status, TestCaseRunStatus::Failure as i32);
        assert_eq!(
            test_case_run1.status_output_message,
            "Expected: <true> but was: <false>"
        );
        assert_eq!(test_case_run1.file, "test.java");
        assert_eq!(test_case_run1.attempt_number, 0);
        assert!(test_case_run1.is_quarantined);
        assert_eq!(
            test_case_run1.started_at,
            Some(Timestamp {
                seconds: 1696161600,
                nanos: 0
            })
        );
        assert_eq!(
            test_case_run1.finished_at,
            Some(Timestamp {
                seconds: 1696161600,
                nanos: 1000000
            })
        );
        assert_eq!(test_case_run1.line, 5);
        // Verify that the ID field is set correctly (generated from gen_info_id_base)
        assert_eq!(
            test_case_run1.id,
            gen_info_id_base(
                org_slug.as_str(),
                repo.repo_full_name().as_str(),
                Some("test.java"),
                Some("test"),
                Some("testsuite"),
                Some("test_variant_truncation1"),
                None,
                "",
            )
        );

        let test_case_run2 = &test_case_runs[1];
        assert_eq!(test_case_run2.name, "test_variant_truncation2");
        assert_eq!(test_case_run2.parent_name, "testsuite");
        assert_eq!(test_case_run2.classname, "");
        assert_eq!(test_case_run2.status, TestCaseRunStatus::Failure as i32);
        assert_eq!(test_case_run2.status_output_message, "Test failed");
        assert_eq!(test_case_run2.file, "test.java");
        assert_eq!(test_case_run2.attempt_number, 0);
        assert!(!test_case_run2.is_quarantined);
        // Verify that the ID field is set correctly for test_case_run2
        assert_eq!(
            test_case_run2.id,
            gen_info_id_base(
                org_slug.as_str(),
                repo.repo_full_name().as_str(),
                Some("test.java"),
                None, // No classname for test_case_run2
                Some("testsuite"),
                Some("test_variant_truncation2"),
                None,
                "",
            )
        );
    }

    #[test]
    fn test_into_test_case_runs_with_custom_id() {
        // Test that custom IDs from xcresult (or other sources) are preserved
        let mut junit_parser = JunitParser::new();
        let file_contents = r#"
        <xml version="1.0" encoding="UTF-8"?>
        <testsuites>
            <testsuite name="testsuite" timestamp="2023-10-01T12:00:00Z" time="0.002">
                <testcase id="custom-uuid-1234-5678" file="test.swift" classname="TestClass" name="test_with_custom_id" time="0.001">
                </testcase>
            </testsuite>
        </testsuites>
        "#;
        let parsed_results = junit_parser.parse(BufReader::new(file_contents.as_bytes()));
        assert!(parsed_results.is_ok());

        let org_slug = "org-url-slug".to_string();
        let repo = RepoUrlParts {
            host: "repo-host".into(),
            owner: "repo-owner".into(),
            name: "repo-name".into(),
        };

        let test_case_runs = junit_parser.into_test_case_runs(None, &org_slug, &repo, &[], "");
        assert_eq!(test_case_runs.len(), 1);
        let test_case_run = &test_case_runs[0];

        // Verify that the custom ID from the XML is preserved
        assert_eq!(test_case_run.id, "custom-uuid-1234-5678");
        assert_eq!(test_case_run.name, "test_with_custom_id");
        assert_eq!(test_case_run.file, "test.swift");
        assert_eq!(test_case_run.classname, "TestClass");
    }

    #[test]
    fn test_into_test_case_runs_mixed_ids() {
        // Test mix of custom IDs and generated IDs
        let mut junit_parser = JunitParser::new();
        let file_contents = r#"
        <xml version="1.0" encoding="UTF-8"?>
        <testsuites>
            <testsuite name="testsuite" timestamp="2023-10-01T12:00:00Z" time="0.002">
                <testcase id="xcresult-uuid-abcd" file="test.swift" classname="TestClass" name="test_with_id" time="0.001">
                </testcase>
                <testcase file="test.swift" classname="TestClass" name="test_without_id" time="0.001">
                </testcase>
            </testsuite>
        </testsuites>
        "#;
        let parsed_results = junit_parser.parse(BufReader::new(file_contents.as_bytes()));
        assert!(parsed_results.is_ok());

        let org_slug = "org-url-slug".to_string();
        let repo = RepoUrlParts {
            host: "repo-host".into(),
            owner: "repo-owner".into(),
            name: "repo-name".into(),
        };

        let test_case_runs = junit_parser.into_test_case_runs(None, &org_slug, &repo, &[], "");
        assert_eq!(test_case_runs.len(), 2);

        // First test case should have the custom ID
        let test_case_run1 = &test_case_runs[0];
        assert_eq!(test_case_run1.id, "xcresult-uuid-abcd");
        assert_eq!(test_case_run1.name, "test_with_id");

        // Second test case should have a generated ID
        let test_case_run2 = &test_case_runs[1];
        assert_eq!(
            test_case_run2.id,
            gen_info_id_base(
                org_slug.as_str(),
                repo.repo_full_name().as_str(),
                Some("test.swift"),
                Some("TestClass"),
                Some("testsuite"),
                Some("test_without_id"),
                None,
                "",
            )
        );
        assert_eq!(test_case_run2.name, "test_without_id");
    }

    #[test]
    fn test_into_test_case_runs_original() {
        let mut junit_parser = JunitParser::new();
        let file_contents = r#"
        <xml version="1.0" encoding="UTF-8"?>
        <testsuites>
            <testsuite name="testsuite" timestamp="2023-10-01T12:00:00Z" time="0.002">
                <testcase file="test.java" line="5" classname="test" name="test_variant_truncation1" time="0.001">
                    <failure message="Test failed" type="java.lang.AssertionError">
                        <![CDATA[Expected: <true> but was: <false>]]>
                    </failure>
                </testcase>
                <testcase file="test.java" name="test_variant_truncation2" time="0.001">
                    <failure message="Test failed"/>
                </testcase>
            </testsuite>
        </testsuites>
        "#;
        let parsed_results = junit_parser.parse(BufReader::new(file_contents.as_bytes()));
        assert!(parsed_results.is_ok());

        let org_slug = "org-url-slug".to_string();
        let repo = RepoUrlParts {
            host: "repo-host".into(),
            owner: "repo-owner".into(),
            name: "repo-name".into(),
        };

        let test_case_runs = junit_parser.into_test_case_runs(
            None,
            &org_slug,
            &repo,
            &[gen_info_id_base(
                org_slug.as_str(),
                repo.repo_full_name().as_str(),
                Some("test.java"),
                Some("test"),
                Some("testsuite"),
                Some("test_variant_truncation1"),
                None,
                "",
            )],
            "",
        );
        assert_eq!(test_case_runs.len(), 2);
        let test_case_run1 = &test_case_runs[0];
        assert_eq!(test_case_run1.name, "test_variant_truncation1");
        assert_eq!(test_case_run1.parent_name, "testsuite");
        assert_eq!(test_case_run1.classname, "test");
        assert_eq!(test_case_run1.status, TestCaseRunStatus::Failure as i32);
        assert_eq!(
            test_case_run1.status_output_message,
            "Expected: <true> but was: <false>"
        );
        assert_eq!(test_case_run1.file, "test.java");
        assert_eq!(test_case_run1.attempt_number, 0);
        assert!(test_case_run1.is_quarantined);
        assert_eq!(
            test_case_run1.started_at,
            Some(Timestamp {
                seconds: 1696161600,
                nanos: 0
            })
        );
        assert_eq!(
            test_case_run1.finished_at,
            Some(Timestamp {
                seconds: 1696161600,
                nanos: 1000000
            })
        );
        assert_eq!(test_case_run1.line, 5);
        // Verify that the ID field is set correctly (generated from gen_info_id_base)
        assert_eq!(
            test_case_run1.id,
            gen_info_id_base(
                org_slug.as_str(),
                repo.repo_full_name().as_str(),
                Some("test.java"),
                Some("test"),
                Some("testsuite"),
                Some("test_variant_truncation1"),
                None,
                "",
            )
        );

        let test_case_run2 = &test_case_runs[1];
        assert_eq!(test_case_run2.name, "test_variant_truncation2");
        assert_eq!(test_case_run2.parent_name, "testsuite");
        assert_eq!(test_case_run2.classname, "");
        assert_eq!(test_case_run2.status, TestCaseRunStatus::Failure as i32);
        assert_eq!(test_case_run2.status_output_message, "Test failed");
        assert_eq!(test_case_run2.file, "test.java");
        assert_eq!(test_case_run2.attempt_number, 0);
        assert!(!test_case_run2.is_quarantined);
        // Verify that the ID field is set correctly for test_case_run2
        assert_eq!(
            test_case_run2.id,
            gen_info_id_base(
                org_slug.as_str(),
                repo.repo_full_name().as_str(),
                Some("test.java"),
                None, // No classname for test_case_run2
                Some("testsuite"),
                Some("test_variant_truncation2"),
                None,
                "",
            )
        );
        assert_eq!(
            test_case_run2.started_at,
            Some(Timestamp {
                seconds: 1696161600,
                nanos: 0
            })
        );
        assert_eq!(
            test_case_run2.finished_at,
            Some(Timestamp {
                seconds: 1696161600,
                nanos: 1000000
            })
        );
        assert_eq!(test_case_run2.line, 0);
    }

    #[cfg(feature = "bindings")]
    #[test]
    fn test_bin_parse() {
        use prost::Message;
        use proto::test_context::test_run::{
            TestCaseRun, TestCaseRunStatus, TestReport, TestResult,
        };

        use crate::junit::parser::bin_parse;

        // Parse TestReport
        let test_case_run = TestCaseRun {
            name: "test_case_1".to_string(),
            parent_name: "test_suite_1".to_string(),
            classname: "TestClass".to_string(),
            status: TestCaseRunStatus::Success as i32,
            status_output_message: "Test passed".to_string(),
            file: "test_file.java".to_string(),
            attempt_number: 1,
            is_quarantined: false,
            started_at: None,
            finished_at: None,
            line: 42,
            ..Default::default()
        };

        let test_report = TestReport {
            test_results: vec![TestResult {
                test_case_runs: vec![test_case_run.clone()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut bin_data = Vec::new();
        test_report.encode(&mut bin_data).unwrap();

        let result = bin_parse(&bin_data);
        assert!(result.is_ok());
        let reports = result.unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].test_suites.len(), 1);
        assert_eq!(reports[0].test_suites[0].test_cases.len(), 1);
        assert_eq!(reports[0].test_suites[0].test_cases[0].name, "test_case_1");

        // Parse TestResult directly
        let test_result = TestResult {
            test_case_runs: vec![test_case_run],
            ..Default::default()
        };

        let mut bin_data = Vec::new();
        test_result.encode(&mut bin_data).unwrap();

        let result = bin_parse(&bin_data);
        assert!(result.is_ok());
        let reports = result.unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].test_suites.len(), 1);
        assert_eq!(reports[0].test_suites[0].test_cases.len(), 1);
        assert_eq!(reports[0].test_suites[0].test_cases[0].name, "test_case_1");

        // Invalid binary data should return an error
        let invalid_data = b"invalid binary data";
        let result = bin_parse(invalid_data);
        assert!(result.is_err());
    }

    #[cfg(feature = "bindings")]
    #[test]
    fn test_junit_time_preserved_without_timestamps() {
        use crate::junit::bindings::BindingsTestCase;

        // Test that when JUnit XML has time attribute but no timestamp,
        // the time is preserved through conversion to TestCaseRun
        const INPUT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites>
  <testsuite name="integration_tests" tests="1" failures="0" errors="0">
    <testcase name="test_api_endpoint" status="run" time="2.5"></testcase>
  </testsuite>
</testsuites>
"#;
        let mut junit_parser = JunitParser::new();
        junit_parser
            .parse(BufReader::new(INPUT_XML.as_bytes()))
            .unwrap();

        // Get test case runs - this is what gets stored in internal.bin
        let test_case_runs = junit_parser.into_test_case_runs(
            None,
            &String::from("test-org"),
            &RepoUrlParts {
                host: "github.com".into(),
                owner: "test-owner".into(),
                name: "test-repo".into(),
            },
            &[],
            "",
        );

        assert_eq!(test_case_runs.len(), 1);
        let test_case_run = &test_case_runs[0];

        // Verify that started_at and finished_at are set (using epoch as fallback)
        assert!(test_case_run.started_at.is_some());
        assert!(test_case_run.finished_at.is_some());

        let started_at = test_case_run.started_at.as_ref().unwrap();
        let finished_at = test_case_run.finished_at.as_ref().unwrap();

        // Calculate the time difference
        let time_diff = (finished_at.seconds - started_at.seconds) as f64
            + (finished_at.nanos - started_at.nanos) as f64 / 1_000_000_000.0;

        // The time difference should match the original time attribute (2.5 seconds)
        assert!(
            (time_diff - 2.5).abs() < 0.01,
            "Time difference {} should be approximately 2.5 seconds",
            time_diff
        );

        // Now convert back to BindingsTestCase (simulating reading from internal.bin)
        let bindings_case = BindingsTestCase::from(test_case_run.clone());

        // Verify the time is preserved
        assert!(bindings_case.time.is_some());
        let time = bindings_case.time.unwrap();
        assert!(
            (time - 2.5).abs() < 0.01,
            "Converted time {} should be approximately 2.5 seconds",
            time
        );
    }

    #[test]
    fn test_into_test_case_runs_with_variant() {
        let mut junit_parser = JunitParser::new();
        let file_contents = r#"
        <xml version="1.0" encoding="UTF-8"?>
        <testsuites>
            <testsuite name="testsuite" timestamp="2023-10-01T12:00:00Z" time="0.002">
                <testcase file="test.java" classname="TestClass" name="test_with_variant" time="0.001">
                </testcase>
            </testsuite>
        </testsuites>
        "#;
        let parsed_results = junit_parser.parse(BufReader::new(file_contents.as_bytes()));
        assert!(parsed_results.is_ok());

        let org_slug = "org-url-slug".to_string();
        let repo = RepoUrlParts {
            host: "repo-host".into(),
            owner: "repo-owner".into(),
            name: "repo-name".into(),
        };
        let variant = "test-variant";

        let test_case_runs_with_variant =
            junit_parser
                .clone()
                .into_test_case_runs(None, &org_slug, &repo, &[], variant);

        assert_eq!(test_case_runs_with_variant.len(), 1);
        let test_case_with_variant = &test_case_runs_with_variant[0];

        let expected_id_with_variant = gen_info_id(
            org_slug.as_str(),
            repo.repo_full_name().as_str(),
            Some("test.java"),
            Some("TestClass"),
            Some("testsuite"),
            Some("test_with_variant"),
            None,
            variant,
        );

        assert_eq!(
            test_case_with_variant.id, expected_id_with_variant,
            "ID should be generated with variant included"
        );

        let mut junit_parser_no_variant = JunitParser::new();
        junit_parser_no_variant
            .parse(BufReader::new(file_contents.as_bytes()))
            .unwrap();
        let test_case_runs_no_variant =
            junit_parser_no_variant.into_test_case_runs(None, &org_slug, &repo, &[], "");

        assert_eq!(test_case_runs_no_variant.len(), 1);
        let test_case_no_variant = &test_case_runs_no_variant[0];

        let expected_id_no_variant = gen_info_id_base(
            org_slug.as_str(),
            repo.repo_full_name().as_str(),
            Some("test.java"),
            Some("TestClass"),
            Some("testsuite"),
            Some("test_with_variant"),
            None,
            "",
        );

        assert_eq!(
            test_case_no_variant.id, expected_id_no_variant,
            "ID should be generated without variant"
        );

        assert_ne!(
            test_case_with_variant.id, test_case_no_variant.id,
            "IDs with and without variant should be different"
        );
    }

    #[test]
    fn test_into_test_case_runs_with_suite_level_file() {
        let mut junit_parser = JunitParser::new();
        let file_contents = r#"
        <xml version="1.0" encoding="UTF-8"?>
        <testsuites>
            <testsuite name="testsuite" file="suite_file.java" timestamp="2023-10-01T12:00:00Z" time="0.002">
                <testcase classname="TestClass" name="test_without_file" time="0.001">
                </testcase>
                <testcase file="testcase_file.java" classname="TestClass" name="test_with_file" time="0.001">
                </testcase>
            </testsuite>
        </testsuites>
        "#;
        let parsed_results = junit_parser.parse(BufReader::new(file_contents.as_bytes()));
        assert!(parsed_results.is_ok());

        let org_slug = "org-url-slug".to_string();
        let repo = RepoUrlParts {
            host: "repo-host".into(),
            owner: "repo-owner".into(),
            name: "repo-name".into(),
        };

        let test_case_runs = junit_parser.into_test_case_runs(None, &org_slug, &repo, &[], "");
        assert_eq!(test_case_runs.len(), 2);

        let test_case_run1 = &test_case_runs[0];
        assert_eq!(test_case_run1.name, "test_without_file");
        assert_eq!(
            test_case_run1.file, "suite_file.java",
            "Test case should inherit file from suite"
        );

        let test_case_run2 = &test_case_runs[1];
        assert_eq!(test_case_run2.name, "test_with_file");
        assert_eq!(
            test_case_run2.file, "testcase_file.java",
            "Test case should use its own file when present"
        );
    }

    #[test]
    fn test_into_test_case_runs_with_suite_level_filepath() {
        let mut junit_parser = JunitParser::new();
        let file_contents = r#"
        <xml version="1.0" encoding="UTF-8"?>
        <testsuites>
            <testsuite name="testsuite" filepath="path/to/suite_file.java" timestamp="2023-10-01T12:00:00Z" time="0.002">
                <testcase classname="TestClass" name="test_without_file" time="0.001">
                </testcase>
            </testsuite>
        </testsuites>
        "#;
        let parsed_results = junit_parser.parse(BufReader::new(file_contents.as_bytes()));
        assert!(parsed_results.is_ok());

        let org_slug = "org-url-slug".to_string();
        let repo = RepoUrlParts {
            host: "repo-host".into(),
            owner: "repo-owner".into(),
            name: "repo-name".into(),
        };

        let test_case_runs = junit_parser.into_test_case_runs(None, &org_slug, &repo, &[], "");
        assert_eq!(test_case_runs.len(), 1);

        let test_case_run = &test_case_runs[0];
        assert_eq!(test_case_run.name, "test_without_file");
        assert_eq!(
            test_case_run.file, "path/to/suite_file.java",
            "Test case should inherit filepath from suite"
        );
    }
}
