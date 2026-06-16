use std::path::Path;

use api::{client::get_api_host, urls::url_for_test_case};
use bundle::Test;
use context::{meta::id::gen_info_id, repo::RepoUrlParts};
use proto::test_context::test_run::{
    TestCaseRun, TestCaseRunStatus, TestReport,
    test_case_run::TestRunnerInformation,
};
use serde::{Deserialize, Serialize};

use crate::upload_command::UploadRunResult;

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SummaryReport {
    pub schema_version: u32,
    pub summary: SummaryCounts,
    pub failures: Vec<SummaryFailure>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SummaryCounts {
    pub total: usize,
    pub pass: usize,
    pub fail: usize,
    pub quarantined: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pass_ratio: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SummaryFailure {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_number: Option<i32>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
    pub suite_name: String,
    pub trunk_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bazel_target: Option<String>,
    pub is_quarantined: bool,
}

pub fn build_summary_report(
    result: &UploadRunResult,
    test_report_override: Option<&TestReport>,
) -> anyhow::Result<SummaryReport> {
    let org_url_slug = &result.quarantine_context.org_url_slug;
    let repo = &result.quarantine_context.repo;
    let variant = result.meta.variant.clone().unwrap_or_default();

    let mut failures = match test_report_override.or(result.test_report.as_ref()) {
        Some(test_report) => {
            failures_from_test_report(test_report, org_url_slug, repo, &variant)?
        }
        None => {
            tracing::warn!("No test report available for summary output; writing empty failures list");
            Vec::new()
        }
    };
    failures.sort_by_key(|failure| failure.is_quarantined);

    let (failure_count, quarantined) = failures.iter().fold((0, 0), |(fail, quar), failure| {
        if failure.is_quarantined {
            (fail, quar + 1)
        } else {
            (fail + 1, quar)
        }
    });
    let total_tests = result.meta.junit_props.num_tests;
    let passes = total_tests.saturating_sub(failure_count + quarantined);

    Ok(SummaryReport {
        schema_version: SCHEMA_VERSION,
        summary: SummaryCounts {
            total: total_tests,
            pass: passes,
            fail: failure_count,
            quarantined,
            pass_ratio: pass_ratio(total_tests, passes),
        },
        failures,
    })
}

fn failures_from_test_report(
    test_report: &TestReport,
    org_url_slug: &String,
    repo: &RepoUrlParts,
    variant: &str,
) -> anyhow::Result<Vec<SummaryFailure>> {
    test_report
        .test_results
        .iter()
        .flat_map(|test_result| test_result.test_case_runs.iter())
        .filter(|case_run| is_failure_run(case_run))
        .map(|case_run| summary_failure_from_case_run(case_run, org_url_slug, repo, variant))
        .collect()
}

fn pass_ratio(total_tests: usize, passes: usize) -> Option<f64> {
    if total_tests == 0 {
        None
    } else {
        Some(((passes as f64 / total_tests as f64) * 100.0).round() / 100.0)
    }
}

fn is_failure_run(case_run: &TestCaseRun) -> bool {
    TestCaseRunStatus::try_from(case_run.status).ok() == Some(TestCaseRunStatus::Failure)
}

fn summary_failure_from_case_run(
    case_run: &TestCaseRun,
    org_url_slug: &String,
    repo: &RepoUrlParts,
    variant: &str,
) -> anyhow::Result<SummaryFailure> {
    let test = test_from_case_run(case_run, org_url_slug, repo, variant);

    Ok(SummaryFailure {
        file: non_empty_string(&case_run.file),
        line_number: line_number_from_run(case_run),
        name: case_run.name.clone(),
        class_name: non_empty_string(&case_run.classname),
        suite_name: case_run.parent_name.clone(),
        trunk_url: url_for_test_case(&get_api_host(), org_url_slug, repo, &test)?,
        duration: duration_from_run(case_run),
        failure_message: failure_message_from_run(case_run),
        bazel_target: bazel_target_from_run(case_run),
        is_quarantined: case_run.is_quarantined,
    })
}

fn test_from_case_run(
    case_run: &TestCaseRun,
    org_slug: &String,
    repo: &RepoUrlParts,
    variant: &str,
) -> Test {
    Test {
        name: case_run.name.clone(),
        parent_name: case_run.parent_name.clone(),
        class_name: non_empty_string(&case_run.classname),
        file: non_empty_string(&case_run.file),
        id: gen_info_id(
            org_slug.as_str(),
            repo.repo_full_name().as_str(),
            Some(case_run.file.as_str()),
            (!case_run.classname.is_empty()).then_some(case_run.classname.as_str()),
            Some(case_run.parent_name.as_str()),
            Some(case_run.name.as_str()),
            (!case_run.id.is_empty()).then_some(case_run.id.as_str()),
            variant,
        ),
        timestamp_millis: None,
        is_quarantined: case_run.is_quarantined,
        failure_message: failure_message_from_run(case_run),
        variant: Some(variant.to_string()),
    }
}

fn non_empty_string(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn line_number_from_run(case_run: &TestCaseRun) -> Option<i32> {
    case_run
        .line_number
        .as_ref()
        .map(|line| line.number)
        .or_else(|| (case_run.line != 0).then_some(case_run.line))
}

fn duration_from_run(case_run: &TestCaseRun) -> Option<f64> {
    let started_at = case_run.started_at.as_ref()?;
    let finished_at = case_run.finished_at.as_ref()?;
    let started_at = chrono::DateTime::from(started_at.clone());
    let finished_at = chrono::DateTime::from(finished_at.clone());
    Some((finished_at - started_at).num_milliseconds() as f64 / 1000.0)
}

fn failure_message_from_run(case_run: &TestCaseRun) -> Option<String> {
    case_run
        .test_output
        .as_ref()
        .and_then(|output| {
            non_empty_string(&output.text).or_else(|| non_empty_string(&output.message))
        })
        // trunk-ignore(clippy/deprecated)
        .or_else(|| non_empty_string(&case_run.status_output_message))
}

fn bazel_target_from_run(case_run: &TestCaseRun) -> Option<String> {
    match &case_run.test_runner_information {
        Some(TestRunnerInformation::BazelRunInformation(info)) => {
            non_empty_string(&info.label)
        }
        _ => None,
    }
}

pub fn write_summary_output_file(
    path: &Path,
    result: &UploadRunResult,
    test_report_override: Option<&TestReport>,
) -> anyhow::Result<()> {
    let report = build_summary_report(result, test_report_override)?;
    let json = serde_json::to_string_pretty(&report)?;
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    if let Some(parent) = parent {
        std::fs::create_dir_all(parent)?;
    }
    let temp_path = path.with_extension("tmp");
    std::fs::write(&temp_path, json)?;
    std::fs::rename(&temp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use bundle::{
        BundleMetaBaseProps, BundleMetaDebugProps, BundleMetaJunitProps, BundleMetaV0_7_8,
    };
    use context::repo::{BundleRepo, RepoUrlParts};
    use proto::test_context::test_run::{TestOutput, TestResult};

    use super::*;
    use crate::context_quarantine::QuarantineContext;

    fn sample_case_run(name: &str, parent: &str, is_quarantined: bool) -> TestCaseRun {
        TestCaseRun {
            name: name.to_string(),
            parent_name: parent.to_string(),
            classname: "MyClass".to_string(),
            file: "src/foo_test.rs".to_string(),
            status: TestCaseRunStatus::Failure.into(),
            is_quarantined,
            test_output: Some(TestOutput {
                message: "assertion failed".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn expected_summary_failure_from_case_run(
        case_run: &TestCaseRun,
        org_url_slug: &str,
        repo: &RepoUrlParts,
        variant: &str,
    ) -> SummaryFailure {
        let org_url_slug_string = org_url_slug.to_string();
        let test = test_from_case_run(case_run, &org_url_slug_string, repo, variant);
        SummaryFailure {
            file: non_empty_string(&case_run.file),
            line_number: line_number_from_run(case_run),
            name: case_run.name.clone(),
            class_name: non_empty_string(&case_run.classname),
            suite_name: case_run.parent_name.clone(),
            trunk_url: url_for_test_case(&get_api_host(), org_url_slug, repo, &test).unwrap(),
            duration: duration_from_run(case_run),
            failure_message: failure_message_from_run(case_run),
            bazel_target: bazel_target_from_run(case_run),
            is_quarantined: case_run.is_quarantined,
        }
    }

    fn sample_upload_result(test_report: TestReport, repo: RepoUrlParts) -> UploadRunResult {
        let meta = BundleMetaV0_7_8 {
            base_props: BundleMetaBaseProps {
                version: String::new(),
                cli_version: String::new(),
                org: "test-org".to_string(),
                test_collection: None,
                repo: BundleRepo::default(),
                bundle_upload_id: "upload-id".to_string(),
                tags: vec![],
                file_sets: vec![],
                envs: std::collections::HashMap::new(),
                upload_time_epoch: 0,
                test_command: None,
                os_info: None,
                quarantined_tests: vec![],
                codeowners: None,
                use_uncloned_repo: None,
            },
            junit_props: BundleMetaJunitProps {
                num_tests: 10,
                ..Default::default()
            },
            debug_props: BundleMetaDebugProps {
                command_line: String::new(),
                trunk_envs: std::collections::HashMap::new(),
            },
            bundle_upload_id_v2: "upload-id-v2".to_string(),
            variant: None,
            internal_bundled_file: None,
            failed_tests: vec![],
        };

        let mut qc = QuarantineContext::skip_fetch(vec![]);
        qc.org_url_slug = "test-org".to_string();
        qc.repo = repo;

        UploadRunResult {
            error_report: None,
            quarantine_context: qc,
            meta,
            test_report: Some(test_report),
            validations: crate::validate_command::JunitReportValidations::new(
                std::collections::BTreeMap::new(),
            ),
            validation_report: crate::report_limiting::ValidationReport::Limited,
            show_failure_messages: false,
            summary_output_file: None,
            summary_output_written: false,
        }
    }

    #[test]
    fn summary_counts_match_terminal_math() {
        let repo = RepoUrlParts {
            host: "github.com".to_string(),
            owner: "trunk-io".to_string(),
            name: "analytics-cli".to_string(),
        };
        let fails = sample_case_run("fails", "suite", false);
        let quarantined = sample_case_run("quarantined", "suite", true);
        let test_report = TestReport {
            test_results: vec![TestResult {
                test_case_runs: vec![fails.clone(), quarantined.clone()],
                ..Default::default()
            }],
            uploader_metadata: None,
        };

        let result = sample_upload_result(test_report, repo.clone());
        let report = build_summary_report(&result, None).unwrap();

        assert!(fails.id.is_empty());
        assert!(quarantined.id.is_empty());
        assert_eq!(
            report,
            SummaryReport {
                schema_version: 1,
                summary: SummaryCounts {
                    total: 10,
                    pass: 8,
                    fail: 1,
                    quarantined: 1,
                    pass_ratio: Some(0.8),
                },
                failures: vec![
                    expected_summary_failure_from_case_run(&fails, "test-org", &repo, ""),
                    expected_summary_failure_from_case_run(&quarantined, "test-org", &repo, ""),
                ],
            }
        );
    }

    #[test]
    fn summary_url_uses_uuid_for_trunk_prefixed_junit_id() {
        use context::meta::id::gen_info_id;

        let repo = RepoUrlParts {
            host: "github.com".to_string(),
            owner: "trunk-io".to_string(),
            name: "analytics-cli".to_string(),
        };
        let mut case_run = sample_case_run("fails", "suite", false);
        case_run.id = "trunk:custom-id".to_string();

        let test_report = TestReport {
            test_results: vec![TestResult {
                test_case_runs: vec![case_run.clone()],
                ..Default::default()
            }],
            uploader_metadata: None,
        };

        let result = sample_upload_result(test_report, repo.clone());
        let report = build_summary_report(&result, None).unwrap();

        let expected_uuid = gen_info_id(
            "test-org",
            repo.repo_full_name().as_str(),
            Some("src/foo_test.rs"),
            Some("MyClass"),
            Some("suite"),
            Some("fails"),
            Some("trunk:custom-id"),
            "",
        );

        assert_eq!(report.failures.len(), 1);
        assert!(
            report.failures[0].trunk_url.contains(&expected_uuid),
            "expected uuid {} in url, got {}",
            expected_uuid,
            report.failures[0].trunk_url
        );
        assert!(
            !report.failures[0].trunk_url.contains("trunk:custom-id"),
            "url should not contain raw junit id: {}",
            report.failures[0].trunk_url
        );
        assert_eq!(
            report.failures[0],
            expected_summary_failure_from_case_run(&case_run, "test-org", &repo, "")
        );
    }
}
