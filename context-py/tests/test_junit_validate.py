def test_junit_validate_valid():
    import typing as PT
    from datetime import datetime, timezone

    from context_py import (
        BindingsReport,
        JunitValidationLevel,
        junit_parse,
        junit_validate,
    )

    valid_timestamp = datetime.now().astimezone(timezone.utc).isoformat()
    valid_junit_xml = f"""
    <testsuites name="my-test-run" tests="1" failures="1" errors="0">
      <testsuite name="my-test-suite" tests="1" disabled="0" errors="0" failures="1" timestamp="{valid_timestamp}">
        <testcase name="failure-case" file="test.py" classname="MyClass" timestamp="{valid_timestamp}" time="1">
          <failure message="AssertionError: assert 'testdata' in '# estdata'">
            FAILURE BODY
          </failure>

          <error message="       " type="">
            <!-- Example of a test case with empty error text. -->
          </error>
        </testcase>
      </testsuite>
    </testsuites>
   """

    reports: PT.List[BindingsReport] = junit_parse(str.encode(valid_junit_xml))

    assert len(reports) == 1
    report = reports[0]

    junit_report_validation = junit_validate(report)

    assert (
        junit_report_validation.max_level() == JunitValidationLevel.Valid
    ), "\n" + "\n".join(
        [
            issue.error_message
            for test_suite in junit_report_validation.test_suites
            for test_case in test_suite.test_cases_owned()
            for issue in test_case.issues_flat()
        ]
    )
    assert len(junit_report_validation.valid_test_suites) == 1


def test_junit_validate_suboptimal():
    import typing as PT
    from datetime import datetime, timedelta, timezone

    from context_py import (
        BindingsReport,
        JunitValidationLevel,
        JunitValidationType,
        junit_parse,
        junit_validate,
        junit_validation_level_to_string,
        junit_validation_type_to_string,
    )

    stale_timestamp = (
        (datetime.now() - timedelta(hours=30)).astimezone(timezone.utc).isoformat()
    )
    suboptimal_junit_xml = f"""
    <testsuites name="my-test-run" tests="1" failures="1" errors="0">
      <testsuite name="my-test-suite" tests="1" disabled="0" errors="0" failures="1" timestamp="{stale_timestamp}">
        <testcase name="failure-case" file="test.py" classname="MyClass" timestamp="{stale_timestamp}" time="1">
          <failure/>
        </testcase>
      </testsuite>
    </testsuites>
   """

    reports: PT.List[BindingsReport] = junit_parse(str.encode(suboptimal_junit_xml))

    assert len(reports) == 1
    report = reports[0]

    junit_report_validation = junit_validate(report)

    assert (
        junit_report_validation.max_level() == JunitValidationLevel.SubOptimal
    ), "\n" + "\n".join(
        [
            issue.error_message
            for test_suite in junit_report_validation.test_suites
            for test_case in test_suite.test_cases_owned()
            for issue in test_case.issues_flat()
        ]
    )
    assert junit_report_validation.num_suboptimal_issues() == 1
    report_level_issues = [
        x
        for x in junit_report_validation.all_issues
        if x.error_type == JunitValidationType.Report
    ]
    assert len(report_level_issues) == 1
    assert (
        junit_validation_type_to_string(report_level_issues[0].error_type) == "Report"
    )
    assert (
        junit_validation_level_to_string(report_level_issues[0].level) == "SUBOPTIMAL"
    )