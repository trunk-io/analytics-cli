use indexmap::indexmap;
use quick_junit::*;
use serde_json::Value;
use std::str;
use std::{fs, process::Command};

#[derive(Debug, Clone)]
pub struct XCResultFile {
    pub path: String,
    results_obj: serde_json::Value,
}

const LEGACY_FLAG_MIN_VERSION: i32 = 70;

fn xcrun(args: Vec<&str>) -> Result<String, anyhow::Error> {
    if !cfg!(target_os = "macos") {
        return Err(anyhow::anyhow!("xcrun is only available on macOS"));
    }
    let mut cmd = Command::new("xcrun");
    let bin = cmd.args(args);
    let output = match bin.output() {
        Ok(val) => val,
        Err(_) => {
            return Err(anyhow::anyhow!(
                "failed to run xcrun -- please make sure you have xcode installed"
            ))
        }
    };
    let results_obj_raw = match String::from_utf8(output.stdout) {
        Ok(val) => val,
        Err(_) => return Err(anyhow::anyhow!("got non UTF-8 data from xcrun output")),
    };
    Ok(results_obj_raw)
}

fn xcrun_version() -> Result<i32, anyhow::Error> {
    let version_raw = xcrun(vec!["--version"])?;
    // regex to match version where the output looks like xcrun version 70.
    let re = regex::Regex::new(r"xcrun version (\d+)").unwrap();
    Ok(match re.captures(&version_raw.to_string()) {
        Some(val) => val.get(1).unwrap().as_str().parse::<i32>().unwrap_or(0),
        None => return Err(anyhow::anyhow!("failed to parse xcrun version")),
    })
}

fn xcresulttool(
    path: &str,
    options: Option<Vec<&str>>,
) -> Result<serde_json::Value, anyhow::Error> {
    let mut base_args = vec!["xcresulttool", "get", "--path", path, "--format", "json"];
    let version = xcrun_version()?;
    if version >= LEGACY_FLAG_MIN_VERSION {
        base_args.push("--legacy");
    }
    if let Some(val) = options {
        base_args.extend(val);
    }
    let output = xcrun(base_args)?;
    match serde_json::from_str(&output) {
        Ok(val) => Ok(val),
        Err(_) => Err(anyhow::anyhow!("failed to parse json from xcrun output")),
    }
}

impl XCResultFile {
    pub fn new(path: String) -> Result<XCResultFile, anyhow::Error> {
        let binding = match fs::canonicalize(path.clone()) {
            Ok(val) => val,
            Err(_) => return Err(anyhow::anyhow!("failed to get absolute path")),
        };
        let absolute_path = binding.to_str().unwrap_or("");
        let results_obj = match xcresulttool(absolute_path, None) {
            Ok(val) => val,
            Err(e) => return Err(e),
        };
        Ok(XCResultFile {
            path: absolute_path.to_string(),
            results_obj,
        })
    }

    fn find_tests(&self, id: &str) -> Result<serde_json::Value, anyhow::Error> {
        xcresulttool(&self.path, Some(vec!["--id", id]))
    }

    fn generate_id(&self, raw_id: String) -> String {
        return uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, raw_id.as_bytes()).to_string();
    }

    fn junit_testcase(
        &self,
        action: &serde_json::Value,
        testcase: &serde_json::Value,
        testcase_group: &serde_json::Value,
    ) -> TestCase {
        let name = testcase
            .get("name")
            .and_then(|r| r.get("_value"))
            .unwrap()
            .as_str()
            .unwrap();
        let raw_status = testcase
            .get("testStatus")
            .and_then(|r| r.get("_value"))
            .unwrap()
            .as_str()
            .unwrap();
        let mut testcase_status = match raw_status {
            "Error" => TestCaseStatus::non_success(NonSuccessKind::Error),
            "Failure" => TestCaseStatus::non_success(NonSuccessKind::Failure),
            "Skipped" => TestCaseStatus::skipped(),
            "Success" => TestCaseStatus::success(),
            _ => TestCaseStatus::non_success(NonSuccessKind::Error),
        };
        let mut uri = String::new();
        if raw_status == "Failure" {
            let mut failures = action
                .get("actionResult")
                .and_then(|r| r.get("issues"))
                .and_then(|r| r.get("testFailureSummaries"))
                .and_then(|r| r.get("_values"))
                .unwrap()
                .as_array()
                .unwrap()
                .iter();
            let failure = failures.find(|f| {
                f.get("testCaseName")
                    .and_then(|r| r.get("_value"))
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .replace('.', "/")
                    == testcase
                        .get("identifier")
                        .and_then(|r| r.get("_value"))
                        .unwrap()
                        .as_str()
                        .unwrap()
            });
            testcase_status.set_message(
                failure
                    .unwrap()
                    .get("message")
                    .and_then(|r| r.get("_value"))
                    .unwrap()
                    .as_str()
                    .unwrap(),
            );
            uri = failure
                .unwrap()
                .get("documentLocationInCreatingWorkspace")
                .and_then(|r| r.get("url"))
                .unwrap()
                .get("_value")
                .unwrap()
                .as_str()
                .unwrap()
                .replace("file://", "");
        }
        let id = self.generate_id(
            testcase
                .get("identifier")
                .and_then(|r| r.get("_value"))
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
        );
        let mut testcase_junit = TestCase::new(name, testcase_status);
        let file_components = uri.split('#').collect::<Vec<&str>>();
        let mut index_map = indexmap! {
           XmlString::new("id") => XmlString::new(id),
        };
        if file_components.len() == 2 {
            index_map.append(&mut indexmap! {
                XmlString::new("file") => XmlString::new(file_components[0]),
            });
        }
        testcase_junit.extra = index_map;
        testcase_junit.set_classname(
            testcase_group
                .get("name")
                .and_then(|r| r.get("_value"))
                .unwrap()
                .as_str()
                .unwrap(),
        );

        let duration = testcase
            .get("duration")
            .and_then(|r| r.get("_value"))
            .unwrap()
            .as_str()
            .unwrap()
            .parse::<f32>()
            .unwrap();
        testcase_junit.set_time(std::time::Duration::from_secs_f32(duration));
        testcase_junit
    }

    fn junit_testsuite(
        &self,
        action: &serde_json::Value,
        testsuite: &serde_json::Value,
    ) -> TestSuite {
        let mut testsuite_junit = TestSuite::new(
            testsuite
                .get("name")
                .and_then(|r| r.get("_value"))
                .unwrap()
                .as_str()
                .unwrap(),
        );
        let index_map = indexmap! {
           XmlString::new("id") => XmlString::new(
                self.generate_id(
                    testsuite
                        .get("identifierURL")
                        .and_then(|r| r.get("_value"))
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_string(),
               ))
        };
        testsuite_junit.extra = index_map;
        let duration = testsuite
            .get("duration")
            .and_then(|r| r.get("_value"))
            .unwrap()
            .as_str()
            .unwrap()
            .parse::<f32>()
            .unwrap();
        testsuite_junit.set_time(std::time::Duration::from_secs_f32(duration));
        let testcase_groups = testsuite
            .get("subtests")
            .and_then(|t| t.get("_values"))
            .unwrap()
            .as_array()
            .unwrap();
        for testcase_group in testcase_groups {
            let testcases = testcase_group
                .get("subtests")
                .and_then(|t| t.get("_values"))
                .unwrap()
                .as_array()
                .unwrap();
            for testcase in testcases {
                let testcase_xml = self.junit_testcase(action, testcase, testcase_group);
                testsuite_junit.add_test_case(testcase_xml);
            }
        }
        testsuite_junit
    }

    fn junit_report(&self, action: &serde_json::Value) -> Report {
        let mut testsuites_junit = Report::new("name");
        const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.f%z";
        let ended_time = chrono::DateTime::parse_from_str(
            action
                .get("endedTime")
                .and_then(|r| r.get("_value"))
                .unwrap()
                .as_str()
                .unwrap(),
            TIME_FORMAT,
        )
        .unwrap();
        let started_time = chrono::DateTime::parse_from_str(
            action
                .get("startedTime")
                .and_then(|r| r.get("_value"))
                .unwrap()
                .as_str()
                .unwrap(),
            TIME_FORMAT,
        )
        .unwrap();
        let duration =
            (ended_time.timestamp_millis() - started_time.timestamp_millis()) as f32 / 1000.0;
        testsuites_junit.set_time(std::time::Duration::from_secs_f32(duration));
        let empty_val = Value::from("");
        let found_tests = self
            .find_tests(
                action
                    .get("actionResult")
                    .and_then(|r| r.get("testsRef"))
                    .and_then(|r| r.get("id"))
                    .and_then(|r| r.get("_value"))
                    .unwrap_or(&empty_val)
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
        let test_summaries = match found_tests.get("summaries").and_then(|r| r.get("_values")) {
            Some(val) => val.as_array().unwrap(),
            None => return testsuites_junit,
        };
        for test_summary in test_summaries {
            let testable_summaries = test_summary
                .get("testableSummaries")
                .and_then(|t| t.get("_values"))
                .unwrap()
                .as_array()
                .unwrap();
            for testable_summary in testable_summaries {
                let top_level_tests = testable_summary
                    .get("tests")
                    .and_then(|t| t.get("_values"))
                    .unwrap()
                    .as_array()
                    .unwrap();
                for top_level_test in top_level_tests {
                    let testsuites = top_level_test
                        .get("subtests")
                        .and_then(|t| t.get("_values"))
                        .unwrap()
                        .as_array()
                        .unwrap();
                    for testsuite in testsuites {
                        let testsuite_junit = self.junit_testsuite(action, testsuite);
                        testsuites_junit.add_test_suite(testsuite_junit);
                    }
                }
            }
        }
        testsuites_junit
    }

    pub fn generate_junits(&self) -> Vec<Report> {
        let mut report_junits: Vec<Report> = Vec::new();
        if let Some(actions) = self
            .results_obj
            .get("actions")
            .and_then(|a| a.get("_values"))
        {
            for action in actions.as_array().unwrap() {
                let report_junit = self.junit_report(action);
                // only add the report if it has test suites
                // xcresult stores build actions
                if !report_junit.test_suites.is_empty() {
                    report_junits.push(report_junit);
                }
            }
        }
        report_junits
    }
}
