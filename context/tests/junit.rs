use std::io::BufReader;

use chrono::{NaiveTime, TimeDelta, Utc};
use junit_mock::JunitMock;
use quick_junit::Report;

use context::junit::{
    self,
    parser::JunitParser,
    validator::{
        JunitTestCaseValidationIssue, JunitTestCaseValidationIssueInvalid,
        JunitTestCaseValidationIssueSubOptimal, JunitTestSuiteValidationIssue,
        JunitTestSuiteValidationIssueInvalid, JunitTestSuiteValidationIssueSubOptimal,
        JunitValidationIssue, JunitValidationLevel,
    },
};

fn generate_mock_junit_reports(
    report_count: usize,
    test_suite_count: Option<usize>,
    test_case_count: Option<usize>,
) -> (u64, Vec<Report>) {
    let mut options = junit_mock::Options::default();
    options.global.timestamp = Utc::now()
        .fixed_offset()
        .checked_sub_signed(TimeDelta::hours(1));
    options.report.report_random_count = report_count;
    // NOTE: Large JUnit.xml files make `pretty_assertions::assert_eq` choke when showing diffs
    options.test_suite.test_suite_random_count = test_suite_count.map(|c| c.min(5)).unwrap_or(1);
    options.test_case.test_case_random_count = test_case_count.map(|c| c.min(10)).unwrap_or(10);

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

fn parse_report<T: AsRef<[u8]>>(serialized_report: T) -> Report {
    let mut junit_parser = JunitParser::new();
    junit_parser
        .parse(BufReader::new(&serialized_report.as_ref()[..]))
        .unwrap();

    assert_eq!(junit_parser.errors(), &[]);

    let mut parsed_reports = junit_parser.into_reports();
    assert_eq!(parsed_reports.len(), 1);

    parsed_reports.pop().unwrap()
}

#[test]
fn validate_test_suite_name_too_short() {
    let (seed, mut generated_reports) = generate_mock_junit_reports(1, Some(1), None);
    let mut generated_report = generated_reports.pop().unwrap();

    for test_suite in &mut generated_report.test_suites {
        test_suite.name = String::new().into();
    }

    let report_validation = junit::validator::validate(&generated_report);

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

    let report_validation = junit::validator::validate(&generated_report);

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

    let report_validation = junit::validator::validate(&generated_report);

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

    let report_validation = junit::validator::validate(&generated_report);

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

    let report_validation = junit::validator::validate(&generated_report);

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

    pretty_assertions::assert_eq!(
        first_parsed_report.timestamp.unwrap(),
        naive_date,
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
        let report_validation = junit::validator::validate(&first_parsed_report);

        assert_eq!(
            report_validation.max_level(),
            JunitValidationLevel::Valid,
            "{} of {} failed to validate with seed `{}`",
            index,
            COUNT,
            seed,
        );

        let mut serialized_first_parsed_report = Vec::new();
        first_parsed_report
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