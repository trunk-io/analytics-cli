use indexmap::indexmap;
use lazy_static::lazy_static;
use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestSuite, XmlString};
use std::str;
use std::{fs, process::Command};

const RESULTS_FIELD_VALUE: &str = "_value";
const RESULTS_FIELD_VALUES: &str = "_values";

#[derive(Debug, Clone)]
pub struct XCResult {
    pub path: String,
    results_obj: serde_json::Value,
}

const LEGACY_FLAG_MIN_VERSION: i32 = 70;

fn xcrun<T: AsRef<str>>(args: &[T]) -> anyhow::Result<String> {
    if !cfg!(target_os = "macos") {
        return Err(anyhow::anyhow!("xcrun is only available on macOS"));
    }
    let mut cmd = Command::new("xcrun");
    let bin = cmd.args(args.iter().map(|arg| arg.as_ref()));
    let output = bin.output().map_err(|_| {
        anyhow::anyhow!("failed to run xcrun -- please make sure you have xcode installed")
    })?;
    let result = String::from_utf8(output.stdout)
        .map_err(|_| anyhow::anyhow!("got non UTF-8 data from xcrun output"))?;
    Ok(result)
}

fn xcrun_version() -> anyhow::Result<i32> {
    let version_raw = xcrun(&["--version"])?;
    lazy_static! {
        // regex to match version where the output looks like xcrun version 70.
        static ref RE: regex::Regex = regex::Regex::new(r"xcrun version (\d+)").unwrap();
    }
    RE.captures(&version_raw)
        .and_then(|capture_group| capture_group.get(1))
        .map(|version| Ok(version.as_str().parse::<i32>().ok().unwrap_or_default()))
        .unwrap_or_else(|| Err(anyhow::anyhow!("failed to parse xcrun version")))
}

fn xcresulttool<T: AsRef<str>>(
    path: T,
    options: Option<&[T]>,
) -> anyhow::Result<serde_json::Value> {
    let mut base_args = vec![
        "xcresulttool",
        "get",
        "--path",
        path.as_ref(),
        "--format",
        "json",
    ];
    let version = xcrun_version()?;
    if version >= LEGACY_FLAG_MIN_VERSION {
        base_args.push("--legacy");
    }
    if let Some(val) = options {
        base_args.extend(val.iter().map(|arg| arg.as_ref()));
    }
    let output = xcrun(&base_args)?;
    serde_json::from_str(&output)
        .map_err(|_| anyhow::anyhow!("failed to parse json from xcrun output"))
}

impl XCResult {
    pub fn new<T: AsRef<str>>(path: T) -> anyhow::Result<XCResult> {
        let binding = fs::canonicalize(path.as_ref())
            .map_err(|_| anyhow::anyhow!("failed to get absolute path -- is the path correct?"))?;
        let absolute_path = binding.to_str().unwrap_or_default();
        let results_obj = xcresulttool(absolute_path, None)?;
        Ok(XCResult {
            path: absolute_path.to_string(),
            results_obj,
        })
    }

    fn find_tests<T: AsRef<str>>(&self, id: T) -> anyhow::Result<serde_json::Value> {
        xcresulttool(self.path.as_str(), Some(&["--id", id.as_ref()]))
    }

    fn generate_id(raw_id: &str) -> String {
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, raw_id.as_bytes()).to_string()
    }

    fn junit_testcase(
        &self,
        action: &serde_json::Value,
        testcase: &serde_json::Value,
        testcase_group: &serde_json::Value,
    ) -> anyhow::Result<TestCase> {
        let name = testcase
            .get("name")
            .and_then(|r| r.get(RESULTS_FIELD_VALUE))
            .and_then(|r| r.as_str())
            .map_or_else(
                || {
                    log::debug!("failed to get name of testcase: {:?}", testcase);
                    Err(anyhow::anyhow!("failed to get name of testcase"))
                },
                Ok,
            )?;
        let status = match testcase
            .get("testStatus")
            .and_then(|r| r.get(RESULTS_FIELD_VALUE))
            .and_then(|r| r.as_str())
        {
            Some(val) => val,
            None => {
                log::debug!("failed to get status of testcase: {:?}", testcase);
                return Err(anyhow::anyhow!("failed to get status of testcase"));
            }
        };
        let mut testcase_status = match status {
            "Error" => TestCaseStatus::non_success(NonSuccessKind::Error),
            "Failure" => TestCaseStatus::non_success(NonSuccessKind::Failure),
            "Skipped" => TestCaseStatus::skipped(),
            "Success" => TestCaseStatus::success(),
            _ => TestCaseStatus::non_success(NonSuccessKind::Error),
        };
        let mut uri = String::new();
        if status == "Failure" {
            let mut failures = match action
                .get("actionResult")
                .and_then(|r| r.get("issues"))
                .and_then(|r| r.get("testFailureSummaries"))
                .and_then(|r| r.get(RESULTS_FIELD_VALUES))
                .and_then(|r| r.as_array())
            {
                Some(val) => val.iter(),
                None => {
                    log::debug!("failed to get failures of testcase: {:?}", testcase);
                    return Err(anyhow::anyhow!("failed to get failures of testcase"));
                }
            };
            let testcase_identifier = testcase
                .get("identifier")
                .and_then(|r| r.get(RESULTS_FIELD_VALUE))
                .and_then(|r| r.as_str());
            if let Some(testcase_identifer) = testcase_identifier {
                let testcase_identifer_updated = testcase_identifer.replace('/', ".");
                let testcase_identifer_updated_str = Some(testcase_identifer_updated.as_str());
                let failure = failures.find(|f| {
                    f.get("testCaseName")
                        .and_then(|r| r.get(RESULTS_FIELD_VALUE))
                        .and_then(|r| r.as_str())
                        == testcase_identifer_updated_str
                });
                if let Some(failure) = failure {
                    let failure_message = failure
                        .get("message")
                        .and_then(|r| r.get(RESULTS_FIELD_VALUE))
                        .and_then(|r| r.as_str());
                    let failure_uri = failure
                        .get("documentLocationInCreatingWorkspace")
                        .and_then(|r| r.get("url"))
                        .and_then(|r| r.get(RESULTS_FIELD_VALUE))
                        .and_then(|r| r.as_str());
                    testcase_status.set_message(failure_message.unwrap_or_default());
                    uri = failure_uri.unwrap_or_default().replace("file://", "");
                }
            }
        }
        let mut testcase_junit = TestCase::new(name, testcase_status);
        let id = testcase
            .get("identifierURL")
            .and_then(|r| r.get(RESULTS_FIELD_VALUE))
            .and_then(|r| r.as_str())
            .map(XCResult::generate_id)
            .unwrap_or_default();
        testcase_junit.extra.insert("id".into(), id.into());
        let file_components = uri.split('#').collect::<Vec<&str>>();
        if file_components.len() == 2 {
            testcase_junit
                .extra
                .insert("file".into(), file_components[0].into());
        }
        if let Some(classname) = testcase_group
            .get("name")
            .and_then(|r| r.get(RESULTS_FIELD_VALUE))
            .and_then(|r| r.as_str())
        {
            testcase_junit.set_classname(classname);
        }

        if let Some(duration) = testcase
            .get("duration")
            .and_then(|r| r.get(RESULTS_FIELD_VALUE))
            .and_then(|r| r.as_str())
            .and_then(|r| r.parse::<f32>().ok())
        {
            testcase_junit.set_time(std::time::Duration::from_secs_f32(duration));
        }
        Ok(testcase_junit)
    }

    fn junit_testsuite(
        &self,
        action: &serde_json::Value,
        testsuite: &serde_json::Value,
    ) -> anyhow::Result<TestSuite> {
        let testsuite_name = testsuite
            .get("name")
            .and_then(|r| r.get(RESULTS_FIELD_VALUE))
            .and_then(|r| r.as_str());
        if testsuite_name.is_none() {
            log::debug!("failed to get name of testsuite: {:?}", testsuite);
            return Err(anyhow::anyhow!("failed to get name of testsuite"));
        }
        let mut testsuite_junit = TestSuite::new(testsuite_name.unwrap_or_default());
        if let Some(identifier) = testsuite
            .get("identifierURL")
            .and_then(|r| r.get(RESULTS_FIELD_VALUE))
            .and_then(|r| r.as_str())
        {
            testsuite_junit.extra.append(&mut indexmap! {
                XmlString::new("id") => XmlString::new(XCResult::generate_id(identifier)),
            });
        }
        if let Some(duration) = testsuite
            .get("duration")
            .and_then(|r| r.get(RESULTS_FIELD_VALUE))
            .and_then(|r| r.as_str())
            .and_then(|r| r.parse::<f32>().ok())
        {
            testsuite_junit.set_time(std::time::Duration::from_secs_f32(duration));
        }
        if let Some(testcase_groups) = testsuite
            .get("subtests")
            .and_then(|t| t.get(RESULTS_FIELD_VALUES))
            .and_then(|r| r.as_array())
        {
            for testcase_group in testcase_groups {
                if let Some(testcases) = testcase_group
                    .get("subtests")
                    .and_then(|t| t.get(RESULTS_FIELD_VALUES))
                    .and_then(|r| r.as_array())
                {
                    for testcase in testcases {
                        let testcase_xml = self.junit_testcase(action, testcase, testcase_group)?;
                        testsuite_junit.add_test_case(testcase_xml);
                    }
                }
            }
        };
        Ok(testsuite_junit)
    }

    fn junit_report(&self, action: &serde_json::Value) -> anyhow::Result<Report> {
        let mut testsuites_junit = Report::new("name");
        let raw_id = action
            .get("actionResult")
            .and_then(|r| r.get("testsRef"))
            .and_then(|r| r.get("id"))
            .and_then(|r| r.get(RESULTS_FIELD_VALUE))
            .and_then(|r| r.as_str());
        if raw_id.is_none() {
            log::debug!("no test id found for action: {:?}", action);
            return Ok(testsuites_junit);
        }
        let id = raw_id.unwrap_or_default();
        const DATE_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.f%z";
        if let (Some(ended_time), Some(started_time)) = (
            action
                .get("endedTime")
                .and_then(|r| r.get(RESULTS_FIELD_VALUE))
                .and_then(|r| r.as_str()),
            action
                .get("startedTime")
                .and_then(|r| r.get(RESULTS_FIELD_VALUE))
                .and_then(|r| r.as_str()),
        ) {
            let ended_time_parsed = chrono::DateTime::parse_from_str(ended_time, DATE_FORMAT)?;
            let started_time_parsed = chrono::DateTime::parse_from_str(started_time, DATE_FORMAT)?;
            let duration = (ended_time_parsed.timestamp_millis()
                - started_time_parsed.timestamp_millis()) as u64;
            testsuites_junit.set_time(std::time::Duration::from_millis(duration));
        }
        let found_tests = self.find_tests(id)?;
        let test_summaries = match found_tests
            .get("summaries")
            .and_then(|r| r.get(RESULTS_FIELD_VALUES))
        {
            Some(val) => val.as_array(),
            None => return Ok(testsuites_junit),
        };
        if let Some(test_summaries) = test_summaries {
            for test_summary in test_summaries {
                let testable_summaries = match test_summary
                    .get("testableSummaries")
                    .and_then(|t| t.get(RESULTS_FIELD_VALUES))
                    .and_then(|r| r.as_array())
                {
                    Some(val) => val,
                    None => {
                        return Ok(testsuites_junit);
                    }
                };
                for testable_summary in testable_summaries {
                    let top_level_tests = match testable_summary
                        .get("tests")
                        .and_then(|t| t.get(RESULTS_FIELD_VALUES))
                        .and_then(|r| r.as_array())
                    {
                        Some(val) => val,
                        None => {
                            return Ok(testsuites_junit);
                        }
                    };
                    for top_level_test in top_level_tests {
                        let testsuites = match top_level_test
                            .get("subtests")
                            .and_then(|t| t.get(RESULTS_FIELD_VALUES))
                            .and_then(|r| r.as_array())
                        {
                            Some(val) => val,
                            None => {
                                return Ok(testsuites_junit);
                            }
                        };
                        for testsuite in testsuites {
                            let testsuite_junit = self.junit_testsuite(action, testsuite)?;
                            testsuites_junit.add_test_suite(testsuite_junit);
                        }
                    }
                }
            }
        }
        Ok(testsuites_junit)
    }

    pub fn generate_junits(&self) -> anyhow::Result<Vec<Report>> {
        let mut report_junits: Vec<Report> = Vec::new();
        if let Some(actions) = self
            .results_obj
            .get("actions")
            .and_then(|a| a.get(RESULTS_FIELD_VALUES))
            .and_then(|r| r.as_array())
        {
            for action in actions {
                let report_junit = self.junit_report(action)?;
                // only add the report if it has test suites
                // xcresult stores build actions
                if !report_junit.test_suites.is_empty() {
                    report_junits.push(report_junit);
                }
            }
        }
        Ok(report_junits)
    }
}
