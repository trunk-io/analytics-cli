use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, TimeDelta};
use proto::test_context::test_run::{TestBuildResult, TestResult};
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum};
use quick_junit::Report;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

use crate::junit::{
    bindings::{
        suite::BindingsTestSuite,
        test_case::{BindingsTestCase, BindingsTestCaseStatusStatus},
    },
    parser::JunitParseFlatIssue,
};

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BindingsParseResult {
    pub report: Option<BindingsReport>,
    pub issues: Vec<JunitParseFlatIssue>,
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass_enum, pyclass(eq, eq_int))]
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum BindingsTestBuildResult {
    Unspecified,
    Success,
    Failure,
    Skipped,
    Flaky,
}

// Ideally this would be an enum, but enums are not directly supportted by wasm conversions, so we have to manually map out the options. See: https://github.com/rustwasm/wasm-bindgen/issues/2407
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BazelBuildInformation {
    pub label: String,
    pub result: BindingsTestBuildResult,
    pub max_attempt_number: Option<i32>,
}

#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass(get_all))]
#[cfg_attr(feature = "wasm", wasm_bindgen(getter_with_clone))]
#[derive(Clone, Debug)]
pub struct BindingsReport {
    pub name: String,
    pub uuid: Option<String>,
    pub timestamp: Option<i64>,
    pub timestamp_micros: Option<i64>,
    pub time: Option<f64>,
    pub tests: usize,
    pub failures: usize,
    pub errors: usize,
    pub test_suites: Vec<BindingsTestSuite>,
    pub variant: Option<String>,
    pub bazel_build_information: Option<BazelBuildInformation>,
}

pub fn map_i32_to_bindings_test_build_result(
    result: i32,
) -> anyhow::Result<BindingsTestBuildResult> {
    if result == TestBuildResult::Unspecified as i32 {
        Ok(BindingsTestBuildResult::Unspecified)
    } else if result == TestBuildResult::Success as i32 {
        Ok(BindingsTestBuildResult::Success)
    } else if result == TestBuildResult::Failure as i32 {
        Ok(BindingsTestBuildResult::Failure)
    } else if result == TestBuildResult::Skipped as i32 {
        Ok(BindingsTestBuildResult::Skipped)
    } else if result == TestBuildResult::Flaky as i32 {
        Ok(BindingsTestBuildResult::Flaky)
    } else {
        Err(anyhow::anyhow!("Unknown TestBuildResult: {result}"))
    }
}

impl From<TestResult> for BindingsReport {
    fn from(
        TestResult {
            test_case_runs,
            // trunk-ignore(clippy/deprecated)
            uploader_metadata,
            test_build_information,
        }: TestResult,
    ) -> Self {
        let test_cases: Vec<BindingsTestCase> = test_case_runs
            .into_iter()
            .map(BindingsTestCase::from)
            .collect();
        let parent_name_map: HashMap<String, Vec<BindingsTestCase>> =
            test_cases.iter().fold(HashMap::new(), |mut acc, testcase| {
                if let Some(parent_name) = testcase.extra.get("parent_name") {
                    acc.entry(parent_name.clone())
                        .or_default()
                        .push(testcase.to_owned());
                }
                acc
            });
        let test_suites: Vec<BindingsTestSuite> = parent_name_map
            .into_iter()
            .map(|(name, testcases)| {
                let tests = testcases.len();
                let disabled = testcases
                    .iter()
                    .filter(|tc| tc.status.status == BindingsTestCaseStatusStatus::Skipped)
                    .count();
                let failures = testcases
                    .iter()
                    .filter(|tc| tc.status.status == BindingsTestCaseStatusStatus::NonSuccess)
                    .count();
                let timestamp = testcases.iter().map(|tc| tc.timestamp.unwrap_or(0)).max();
                let timestamp_micros = testcases
                    .iter()
                    .map(|tc| tc.timestamp_micros.unwrap_or(0))
                    .max();
                let time = testcases.iter().map(|tc| tc.time.unwrap_or(0.0)).sum();
                BindingsTestSuite {
                    name,
                    tests,
                    disabled,
                    errors: 0,
                    failures,
                    timestamp,
                    timestamp_micros,
                    time: Some(time),
                    test_cases: testcases,
                    properties: vec![],
                    system_out: None,
                    system_err: None,
                    extra: HashMap::new(),
                }
            })
            .collect();
        let (report_time, report_failures, report_tests) =
            test_suites.iter().fold((0.0, 0, 0), |acc, ts| {
                (
                    acc.0 + ts.time.unwrap_or(0.0),
                    acc.1 + ts.failures,
                    acc.2 + ts.tests,
                )
            });
        let (name, timestamp, timestamp_micros, variant) = match uploader_metadata {
            Some(t) => {
                let upload_time = t.upload_time.clone().unwrap_or_default();
                (
                    t.origin,
                    Some(upload_time.seconds),
                    Some(
                        chrono::Duration::nanoseconds(upload_time.nanos as i64)
                            .num_microseconds()
                            .unwrap_or_default(),
                    ),
                    Some(t.variant),
                )
            }
            None => ("Unknown".to_string(), None, None, None),
        };
        let bazel_build_information = match test_build_information {
            Some(proto::test_context::test_run::test_result::TestBuildInformation::BazelBuildInformation(
                bazel_build_information,
            )) => Some(BazelBuildInformation {
                label: bazel_build_information.label,
                result: map_i32_to_bindings_test_build_result(bazel_build_information.result)
                    .unwrap_or(BindingsTestBuildResult::Unspecified),
                max_attempt_number: bazel_build_information.max_attempt_number.map(|number| number.number),
            }),
            _ => None,
        };
        BindingsReport {
            name,
            test_suites,
            time: Some(report_time),
            uuid: None,
            timestamp,
            timestamp_micros,
            errors: 0,
            failures: report_failures,
            tests: report_tests,
            variant,
            bazel_build_information,
        }
    }
}

impl From<Report> for BindingsReport {
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
            timestamp_micros: timestamp.map(|t| t.timestamp_micros()),
            time: time.map(|t| t.as_secs_f64()),
            tests,
            failures,
            errors,
            test_suites: test_suites
                .into_iter()
                .map(BindingsTestSuite::from)
                .collect(),
            variant: None,
            bazel_build_information: None,
        }
    }
}

impl From<BindingsReport> for Report {
    fn from(val: BindingsReport) -> Self {
        let BindingsReport {
            name,
            uuid,
            timestamp: _,
            timestamp_micros,
            time,
            tests,
            failures,
            errors,
            test_suites,
            variant: _,
            bazel_build_information: _,
        } = val;
        // NOTE: Cannot make a UUID without a `&'static str`
        let _ = uuid;
        Report {
            name: name.into(),
            uuid: None,
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
            tests,
            failures,
            errors,
            test_suites: test_suites
                .into_iter()
                .map(BindingsTestSuite::into)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::BufReader;

    use proto::test_context::test_run::{
        AttemptNumber, CodeOwner, LineNumber, TestCaseRun, TestCaseRunStatus, TestResult,
    };

    use crate::junit::bindings::{BindingsReport, BindingsTestCaseStatusStatus};
    use crate::junit::parser::JunitParser;
    use crate::junit::validator::{JunitValidationLevel, JunitValidationType};

    #[cfg(feature = "bindings")]
    #[test]
    fn parse_quick_junit_to_bindings() {
        use std::io::BufReader;

        use crate::junit::parser::JunitParser;
        const INPUT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="my-test-run" tests="2" failures="1" errors="0">
    <testsuite name="my-test-suite" file="path/to/my/test.js" tests="2" disabled="0" errors="0" failures="1">
        <testcase name="success-case">
        </testcase>
        <testcase name="failure-case">
            <failure/>
        </testcase>
    </testsuite>
</testsuites>
"#;
        let mut junit_parser = JunitParser::new();
        junit_parser
            .parse(BufReader::new(INPUT_XML.as_bytes()))
            .unwrap();
        let reports = junit_parser.into_reports();
        assert_eq!(reports.len(), 1);
        let bindings_report = BindingsReport::from(reports[0].clone());
        assert_eq!(bindings_report.name, "my-test-run");
        assert_eq!(bindings_report.tests, 2);
        assert_eq!(bindings_report.failures, 1);
        assert_eq!(bindings_report.errors, 0);
        assert_eq!(bindings_report.test_suites.len(), 1);
        let test_suite = &bindings_report.test_suites[0];
        assert_eq!(test_suite.name, "my-test-suite");
        assert_eq!(test_suite.tests, 2);
        assert_eq!(test_suite.disabled, 0);
        assert_eq!(test_suite.errors, 0);
        assert_eq!(test_suite.failures, 1);
        assert_eq!(test_suite.test_cases.len(), 2);
        let test_case1 = &test_suite.test_cases[0];
        assert_eq!(test_case1.name, "success-case");
        assert_eq!(test_case1.classname, None);
        assert_eq!(test_case1.assertions, None);
        assert_eq!(test_case1.timestamp, None);
        assert_eq!(test_case1.timestamp_micros, None);
        assert_eq!(test_case1.time, None);
        assert_eq!(test_case1.system_out, None);
        assert_eq!(test_case1.system_err, None);
        assert_eq!(test_case1.extra.len(), 1);
        assert_eq!(test_case1.extra["file"], "path/to/my/test.js");
        assert_eq!(test_case1.properties.len(), 0);
        let test_case2 = &test_suite.test_cases[1];
        assert_eq!(test_case2.name, "failure-case");
        assert_eq!(test_case2.classname, None);
        assert_eq!(test_case2.assertions, None);
        assert_eq!(test_case2.timestamp, None);
        assert_eq!(test_case2.timestamp_micros, None);
        assert_eq!(test_case2.time, None);
        assert_eq!(test_case2.system_out, None);
        assert_eq!(test_case2.system_err, None);
        assert_eq!(test_case2.extra.len(), 1);
        assert_eq!(test_case2.extra["file"], "path/to/my/test.js");
        assert_eq!(test_case2.properties.len(), 0);
    }

    #[cfg(feature = "bindings")]
    #[test]
    fn parse_test_report_to_bindings() {
        use prost_wkt_types::Timestamp;
        use proto::test_context::test_run::TestOutput;

        use crate::junit::validator::validate;
        let test_started_at = Timestamp {
            seconds: 1000,
            nanos: 0,
        };
        let test_finished_at = Timestamp {
            seconds: 2000,
            nanos: 0,
        };
        let codeowner1 = CodeOwner {
            name: "@user".into(),
        };
        let test1 = TestCaseRun {
            id: "test_id1".into(),
            name: "test_name".into(),
            classname: "test_classname".into(),
            file: "test_file".into(),
            parent_name: "test_parent_name1".into(),
            // trunk-ignore(clippy/deprecated)
            line: 0,
            line_number: Some(LineNumber { number: 1 }),
            status: TestCaseRunStatus::Success.into(),
            // trunk-ignore(clippy/deprecated)
            attempt_number: 0,
            attempt_index: Some(AttemptNumber { number: 1 }),
            started_at: Some(test_started_at.clone()),
            finished_at: Some(test_finished_at.clone()),
            // trunk-ignore(clippy/deprecated)
            status_output_message: "test_status_output_message".into(),
            codeowners: vec![codeowner1],
            test_output: Some(TestOutput {
                message: "test_failure_message".into(),
                text: "".into(),
                system_out: "".into(),
                system_err: "".into(),
            }),
            ..Default::default()
        };

        let test2 = TestCaseRun {
            id: "test_id2".into(),
            name: "test_name".into(),
            classname: "test_classname".into(),
            file: "test_file".into(),
            parent_name: "test_parent_name2".into(),
            // trunk-ignore(clippy/deprecated)
            line: 1,
            status: TestCaseRunStatus::Failure.into(),
            // trunk-ignore(clippy/deprecated)
            attempt_number: 1,
            started_at: Some(test_started_at.clone()),
            finished_at: Some(test_finished_at),
            // trunk-ignore(clippy/deprecated)
            status_output_message: "test_status_output_message".into(),
            test_output: Some(TestOutput {
                message: "".into(),
                text: "test_status_output_message".into(),
                system_out: "".into(),
                system_err: "".into(),
            }),
            ..Default::default()
        };

        let mut test_result = TestResult::default();
        test_result.test_case_runs.push(test1.clone());
        test_result.test_case_runs.push(test1.clone());
        test_result.test_case_runs.push(test2.clone());

        let converted_bindings: BindingsReport = test_result.into();
        assert_eq!(converted_bindings.test_suites.len(), 2);
        let mut test_suite1 = &converted_bindings.test_suites[0];
        let mut test_suite2 = &converted_bindings.test_suites[1];
        if test_suite1.name == "test_parent_name1" {
            assert_eq!(test_suite1.tests, 2);
            assert_eq!(test_suite2.tests, 1);
        } else {
            assert_eq!(test_suite1.tests, 1);
            assert_eq!(test_suite2.tests, 2);
            // swap them for convenience
            (test_suite1, test_suite2) = (test_suite2, test_suite1);
        }
        let test_case1 = &test_suite1.test_cases[0];
        assert_eq!(test_case1.name, test1.name);
        assert_eq!(test_case1.classname, Some(test1.classname));
        assert_eq!(test_case1.assertions, None);
        assert_eq!(
            test_case1.timestamp,
            Some(test1.started_at.clone().unwrap().seconds)
        );
        assert_eq!(
            test_case1.timestamp_micros,
            Some(
                test1.started_at.clone().unwrap().seconds * 1000000
                    + test1.started_at.unwrap().nanos as i64 / 1000
            )
        );
        assert_eq!(test_case1.time, Some(1000.0));
        assert_eq!(test_case1.system_out, None);
        assert_eq!(test_case1.system_err, None);
        assert!(test_case1.status.success.is_some());
        assert_eq!(test_case1.extra["id"], test1.id);
        assert_eq!(test_case1.extra["file"], test1.file);
        assert_eq!(
            test_case1.extra["line"],
            test1.line_number.unwrap().number.to_string()
        );
        assert_eq!(
            test_case1.extra["attempt_number"],
            test1.attempt_index.unwrap().number.to_string()
        );
        assert_eq!(test_case1.properties.len(), 0);
        assert_eq!(test_case1.codeowners.clone().unwrap().len(), 1);
        assert_eq!(test_case1.codeowners.clone().unwrap()[0], "@user");

        assert_eq!(test_suite2.test_cases.len(), 1);
        let test_case2 = &test_suite2.test_cases[0];
        assert_eq!(test_case2.name, test2.name);
        assert_eq!(test_case2.classname, Some(test2.classname));
        assert_eq!(test_case2.assertions, None);
        assert_eq!(
            test_case2.timestamp,
            Some(test2.started_at.clone().unwrap().seconds)
        );
        assert_eq!(
            test_case2.timestamp_micros,
            Some(
                test2.started_at.clone().unwrap().seconds * 1000000
                    + test2.started_at.unwrap().nanos as i64 / 1000
            )
        );
        assert_eq!(test_case2.time, Some(1000.0));
        assert_eq!(test_case2.system_out, None);
        assert_eq!(test_case2.system_err, None);
        assert_eq!(
            test_case2.status.non_success.as_ref().unwrap().description,
            Some(test2.test_output.clone().unwrap().text)
        );
        assert_eq!(
            test_case2.status.non_success.as_ref().unwrap().message,
            None
        );
        assert_eq!(test_case2.extra["id"], test2.id);
        assert_eq!(test_case2.extra["file"], test2.file);
        // trunk-ignore(clippy/deprecated)
        assert_eq!(test_case2.extra["line"], test2.line.to_string());
        assert_eq!(
            test_case2.extra["attempt_number"],
            // trunk-ignore(clippy/deprecated)
            test2.attempt_number.to_string()
        );
        assert_eq!(test_case2.properties.len(), 0);
        assert_eq!(test_case2.codeowners.clone().unwrap().len(), 0);

        // verify that the test report is valid
        let results = validate(
            &converted_bindings,
            &None,
            chrono::Utc::now().fixed_offset(),
        );
        assert_eq!(results.all_issues_owned().len(), 1);
        results
            .all_issues_owned()
            .sort_by(|a, b| a.error_message.cmp(&b.error_message));
        results
            .all_issues_owned()
            .iter()
            .enumerate()
            .for_each(|issue| {
                assert_eq!(issue.1.level, JunitValidationLevel::SubOptimal);
                if issue.0 == 0 {
                    assert_eq!(issue.1.error_type, JunitValidationType::Report);
                    assert_eq!(
                        issue.1.error_message,
                        "report has old (> 24 hour(s)) timestamps"
                    );
                } else {
                    assert_eq!(issue.1.error_type, JunitValidationType::TestCase);
                    assert_eq!(issue.1.error_message, "test case id is not a valid uuidv5");
                }
            });
        assert_eq!(results.test_suites.len(), 2);
        assert_eq!(results.valid_test_suites.len(), 2);
        assert_eq!(
            results.valid_test_suites[0].test_cases.len(),
            converted_bindings.test_suites[0].tests
        );
        assert_eq!(
            results.valid_test_suites[1].test_cases.len(),
            converted_bindings.test_suites[1].tests
        );
    }
    #[cfg(feature = "bindings")]
    #[test]
    fn test_junit_conversion_paths() {
        use crate::repo::RepoUrlParts;

        let mut junit_parser = JunitParser::new();
        let file_contents = r#"
        <xml version="1.0" encoding="UTF-8"?>
        <testsuites>
            <testsuite name="testsuite" time="0.002">
                <testcase file="test.java" line="5" timestamp="2023-10-01T12:00:00Z" classname="test" name="test_variant_truncation1" time="0.001">
                    <failure message="Test failed" type="java.lang.AssertionError">
                        <![CDATA[Expected: <true> but was: <false>]]>
                    </failure>
                </testcase>
                <testcase file="test.java" name="test_variant_truncation2" timestamp="2023-10-01T12:00:00Z" time="0.001" />
            </testsuite>
        </testsuites>
        "#;
        let parsed_results = junit_parser.parse(BufReader::new(file_contents.as_bytes()));
        assert!(parsed_results.is_ok());

        // Get test case runs from parser
        let test_case_runs = junit_parser.into_test_case_runs(
            None,
            &String::from(""),
            &RepoUrlParts {
                host: "".into(),
                owner: "".into(),
                name: "".into(),
            },
            &[],
            "",
        );
        assert_eq!(test_case_runs.len(), 2);

        // Convert test case runs to bindings
        let bindings_from_runs: Vec<crate::junit::bindings::BindingsTestCase> =
            test_case_runs.into_iter().map(|run| run.into()).collect();

        // Get reports and convert directly to bindings
        let mut junit_parser = JunitParser::new();
        junit_parser
            .parse(BufReader::new(file_contents.as_bytes()))
            .unwrap();
        let reports = junit_parser.into_reports();
        assert_eq!(reports.len(), 1);

        let bindings_from_reports: Vec<crate::junit::bindings::BindingsTestCase> = reports[0]
            .test_suites
            .iter()
            .flat_map(|suite| suite.test_cases.iter().map(|case| case.clone().into()))
            .collect();

        // Compare the two conversion paths
        assert_eq!(bindings_from_runs.len(), bindings_from_reports.len());

        for (run_binding, report_binding) in
            bindings_from_runs.iter().zip(bindings_from_reports.iter())
        {
            assert_eq!(run_binding.classname, report_binding.classname);
            assert_eq!(run_binding.status.status, report_binding.status.status);
            assert_eq!(run_binding.timestamp, report_binding.timestamp);
            assert_eq!(
                run_binding.timestamp_micros,
                report_binding.timestamp_micros
            );
            assert_eq!(run_binding.time, report_binding.time);
            assert_eq!(run_binding.system_out, report_binding.system_out);
            assert_eq!(run_binding.system_err, report_binding.system_err);
            if run_binding.status.status == BindingsTestCaseStatusStatus::NonSuccess {
                assert_eq!(
                    run_binding.status.non_success.as_ref().unwrap().description,
                    Some("Expected: <true> but was: <false>".into())
                );
                assert_eq!(
                    run_binding.status.non_success.as_ref().unwrap().message,
                    Some("Test failed".into())
                );
            }
            // check that the properties match
            for property in run_binding.properties.iter() {
                if let Some(report_property) = report_binding
                    .properties
                    .iter()
                    .find(|p| p.name == property.name)
                {
                    assert_eq!(property.value, report_property.value);
                } else {
                    panic!("Property {} not found in report binding", property.name);
                }
            }
            assert_eq!(
                run_binding.extra().get("file"),
                report_binding.extra().get("file")
            );
        }
    }

    #[cfg(feature = "bindings")]
    #[test]
    fn test_validate_preserves_codeowners_in_valid_test_suites() {
        use prost_wkt_types::Timestamp;

        use crate::junit::validator::validate;
        let test_started_at = Timestamp {
            seconds: chrono::Utc::now().timestamp(),
            nanos: 0,
        };
        let test_finished_at = Timestamp {
            seconds: test_started_at.seconds + 1,
            nanos: 0,
        };
        let codeowner1 = CodeOwner {
            name: "@user1".into(),
        };
        let codeowner2 = CodeOwner {
            name: "@user2".into(),
        };
        let test1 = TestCaseRun {
            id: "test_id1".into(),
            name: "test_name1".into(),
            classname: "test_classname".into(),
            file: "test_file1.java".into(),
            parent_name: "test_parent_name1".into(),
            // trunk-ignore(clippy/deprecated)
            line: 1,
            status: TestCaseRunStatus::Success.into(),
            // trunk-ignore(clippy/deprecated)
            attempt_number: 1,
            started_at: Some(test_started_at.clone()),
            finished_at: Some(test_finished_at.clone()),
            // trunk-ignore(clippy/deprecated)
            status_output_message: "".into(),
            codeowners: vec![codeowner1.clone()],
            test_output: None,
            ..Default::default()
        };

        let test2 = TestCaseRun {
            id: "test_id2".into(),
            name: "test_name2".into(),
            classname: "test_classname".into(),
            file: "test_file2.java".into(),
            parent_name: "test_parent_name1".into(),
            // trunk-ignore(clippy/deprecated)
            line: 2,
            status: TestCaseRunStatus::Success.into(),
            // trunk-ignore(clippy/deprecated)
            attempt_number: 1,
            started_at: Some(test_started_at.clone()),
            finished_at: Some(test_finished_at.clone()),
            // trunk-ignore(clippy/deprecated)
            status_output_message: "".into(),
            codeowners: vec![codeowner2.clone()],
            test_output: None,
            ..Default::default()
        };

        let mut test_result = TestResult::default();
        test_result.test_case_runs.push(test1.clone());
        test_result.test_case_runs.push(test2.clone());

        let converted_bindings: BindingsReport = test_result.into();

        // Verify codeowners are present in the original report
        assert_eq!(converted_bindings.test_suites.len(), 1);
        let original_test_suite = &converted_bindings.test_suites[0];
        assert_eq!(original_test_suite.test_cases.len(), 2);
        assert_eq!(
            original_test_suite.test_cases[0].codeowners,
            Some(vec!["@user1".to_string()])
        );
        assert_eq!(
            original_test_suite.test_cases[1].codeowners,
            Some(vec!["@user2".to_string()])
        );

        // Validate the report
        let validation_result = validate(
            &converted_bindings,
            &None,
            chrono::Utc::now().fixed_offset(),
        );

        // Verify that valid_test_suites preserves codeowners
        assert_eq!(validation_result.valid_test_suites.len(), 1);
        let valid_test_suite = &validation_result.valid_test_suites[0];
        assert_eq!(valid_test_suite.test_cases.len(), 2);

        // Find test cases by name to match them up
        let valid_test_case1 = valid_test_suite
            .test_cases
            .iter()
            .find(|tc| tc.name == "test_name1")
            .expect("test_name1 should be in valid_test_suites");
        let valid_test_case2 = valid_test_suite
            .test_cases
            .iter()
            .find(|tc| tc.name == "test_name2")
            .expect("test_name2 should be in valid_test_suites");

        // Verify codeowners are preserved
        assert_eq!(
            valid_test_case1.codeowners,
            Some(vec!["@user1".to_string()]),
            "codeowners for test_name1 should be preserved"
        );
        assert_eq!(
            valid_test_case2.codeowners,
            Some(vec!["@user2".to_string()]),
            "codeowners for test_name2 should be preserved"
        );
    }
}
