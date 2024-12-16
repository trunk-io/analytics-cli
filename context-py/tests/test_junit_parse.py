def test_junit_parse_valid():
    import typing as PT
    from datetime import datetime, timezone

    from context_py import (
        BindingsNonSuccessKind,
        BindingsReport,
        BindingsTestCaseStatusStatus,
        junit_parse,
    )

    MICROSECONDS_PER_SECOND = 1_000_000

    valid_timestamp = datetime.now().astimezone(timezone.utc).isoformat()
    valid_junit_xml = f"""
    <testsuites name="my-test-run" tests="1" failures="1" errors="0">
      <testsuite name="my-test-suite" tests="1" disabled="0" errors="0" failures="1" timestamp="{valid_timestamp}">
        <testcase name="failure-case" file="test.py" classname="MyClass" timestamp="{valid_timestamp}" time="1">
          <failure message="AssertionError: assert 'testdata' in '# estdata'">
            FAILURE BODY
          </failure>
          <system-out/>
          <system-err/>
          <error message="       " type="">
            <!-- Example of a test case with empty error text. -->
          </error>
        </testcase>
      </testsuite>
      <testsuite name="" tests="1" disabled="0" errors="0" failures="1" timestamp="{valid_timestamp}">
        <testcase name="failure-case" file="test.py" classname="MyClass" timestamp="{valid_timestamp}" time="1">
          <failure message="AssertionError: assert 'testdata' in '# estdata'">
            FAILURE BODY
          </failure>
          <system-out/>
          <system-err/>
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

    assert len(report.test_suites) == 1
    test_suite = report.test_suites[0]

    assert test_suite.timestamp == int(
        datetime.fromisoformat(valid_timestamp).timestamp()
    )
    assert (
        test_suite.timestamp_micros
        == datetime.fromisoformat(valid_timestamp).timestamp() * MICROSECONDS_PER_SECOND
    )

    assert len(test_suite.test_cases) == 1
    test_case = test_suite.test_cases[0]

    assert test_case.status.status == BindingsTestCaseStatusStatus.NonSuccess
    assert test_case.status.non_success is not None
    assert test_case.status.non_success.kind == BindingsNonSuccessKind.Failure
    assert (
        test_case.status.non_success.message
        == "AssertionError: assert 'testdata' in '# estdata'"
    )
    assert test_case.status.non_success.description == "FAILURE BODY"


def test_junit_parse_non_xml():
    from context_py import junit_parse
    from pytest import raises

    simple_string = "no reports here!"

    with raises(Exception) as excinfo:
        _ = junit_parse(str.encode(simple_string))

    assert str(excinfo.value) == "no reports found"


def test_junit_parse_broken_xml():
    from context_py import junit_parse
    from pytest import raises

    broken_xml = b"<testsuites"

    with raises(Exception) as excinfo:
        _ = junit_parse(broken_xml)

    assert (
        str(excinfo.value)
        == "syntax error: tag not closed: `>` not found before end of input"
    )


def test_junit_parse_nested_testsuites():
    import typing as PT

    from context_py import BindingsReport, BindingsTestCaseStatusStatus, junit_parse

    nested_testsuites_xml = b"""<?xml version="1.0" encoding="UTF-8"?>
    <testsuites>
        <testsuite name="/home/runner/work/flake-farm/flake-farm/php/phpunit/phpunit.xml" tests="2" assertions="2" errors="0" failures="0" skipped="0" time="0.001161">
            <testsuite name="Project Test Suite" tests="2" assertions="2" errors="0" failures="0" skipped="0" time="0.001161">
                <testsuite name="EmailTest" file="/home/runner/work/flake-farm/flake-farm/php/phpunit/tests/EmailTest.php" tests="2" assertions="2" errors="0" failures="0" skipped="0" time="0.001161">
                    <testcase name="testCanBeCreatedFromValidEmail" file="/home/runner/work/flake-farm/flake-farm/php/phpunit/tests/EmailTest.php" line="6" class="EmailTest" classname="EmailTest" assertions="1" time="0.000860"/>
                    <testcase name="testCannotBeCreatedFromInvalidEmail" file="/home/runner/work/flake-farm/flake-farm/php/phpunit/tests/EmailTest.php" line="15" class="EmailTest" classname="EmailTest" assertions="1" time="0.000301"/>
                </testsuite>
            </testsuite>
        </testsuite>
    </testsuites>"""

    reports: PT.List[BindingsReport] = junit_parse(nested_testsuites_xml)

    assert len(reports) == 1
    report = reports[0]

    assert len(report.test_suites) == 1
    test_suite = report.test_suites[0]
    assert (
        test_suite.name
        == "/home/runner/work/flake-farm/flake-farm/php/phpunit/phpunit.xml"
    )

    assert len(test_suite.test_cases) == 2
    for test_case in test_suite.test_cases:
        assert test_case.status.status == BindingsTestCaseStatusStatus.Success
