use std::{fs, io::BufReader, time::Duration};

use chrono::{Days, NaiveTime, TimeDelta, Utc};
use context::junit::{
    self,
    bindings::BindingsReport,
    junit_path::{TestRunnerReport, TestRunnerReportStatus},
    parser::JunitParser,
    validator::{
        JunitReportValidation, JunitReportValidationIssue, JunitReportValidationIssueSubOptimal,
        JunitTestCaseValidationIssue, JunitTestCaseValidationIssueInvalid,
        JunitTestCaseValidationIssueSubOptimal, JunitTestSuiteValidationIssue,
        JunitTestSuiteValidationIssueInvalid, JunitTestSuiteValidationIssueSubOptimal,
        JunitValidationIssue, JunitValidationIssueType, JunitValidationLevel,
        TestRunnerReportValidationIssue, TestRunnerReportValidationIssueSubOptimal,
    },
};
use junit_mock::JunitMock;
use quick_junit::Report;
use tempfile::TempDir;

fn new_mock_junit_options(
    report_count: usize,
    test_suite_count: Option<usize>,
    test_case_count: Option<usize>,
    do_not_render_testsuites_element: bool,
) -> junit_mock::Options {
    let mut options = junit_mock::Options::default();

    options.report.do_not_render_testsuites_element = do_not_render_testsuites_element;

    // Tests that run later than 1 hour ago are sub-optimal
    options.global.timestamp = Utc::now()
        .fixed_offset()
        .checked_sub_signed(TimeDelta::hours(1));

    // Make test durations short so we don't have tests mocked into the future
    options.test_case.test_case_duration_range =
        vec![Duration::from_secs(1).into(), Duration::from_secs(2).into()];
    options.test_rerun.test_rerun_duration_range =
        vec![Duration::from_secs(1).into(), Duration::from_secs(2).into()];

    options.report.report_random_count = report_count;

    // NOTE: Large JUnit.xml files make `pretty_assertions::assert_eq` choke when showing diffs
    options.test_suite.test_suite_random_count = test_suite_count.map(|c| c.min(5)).unwrap_or(1);
    options.test_case.test_case_random_count = test_case_count.map(|c| c.min(10)).unwrap_or(10);

    options
}

fn generate_mock_junit_reports(
    report_count: usize,
    test_suite_count: Option<usize>,
    test_case_count: Option<usize>,
) -> (u64, Vec<Report>) {
    let options = new_mock_junit_options(report_count, test_suite_count, test_case_count, false);

    let mut jm = JunitMock::new(options);
    let seed = jm.get_seed();
    let reports = jm.generate_reports();
    (seed, reports)
}

fn serialize_report(report: &Report) -> Vec<u8> {
    let mut serialized_report = Vec::new();
    report.serialize(&mut serialized_report).unwrap();
    serialized_report
}

fn parse_report<T: AsRef<[u8]>>(serialized_report: T) -> BindingsReport {
    let mut junit_parser = JunitParser::new();
    junit_parser
        .parse(BufReader::new(serialized_report.as_ref()))
        .unwrap();

    assert_eq!(junit_parser.issues(), &[]);

    let mut parsed_reports = junit_parser.into_reports();
    assert_eq!(parsed_reports.len(), 1);

    BindingsReport::from(parsed_reports.pop().unwrap())
}

#[test]
fn validate_test_suite_name_too_short() {
    let (seed, mut generated_reports) = generate_mock_junit_reports(1, Some(1), None);
    let mut generated_report = generated_reports.pop().unwrap();

    for test_suite in &mut generated_report.test_suites {
        test_suite.name = String::new().into();
    }

    let bindings_report = BindingsReport::from(generated_report.clone());
    let bindings_validation =
        junit::validator::validate(&bindings_report, &None, Utc::now().fixed_offset());
    let report_validation = JunitReportValidation::from(bindings_validation);

    assert_eq!(
        report_validation.max_level(),
        JunitValidationLevel::Invalid,
        "failed to validate with seed `{}`",
        seed,
    );

    assert_eq!(report_validation.valid_test_suites.len(), 0);

    pretty_assertions::assert_eq!(
        report_validation
            .test_suites()
            .iter()
            .flat_map(|test_suite| Vec::from(test_suite.issues()))
            .collect::<Vec<JunitTestSuiteValidationIssue>>(),
        vec![JunitValidationIssue::Invalid(
            JunitTestSuiteValidationIssueInvalid::TestSuiteNameTooShort(String::new()),
        )],
        "failed to validate with seed `{}`",
        seed,
    );
}

#[test]
fn validate_test_case_name_too_short() {
    let (seed, mut generated_reports) = generate_mock_junit_reports(1, Some(1), Some(1));
    let mut generated_report = generated_reports.pop().unwrap();

    for test_suite in &mut generated_report.test_suites {
        for test_case in &mut test_suite.test_cases {
            test_case.name = String::new().into();
        }
    }

    let bindings_report = BindingsReport::from(generated_report.clone());
    let bindings_validation =
        junit::validator::validate(&bindings_report, &None, Utc::now().fixed_offset());
    let report_validation = JunitReportValidation::from(bindings_validation);

    assert_eq!(
        report_validation.max_level(),
        JunitValidationLevel::Invalid,
        "failed to validate with seed `{}`",
        seed,
    );

    pretty_assertions::assert_eq!(
        report_validation
            .test_suites()
            .iter()
            .flat_map(|test_suite| test_suite.test_cases())
            .flat_map(|test_case| Vec::from(test_case.issues()))
            .collect::<Vec<JunitTestCaseValidationIssue>>(),
        vec![JunitValidationIssue::Invalid(
            JunitTestCaseValidationIssueInvalid::TestCaseNameTooShort(String::new()),
        )],
        "failed to validate with seed `{}`",
        seed,
    );
}

#[test]
fn validate_test_suite_name_too_long() {
    let (seed, mut generated_reports) = generate_mock_junit_reports(1, Some(1), None);
    let mut generated_report = generated_reports.pop().unwrap();

    for test_suite in &mut generated_report.test_suites {
        test_suite.name = "a".repeat(junit::validator::MAX_FIELD_LEN + 1).into();
    }

    let bindings_report = BindingsReport::from(generated_report.clone());
    let bindings_validation =
        junit::validator::validate(&bindings_report, &None, Utc::now().fixed_offset());
    let report_validation = JunitReportValidation::from(bindings_validation);

    assert_eq!(
        report_validation.max_level(),
        JunitValidationLevel::SubOptimal,
        "failed to validate with seed `{}`",
        seed,
    );

    pretty_assertions::assert_eq!(
        report_validation
            .test_suites()
            .iter()
            .flat_map(|test_suite| Vec::from(test_suite.issues()))
            .collect::<Vec<JunitTestSuiteValidationIssue>>(),
        vec![JunitValidationIssue::SubOptimal(
            JunitTestSuiteValidationIssueSubOptimal::TestSuiteNameTooLong(
                "a".repeat(junit::validator::MAX_FIELD_LEN)
            ),
        )],
        "failed to validate with seed `{}`",
        seed,
    );
}

#[test]
fn validate_test_case_name_too_long() {
    let (seed, mut generated_reports) = generate_mock_junit_reports(1, Some(1), Some(1));
    let mut generated_report = generated_reports.pop().unwrap();

    for test_suite in &mut generated_report.test_suites {
        for test_case in &mut test_suite.test_cases {
            test_case.name = "a".repeat(junit::validator::MAX_FIELD_LEN + 1).into();
        }
    }

    let bindings_report = BindingsReport::from(generated_report.clone());
    let bindings_validation =
        junit::validator::validate(&bindings_report, &None, Utc::now().fixed_offset());
    let report_validation = JunitReportValidation::from(bindings_validation);

    assert_eq!(
        report_validation.max_level(),
        JunitValidationLevel::SubOptimal,
        "failed to validate with seed `{}`",
        seed,
    );

    pretty_assertions::assert_eq!(
        report_validation
            .test_suites()
            .iter()
            .flat_map(|test_suite| test_suite.test_cases())
            .flat_map(|test_case| Vec::from(test_case.issues()))
            .collect::<Vec<JunitTestCaseValidationIssue>>(),
        vec![JunitValidationIssue::SubOptimal(
            JunitTestCaseValidationIssueSubOptimal::TestCaseNameTooLong(
                "a".repeat(junit::validator::MAX_FIELD_LEN)
            ),
        )],
        "failed to validate with seed `{}`",
        seed,
    );
}

#[test]
fn validate_max_level() {
    let (seed, mut generated_reports) = generate_mock_junit_reports(1, Some(1), Some(1));
    let mut generated_report = generated_reports.pop().unwrap();

    for test_suite in &mut generated_report.test_suites {
        test_suite.name = "a".repeat(junit::validator::MAX_FIELD_LEN + 1).into();
        for test_case in &mut test_suite.test_cases {
            test_case.name = String::new().into();
        }
    }

    let bindings_report = BindingsReport::from(generated_report.clone());
    let bindings_validation =
        junit::validator::validate(&bindings_report, &None, Utc::now().fixed_offset());
    let report_validation = JunitReportValidation::from(bindings_validation);

    assert_eq!(
        report_validation.max_level(),
        JunitValidationLevel::Invalid,
        "failed to validate with seed `{}`",
        seed,
    );

    pretty_assertions::assert_eq!(
        report_validation
            .test_suites()
            .iter()
            .flat_map(|test_suite| Vec::from(test_suite.issues()))
            .collect::<Vec<JunitTestSuiteValidationIssue>>(),
        vec![JunitValidationIssue::SubOptimal(
            JunitTestSuiteValidationIssueSubOptimal::TestSuiteNameTooLong(
                "a".repeat(junit::validator::MAX_FIELD_LEN)
            ),
        )],
        "failed to validate with seed `{}`",
        seed,
    );

    pretty_assertions::assert_eq!(
        report_validation
            .test_suites()
            .iter()
            .flat_map(|test_suite| test_suite.test_cases())
            .flat_map(|test_case| Vec::from(test_case.issues()))
            .collect::<Vec<JunitTestCaseValidationIssue>>(),
        vec![JunitValidationIssue::Invalid(
            JunitTestCaseValidationIssueInvalid::TestCaseNameTooShort(String::new()),
        )],
        "failed to validate with seed `{}`",
        seed,
    );
}

#[test]
fn validate_timestamps() {
    let (seed, mut generated_reports) = generate_mock_junit_reports(1, Some(1), Some(4));
    let mut generated_report = generated_reports.pop().unwrap();
    let generated_report_timestamp = generated_report.timestamp.unwrap();

    for test_suite in &mut generated_report.test_suites {
        for (index, test_case) in &mut test_suite.test_cases.iter_mut().enumerate() {
            match index {
                0 => {
                    // future timestamp
                    test_case.timestamp = Utc::now()
                        .fixed_offset()
                        .checked_add_signed(TimeDelta::hours(1));
                }
                1 => {
                    // stale timestamp
                    test_case.timestamp =
                        generated_report_timestamp.checked_sub_signed(TimeDelta::hours(1))
                }
                2 => {
                    // old timestamp
                    test_case.timestamp =
                        generated_report_timestamp.checked_sub_signed(TimeDelta::hours(24))
                }
                _ => {
                    // valid timestamp
                }
            };
        }
    }

    let bindings_report = BindingsReport::from(generated_report.clone());
    let bindings_validation =
        junit::validator::validate(&bindings_report, &None, Utc::now().fixed_offset());
    let report_validation = JunitReportValidation::from(bindings_validation);

    assert_eq!(
        report_validation.max_level(),
        JunitValidationLevel::SubOptimal,
        "failed to validate with seed `{}`",
        seed,
    );

    pretty_assertions::assert_eq!(
        report_validation.all_issues(),
        vec![
            JunitValidationIssueType::Report(JunitReportValidationIssue::SubOptimal(
                JunitReportValidationIssueSubOptimal::OldTimestamps,
            )),
            JunitValidationIssueType::Report(JunitReportValidationIssue::SubOptimal(
                JunitReportValidationIssueSubOptimal::StaleTimestamps,
            )),
            JunitValidationIssueType::Report(JunitReportValidationIssue::SubOptimal(
                JunitReportValidationIssueSubOptimal::FutureTimestamps,
            )),
        ],
        "failed to validate with seed `{}`",
        seed,
    );
}

#[test]
fn validate_test_runner_report_overrides_timestamp() {
    let mut options = new_mock_junit_options(1, Some(1), Some(1), true);
    let old_timestamp = Utc::now().checked_sub_days(Days::new(1)).unwrap();
    options.global.timestamp = Some(old_timestamp.fixed_offset());
    let mut jm = JunitMock::new(options);
    let seed = jm.get_seed();
    let mut generated_reports = jm.generate_reports();

    let generated_report = generated_reports.pop().unwrap();

    {
        let start_time = Utc::now().checked_add_signed(TimeDelta::hours(1)).unwrap();
        let override_report = TestRunnerReport {
            status: TestRunnerReportStatus::Passed,
            start_time,
            end_time: start_time
                .checked_add_signed(TimeDelta::minutes(1))
                .unwrap(),
            label: None,
        };
        let override_report_validation = junit::validator::validate(
            &BindingsReport::from(generated_report.clone()),
            &Some(override_report.clone()),
            Utc::now().fixed_offset(),
        );
        pretty_assertions::assert_eq!(
            JunitReportValidation::from(override_report_validation).all_issues(),
            &[
                JunitValidationIssueType::Report(JunitReportValidationIssue::SubOptimal(
                    JunitReportValidationIssueSubOptimal::FutureTimestamps
                )),
                JunitValidationIssueType::TestRunnerReport(
                    TestRunnerReportValidationIssue::SubOptimal(
                        TestRunnerReportValidationIssueSubOptimal::EndTimeFutureTimestamp(
                            override_report.end_time.fixed_offset()
                        )
                    )
                ),
                JunitValidationIssueType::TestRunnerReport(
                    TestRunnerReportValidationIssue::SubOptimal(
                        TestRunnerReportValidationIssueSubOptimal::StartTimeFutureTimestamp(
                            override_report.start_time.fixed_offset()
                        )
                    )
                ),
            ],
            "failed to validate with seed `{}`",
            seed,
        );
    }

    {
        let start_time = Utc::now().checked_sub_days(Days::new(2)).unwrap();
        let override_report = TestRunnerReport {
            status: TestRunnerReportStatus::Passed,
            start_time,
            end_time: start_time
                .checked_add_signed(TimeDelta::minutes(1))
                .unwrap(),
            label: None,
        };
        let bindings_report = BindingsReport::from(generated_report.clone());
        let bindings_validation = junit::validator::validate(
            &bindings_report,
            &Some(override_report.clone()),
            Utc::now().fixed_offset(),
        );
        let override_report_validation = JunitReportValidation::from(bindings_validation);
        pretty_assertions::assert_eq!(
            override_report_validation.all_issues(),
            &[
                JunitValidationIssueType::Report(JunitReportValidationIssue::SubOptimal(
                    JunitReportValidationIssueSubOptimal::OldTimestamps
                )),
                JunitValidationIssueType::TestRunnerReport(
                    TestRunnerReportValidationIssue::SubOptimal(
                        TestRunnerReportValidationIssueSubOptimal::EndTimeOldTimestamp(
                            override_report.end_time.fixed_offset()
                        )
                    )
                ),
                JunitValidationIssueType::TestRunnerReport(
                    TestRunnerReportValidationIssue::SubOptimal(
                        TestRunnerReportValidationIssueSubOptimal::StartTimeOldTimestamp(
                            override_report.start_time.fixed_offset()
                        )
                    )
                ),
            ],
            "failed to validate with seed `{}`",
            seed,
        );
    }

    {
        let start_time = Utc::now().checked_sub_signed(TimeDelta::hours(3)).unwrap();
        let override_report = TestRunnerReport {
            status: TestRunnerReportStatus::Passed,
            start_time,
            end_time: start_time
                .checked_add_signed(TimeDelta::minutes(1))
                .unwrap(),
            label: None,
        };
        let bindings_report = BindingsReport::from(generated_report.clone());
        let bindings_validation = junit::validator::validate(
            &bindings_report,
            &Some(override_report.clone()),
            Utc::now().fixed_offset(),
        );
        let override_report_validation = JunitReportValidation::from(bindings_validation);
        pretty_assertions::assert_eq!(
            override_report_validation.all_issues(),
            &[
                JunitValidationIssueType::Report(JunitReportValidationIssue::SubOptimal(
                    JunitReportValidationIssueSubOptimal::StaleTimestamps
                )),
                JunitValidationIssueType::TestRunnerReport(
                    TestRunnerReportValidationIssue::SubOptimal(
                        TestRunnerReportValidationIssueSubOptimal::EndTimeStaleTimestamp(
                            override_report.end_time.fixed_offset()
                        )
                    )
                ),
                JunitValidationIssueType::TestRunnerReport(
                    TestRunnerReportValidationIssue::SubOptimal(
                        TestRunnerReportValidationIssueSubOptimal::StartTimeStaleTimestamp(
                            override_report.start_time.fixed_offset()
                        )
                    )
                ),
            ],
            "failed to validate with seed `{}`",
            seed,
        );
    }

    {
        let start_time = Utc::now()
            .checked_sub_signed(TimeDelta::minutes(3))
            .unwrap();
        let override_report = TestRunnerReport {
            status: TestRunnerReportStatus::Passed,
            start_time,
            end_time: start_time
                .checked_sub_signed(TimeDelta::minutes(1))
                .unwrap(),
            label: None,
        };
        let bindings_report = BindingsReport::from(generated_report.clone());
        let bindings_validation = junit::validator::validate(
            &bindings_report,
            &Some(override_report.clone()),
            Utc::now().fixed_offset(),
        );
        let report_validation = JunitReportValidation::from(bindings_validation);
        pretty_assertions::assert_eq!(
            report_validation.test_runner_report.issues(),
            &[TestRunnerReportValidationIssue::SubOptimal(
                TestRunnerReportValidationIssueSubOptimal::EndTimeBeforeStartTime(
                    override_report.clone()
                )
            ),],
            "failed to validate with seed `{}`",
            seed,
        );
    }

    {
        let start_time = Utc::now()
            .checked_sub_signed(TimeDelta::minutes(3))
            .unwrap();
        let override_report = TestRunnerReport {
            status: TestRunnerReportStatus::Passed,
            start_time,
            end_time: start_time
                .checked_add_signed(TimeDelta::minutes(1))
                .unwrap(),
            label: None,
        };
        let bindings_report = BindingsReport::from(generated_report.clone());
        let bindings_validation = junit::validator::validate(
            &bindings_report,
            &Some(override_report.clone()),
            Utc::now().fixed_offset(),
        );
        let report_validation = JunitReportValidation::from(bindings_validation);
        pretty_assertions::assert_eq!(
            report_validation.all_issues(),
            &[],
            "failed to validate with seed `{}`",
            seed,
        );
    }

    {
        let bindings_report = BindingsReport::from(generated_report.clone());
        let bindings_validation =
            junit::validator::validate(&bindings_report, &None, Utc::now().fixed_offset());
        let report_validation = JunitReportValidation::from(bindings_validation);
        pretty_assertions::assert_eq!(
            report_validation.all_issues(),
            &[JunitValidationIssueType::Report(
                JunitReportValidationIssue::SubOptimal(
                    JunitReportValidationIssueSubOptimal::StaleTimestamps
                )
            ),],
            "failed to validate with seed `{}`",
            seed,
        );
    }

    {
        let start_time = Utc::now()
            .checked_sub_signed(TimeDelta::minutes(5))
            .unwrap();
        let end_time = start_time
            .checked_add_signed(TimeDelta::minutes(1))
            .unwrap();
        let override_report = TestRunnerReport {
            status: TestRunnerReportStatus::Passed,
            start_time,
            end_time,
            label: None,
        };
        let test_case_timestamp = end_time
            .checked_add_signed(TimeDelta::minutes(1))
            .unwrap()
            .fixed_offset();
        let mut generated_report = generated_report.clone();
        generated_report
            .test_suites
            .iter_mut()
            .for_each(|test_suite| {
                test_suite.test_cases.iter_mut().for_each(|test_case| {
                    test_case.timestamp = Some(test_case_timestamp);
                });
            });
        let bindings_report = BindingsReport::from(generated_report.clone());
        let bindings_validation = junit::validator::validate(
            &bindings_report,
            &Some(override_report.clone()),
            Utc::now().fixed_offset(),
        );
        let override_report_validation = JunitReportValidation::from(bindings_validation);
        let all_issues = override_report_validation.all_issues();

        let expected_timestamp = test_case_timestamp
            + (start_time
                .signed_duration_since(generated_report.timestamp.unwrap().fixed_offset()));

        let actual_timestamp = all_issues
            .iter()
            .find_map(|issue| {
                if let JunitValidationIssueType::TestCase(
                    JunitTestCaseValidationIssue::SubOptimal(
                        JunitTestCaseValidationIssueSubOptimal::TestCaseTimestampIsAfterTestReportEndTime(timestamp),
                    ),
                ) = issue
                {
                    Some(*timestamp)
                } else {
                    None
                }
            })
            .expect("Expected TestCaseTimestampIsAfterTestReportEndTime issue");

        // Allow up to 10ms difference due to microsecond precision conversions
        let timestamp_diff = (actual_timestamp - expected_timestamp)
            .num_milliseconds()
            .abs();
        assert!(
            timestamp_diff <= 10,
            "Timestamp mismatch: expected {expected_timestamp:?}, got {actual_timestamp:?}, diff: {timestamp_diff}ms (seed: {seed})"
        );

        pretty_assertions::assert_eq!(
            all_issues,
            &[
                JunitValidationIssueType::Report(JunitReportValidationIssue::SubOptimal(JunitReportValidationIssueSubOptimal::FutureTimestamps)),
                JunitValidationIssueType::TestCase(JunitTestCaseValidationIssue::SubOptimal(JunitTestCaseValidationIssueSubOptimal::TestCaseTimestampIsAfterTestReportEndTime(actual_timestamp))),
            ],
            "failed to validate with seed `{}`",
            seed,
        );
    }
}

#[test]
fn parse_naive_date() {
    let (seed, mut generated_reports) = generate_mock_junit_reports(1, Some(0), Some(0));

    let mut generated_report = generated_reports.pop().unwrap();
    generated_report.timestamp = None;

    let naive_date = Utc::now()
        .fixed_offset()
        .with_time(NaiveTime::default())
        .unwrap();
    let serialized_generated_report =
        String::from_utf8_lossy(&serialize_report(&mut generated_report)).replace(
            "<testsuites",
            &format!(
                r#"<testsuites timestamp="{}""#,
                naive_date.format("%Y-%m-%d")
            ),
        );
    let first_parsed_report = parse_report(serialized_generated_report.as_bytes());

    let expected_timestamp_micros = naive_date.timestamp_micros();
    pretty_assertions::assert_eq!(
        first_parsed_report.timestamp_micros,
        Some(expected_timestamp_micros),
        "failed to validate with seed `{}`",
        seed,
    );
}

#[test]
fn parse_round_trip_and_validate_fuzzed() {
    const COUNT: usize = 100;
    let (seed, generated_reports) = generate_mock_junit_reports(COUNT, None, None);
    for (index, generated_report) in generated_reports.iter().enumerate() {
        let serialized_generated_report = serialize_report(generated_report);
        let first_parsed_report = parse_report(&serialized_generated_report);
        let bindings_validation =
            junit::validator::validate(&first_parsed_report, &None, Utc::now().fixed_offset());
        let report_validation = JunitReportValidation::from(bindings_validation);

        assert_eq!(
            report_validation.max_level(),
            JunitValidationLevel::Valid,
            "{} of {} failed to validate with seed `{}`",
            index,
            COUNT,
            seed,
        );

        let report_for_serialization: Report = first_parsed_report.clone().into();
        let mut serialized_first_parsed_report = Vec::new();
        report_for_serialization
            .serialize(&mut serialized_first_parsed_report)
            .unwrap();

        pretty_assertions::assert_eq!(
            String::from_utf8_lossy(&serialized_first_parsed_report),
            String::from_utf8_lossy(&serialized_generated_report),
            "{} of {} failed to round-trip with seed `{}`",
            index,
            COUNT,
            seed,
        );
    }
}

#[test]
fn parse_without_testsuites_element() {
    let options = new_mock_junit_options(1, Some(1), Some(1), true);
    let mut jm = JunitMock::new(options);
    let reports = jm.generate_reports();

    let tempdir = TempDir::new().unwrap();
    let xml_path = jm
        .write_reports_to_file(&tempdir, reports.clone())
        .unwrap()
        .pop()
        .unwrap();
    let xml = BufReader::new(fs::File::open(xml_path).unwrap());
    let mut junit_parser = JunitParser::new();
    junit_parser.parse(xml).unwrap();

    let reports_with_default_testsuites: Vec<String> = reports
        .into_iter()
        .map(|r| {
            let mut default_testsuites = Report::new("");
            default_testsuites.add_test_suites(r.test_suites);
            default_testsuites.to_string().unwrap()
        })
        .collect();

    let parsed_reports: Vec<String> = junit_parser
        .into_reports()
        .into_iter()
        .map(|r| r.to_string().unwrap())
        .collect();

    pretty_assertions::assert_eq!(reports_with_default_testsuites, parsed_reports)
}
