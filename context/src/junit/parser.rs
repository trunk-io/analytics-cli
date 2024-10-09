use std::io::BufRead;

use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestRerun, TestSuite};
use quick_xml::{
    events::{BytesStart, BytesText, Event},
    Reader,
};
use thiserror::Error;

use super::date_parser::JunitDateParser;

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
}

#[derive(Error, Debug, Copy, Clone, PartialEq, Eq)]
pub enum JunitParseError {
    #[error("could not parse report name")]
    ReportName,
    #[error("no reports found")]
    ReportNotFound,
    #[error("multiple reports found")]
    ReportMultipleFound,
    #[error("report end tag found without start tag")]
    ReportStartTagNotFound,
    #[error("could not parse test suite name")]
    TestSuiteName,
    #[error("test suite found without a report found")]
    TestSuiteReportNotFound,
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
    #[error("system out is empty")]
    SystemOutEmpty,
    #[error("system err is empty")]
    SystemErrEmpty,
    #[error("stack trace is empty")]
    StackTraceEmpty,
}

#[derive(Debug, Clone)]
enum Text {
    SystemOut(Option<String>),
    SystemErr(Option<String>),
    StackTrace(Option<String>),
}

#[derive(Debug, Clone, Default)]
pub struct JunitParser {
    date_parser: JunitDateParser,
    errors: Vec<JunitParseError>,
    reports: Vec<Report>,
    current_report: Option<Report>,
    current_test_suite: Option<TestSuite>,
    current_test_case: Option<TestCase>,
    current_test_rerun: Option<TestRerun>,
    current_text: Option<Text>,
}

impl JunitParser {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn errors(&self) -> &Vec<JunitParseError> {
        &self.errors
    }

    pub fn reports(&self) -> &Vec<Report> {
        &self.reports
    }

    pub fn into_reports(self) -> Vec<Report> {
        self.reports
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
            0 => self.errors.push(JunitParseError::ReportNotFound),
            1 => {
                // There should only be 1 report per JUnit.xml file
            }
            _ => self.errors.push(JunitParseError::ReportMultipleFound),
        };

        Ok(())
    }

    fn match_event(&mut self, event: Event) -> Option<()> {
        match event {
            Event::Eof => return None,
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
                TAG_TEST_RERUN_STACK_TRACE => self.errors.push(JunitParseError::StackTraceEmpty),
                TAG_SYSTEM_OUT => self.errors.push(JunitParseError::SystemOutEmpty),
                TAG_SYSTEM_ERR => self.errors.push(JunitParseError::SystemErrEmpty),
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
            self.set_text_value(&e);
        } else if self.current_test_rerun.is_some() {
            self.set_test_rerun_description(&e);
        } else {
            self.set_test_case_status_description(&e);
        }
    }

    fn open_report(&mut self, e: &BytesStart) {
        let report_name = parse_attr::name(e).unwrap_or_default();
        if report_name.is_empty() {
            self.errors.push(JunitParseError::ReportName);
        }
        let mut report = Report::new(report_name);

        if let Some(timestamp) = parse_attr::timestamp(e, &mut self.date_parser) {
            report.set_timestamp(timestamp);
        }

        if let Some(time) = parse_attr::time(e) {
            report.set_time(time);
        }

        self.current_report = Some(report);
    }

    fn close_report(&mut self) {
        if let Some(report) = self.current_report.take() {
            self.reports.push(report);
        } else {
            self.errors.push(JunitParseError::ReportStartTagNotFound);
        }
    }

    fn open_test_suite(&mut self, e: &BytesStart) {
        let test_suite_name = parse_attr::name(e).unwrap_or_default();
        if test_suite_name.is_empty() {
            self.errors.push(JunitParseError::TestSuiteName);
        };
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
        if let Some(report) = self.current_report.as_mut() {
            if let Some(test_suite) = self.current_test_suite.take() {
                report.add_test_suite(test_suite);
            } else {
                self.errors.push(JunitParseError::TestSuiteStartTagNotFound);
            }
        } else {
            self.errors.push(JunitParseError::TestSuiteReportNotFound);
        }
    }

    fn open_test_case(&mut self, e: &BytesStart) {
        let test_case_name = parse_attr::name(e).unwrap_or_default();
        if test_case_name.is_empty() {
            self.errors.push(JunitParseError::TestCaseName);
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
                self.errors.push(JunitParseError::TestCaseStartTagNotFound);
            }
        } else {
            self.errors.push(JunitParseError::TestCaseTestSuiteNotFound);
        }
    }

    fn set_test_case_status(&mut self, e: &BytesStart) {
        if let Some(test_case) = self.current_test_case.as_mut() {
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
            self.errors
                .push(JunitParseError::TestCaseStatusTestCaseNotFound);
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
                self.errors.push(JunitParseError::TestRerunStartTagNotFound);
            }
        } else {
            self.errors.push(JunitParseError::TestRerunTestCaseNotFound);
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

    use crate::junit::date_parser::JunitDateParser;

    use super::{extra_attrs, unescape_and_truncate};

    pub fn name<'a>(e: &'a BytesStart<'a>) -> Option<Cow<'a, str>> {
        parse_string_attr(e, "name")
    }

    pub fn timestamp(
        e: &BytesStart,
        date_parser: &mut JunitDateParser,
    ) -> Option<DateTime<FixedOffset>> {
        parse_string_attr(e, "timestamp").and_then(|value| date_parser.parse_date(&value))
    }

    pub fn time(e: &BytesStart) -> Option<Duration> {
        parse_string_attr_into_other_type(e, "time")
            .map(|seconds: f64| Duration::from_secs_f64(seconds))
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
}

mod unescape_and_truncate {
    use std::borrow::Cow;

    use quick_xml::events::{attributes::Attribute, BytesText};

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

    fn safe_truncate_cow<'a, const MAX_LEN: usize>(value: Cow<'a, str>) -> Cow<'a, str> {
        match value {
            Cow::Borrowed(b) => Cow::Borrowed(safe_truncate_str::<MAX_LEN>(b)),
            Cow::Owned(b) => Cow::Owned(String::from(safe_truncate_str::<MAX_LEN>(b.as_str()))),
        }
    }
}
