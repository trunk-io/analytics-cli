use serde_json::Value;
use std::str;
use std::{fs, process::Command};
use xml_builder::{XMLBuilder, XMLElement, XMLVersion};

#[derive(Debug, Clone)]
pub struct XCResultFile {
    pub path: String,
    results_obj: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Status {
    Success,
    Failure,
    Error,
    Skipped,
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
    ) -> (Status, XMLElement) {
        let mut testcase_xml = XMLElement::new("testcase");
        testcase_xml.add_attribute(
            "name",
            testcase
                .get("name")
                .and_then(|r| r.get("_value"))
                .unwrap()
                .as_str()
                .unwrap(),
        );
        testcase_xml.add_attribute(
            "id",
            self.generate_id(
                testcase
                    .get("identifierURL")
                    .and_then(|r| r.get("_value"))
                    .unwrap()
                    .to_string(),
            )
            .as_str(),
        );

        testcase_xml.add_attribute(
            "classname",
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
            .unwrap();
        testcase_xml.add_attribute("time", duration);
        let status = match testcase
            .get("testStatus")
            .and_then(|r| r.get("_value"))
            .unwrap()
            .as_str()
            .unwrap()
        {
            "Error" => Status::Error,
            "Failure" => Status::Failure,
            "Skipped" => Status::Skipped,
            _ => Status::Success,
        };
        if status == Status::Skipped {
            let skipped_xml = XMLElement::new("skipped");
            match testcase_xml.add_child(skipped_xml) {
                Ok(_) => {}
                Err(e) => {
                    log::debug!("failed to add failure to testcase: {:?}", e);
                }
            }
        }
        if status == Status::Failure {
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
            let mut failure_xml = XMLElement::new("failure");
            failure_xml
                .add_text(String::from(
                    failure
                        .unwrap()
                        .get("message")
                        .and_then(|r| r.get("_value"))
                        .unwrap()
                        .as_str()
                        .unwrap(),
                ))
                .unwrap();
            let raw_uri = failure
                .unwrap()
                .get("documentLocationInCreatingWorkspace")
                .and_then(|r| r.get("url"))
                .unwrap()
                .get("_value")
                .unwrap()
                .as_str()
                .unwrap()
                .replace("file://", "");
            let uri = raw_uri.split('#').collect::<Vec<&str>>()[0];

            testcase_xml.add_attribute("file", uri);
            match testcase_xml.add_child(failure_xml) {
                Ok(_) => {}
                Err(e) => {
                    log::debug!("failed to add failure to testcase: {:?}", e);
                }
            }
        }
        (status, testcase_xml)
    }

    fn junit_testsuite(
        &self,
        action: &serde_json::Value,
        testsuite: &serde_json::Value,
    ) -> XMLElement {
        let mut testsuite_xml = XMLElement::new("testsuite");
        testsuite_xml.add_attribute(
            "id",
            self.generate_id(
                testsuite
                    .get("identifierURL")
                    .and_then(|r| r.get("_value"))
                    .unwrap()
                    .to_string(),
            )
            .as_str(),
        );
        testsuite_xml.add_attribute(
            "name",
            testsuite
                .get("name")
                .and_then(|r| r.get("_value"))
                .unwrap()
                .as_str()
                .unwrap(),
        );
        let duration = testsuite
            .get("duration")
            .and_then(|r| r.get("_value"))
            .unwrap()
            .as_str()
            .unwrap();
        testsuite_xml.add_attribute("time", duration);
        let testcase_groups = testsuite
            .get("subtests")
            .and_then(|t| t.get("_values"))
            .unwrap()
            .as_array()
            .unwrap();
        let mut failure_count = 0;
        let mut total_count = 0;
        let mut error_count = 0;
        let mut skipped_count = 0;
        for testcase_group in testcase_groups {
            let testcases = testcase_group
                .get("subtests")
                .and_then(|t| t.get("_values"))
                .unwrap()
                .as_array()
                .unwrap();
            for testcase in testcases {
                total_count += 1;
                let (status, testcase_xml) = self.junit_testcase(action, testcase, testcase_group);
                match status {
                    Status::Skipped => {
                        skipped_count += 1;
                    }
                    Status::Failure => {
                        failure_count += 1;
                    }
                    Status::Error => {
                        error_count += 1;
                    }
                    _ => {}
                }
                match testsuite_xml.add_child(testcase_xml) {
                    Ok(_) => {}
                    Err(e) => {
                        log::debug!("failed to add testcase to testsuite: {:?}", e);
                    }
                }
            }
        }
        testsuite_xml.add_attribute("failures", failure_count.to_string().as_str());
        testsuite_xml.add_attribute("skipped", skipped_count.to_string().as_str());
        testsuite_xml.add_attribute("errors", error_count.to_string().as_str());
        testsuite_xml.add_attribute("tests", total_count.to_string().as_str());
        testsuite_xml
    }

    fn junit_testsuites(&self, action: &serde_json::Value) -> XMLElement {
        let mut testsuites_xml = XMLElement::new("testsuites");
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
        testsuites_xml.add_attribute("time", duration.to_string().as_str());
        let tests = match action
            .get("actionResult")
            .and_then(|r| r.get("metrics"))
            .and_then(|r| r.get("testsCount"))
            .and_then(|r| r.get("_value"))
        {
            Some(val) => val.as_str().unwrap(),
            None => return testsuites_xml,
        };
        testsuites_xml.add_attribute("tests", tests);
        testsuites_xml.add_attribute(
            "failures",
            action
                .get("actionResult")
                .and_then(|r| r.get("metrics"))
                .and_then(|r| r.get("testsFailedCount"))
                .and_then(|r| r.get("_value"))
                .unwrap_or(&Value::from("0"))
                .as_str()
                .unwrap(),
        );
        testsuites_xml.add_attribute(
            "skipped",
            action
                .get("actionResult")
                .and_then(|r| r.get("metrics"))
                .and_then(|r| r.get("testsSkippedCount"))
                .and_then(|r| r.get("_value"))
                .unwrap_or(&Value::from("0"))
                .as_str()
                .unwrap(),
        );
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
            None => return testsuites_xml,
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
                        let testsuite_xml = self.junit_testsuite(action, testsuite);
                        match testsuites_xml.add_child(testsuite_xml) {
                            Ok(_) => {}
                            Err(e) => {
                                log::debug!("failed to add testsuite to testsuites: {:?}", e);
                            }
                        }
                    }
                }
            }
        }
        testsuites_xml
    }

    pub fn junit(&self) -> Vec<u8> {
        let mut xml = XMLBuilder::new()
            .version(XMLVersion::XML1_0)
            .encoding("UTF-8".into())
            .build();

        if let Some(actions) = self
            .results_obj
            .get("actions")
            .and_then(|a| a.get("_values"))
        {
            for action in actions.as_array().unwrap() {
                let testsuites_xml = self.junit_testsuites(action);
                xml.set_root_element(testsuites_xml);
            }
        }

        let mut writer: Vec<u8> = Vec::new();
        xml.generate(&mut writer).unwrap();
        log::info!("junit xml: {}", str::from_utf8(&writer).unwrap());
        writer
    }
}
