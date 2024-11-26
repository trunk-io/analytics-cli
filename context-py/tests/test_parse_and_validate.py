def test_env_parse_and_validate():
    from context_py import CIPlatform, EnvValidationLevel, env_parse, env_validate

    env_vars = {
        "GITHUB_ACTIONS": "true",
        "GITHUB_REF": "abc",
        "GITHUB_ACTOR": "Spikey",
        "GITHUB_REPOSITORY": "analytics-cli",
        "GITHUB_RUN_ID": "12345",
        "GITHUB_WORKFLOW": "test-workflow",
        "GITHUB_JOB": "test-job",
    }

    ci_info = env_parse(env_vars)
    assert ci_info is not None
    env_validation = env_validate(ci_info)

    assert ci_info.platform == CIPlatform.GitHubActions
    assert ci_info.workflow == "test-workflow"
    assert ci_info.job == "test-job"
    assert env_validation.max_level() == EnvValidationLevel.SubOptimal
    assert [issue.error_message for issue in env_validation.issues_flat()] == [
        "CI info author email too short",
        "CI info author name too short",
        "CI info commit message too short",
        "CI info committer email too short",
        "CI info committer name too short",
        "CI info title too short",
    ], "\n" + "\n".join([issue.error_message for issue in env_validation.issues_flat()])


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

    broken_xml = "<testsuites"

    with raises(Exception) as excinfo:
        _ = junit_parse(str.encode(broken_xml))

    assert (
        str(excinfo.value)
        == "syntax error: tag not closed: `>` not found before end of input"
    )


def test_junit_parse_nested_testsuites():
    import typing as PT

    from context_py import BindingsReport, BindingsTestCaseStatusStatus, junit_parse

    nested_testsuites_xml = """<?xml version="1.0" encoding="UTF-8"?>
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

    reports: PT.List[BindingsReport] = junit_parse(str.encode(nested_testsuites_xml))

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
            for test_suite in junit_report_validation.test_suites_owned()
            for test_case in test_suite.test_cases_owned()
            for issue in test_case.issues_flat()
        ]
    )


def test_junit_validate_suboptimal():
    import typing as PT
    from datetime import datetime, timedelta, timezone

    from context_py import (
        BindingsReport,
        JunitValidationLevel,
        JunitValidationType,
        junit_parse,
        junit_validate,
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
            for test_suite in junit_report_validation.test_suites_owned()
            for test_case in test_suite.test_cases_owned()
            for issue in test_case.issues_flat()
        ]
    )
    assert junit_report_validation.num_suboptimal_issues() == 1
    assert (
        len(
            [
                x
                for x in junit_report_validation.all_issues_owned()
                if x.error_type == JunitValidationType.Report
            ]
        )
        == 1
    )


def test_repo_validate():
    import math
    import time

    from context_py import BundleRepo, RepoUrlParts, RepoValidationLevel, repo_validate

    repo = RepoUrlParts(host="github", owner="trunk-io", name="analytics-cli")
    bundle_repo = BundleRepo(
        repo,
        ".",
        "https://github.com/trunk-io/analytics-cli",
        "abc",
        "abc",
        "main",
        math.floor(time.time()),
        "commit",
        "Spikey",
        "spikey@trunk.io",
    )

    repo_validation = repo_validate(bundle_repo)

    assert repo_validation.max_level() == RepoValidationLevel.Valid, "\n" + "\n".join(
        [issue.error_message for issue in repo_validation.issues_flat()]
    )


def test_parse_meta_from_tarball():
    import io
    import json
    import tarfile
    import tempfile

    import zstandard as zstd
    from botocore.response import StreamingBody
    from context_py import parse_meta_from_tarball

    # trunk-ignore(pyright/reportUnknownVariableType)
    expected_meta = {
        "version": "1",
        "bundle_upload_id": "59c8ddd9-0a00-4b56-9eea-ef0d60ebcb79",
        "cli_version": "cargo=0.5.11 git=7e5824fa365c63a2d4b38020762be17f4edd6425 rustc=1.80.0-nightly",
        "org": "trunk",
        "repo": {
            "repo": {"host": "github.com", "owner": "trunk", "name": "test"},
            "repo_root": "/home/runner/work/trunk/test",
            "repo_url": "https://github.com/trunk/test",
            "repo_head_sha": "74518d470d8cfeb41408a85cf6097bb7f09ad902",
            "repo_head_branch": "refs/heads/main",
            "repo_head_commit_epoch": 1720652103,
            "repo_head_commit_message": "ci: add .deepsource.toml",
            "repo_head_author_name": "deepsource-io[bot]",
            "repo_head_author_email": "42547082+deepsource-io[bot]@users.noreply.github.com",
        },
        "tags": [],
        "file_sets": [
            {
                "file_set_type": "Junit",
                "files": [
                    {
                        "original_path": "/home/runner/work/trunk/test/file1.xml",
                        "path": "junit/0",
                        "last_modified_epoch_ns": 1721095230341044019,
                        "owners": [],
                        "team": "",
                    },
                    {
                        "original_path": "/home/runner/work/trunk/test/file2.xml",
                        "path": "junit/1",
                        "last_modified_epoch_ns": 1721095230341044019,
                        "owners": [],
                        "team": "",
                    },
                ],
                "glob": "junit.xml",
            }
        ],
        "envs": {
            "GITHUB_ACTION_REPOSITORY": "",
            "GITHUB_SERVER_URL": "https://github.com",
            "GITHUB_REPOSITORY": "trunk/test",
            "GITHUB_HEAD_REF": "",
            "GITHUB_RUN_ATTEMPT": "1",
            "GITHUB_BASE_REF": "",
            "RUNNER_ARCH": "X64",
            "RUNNER_OS": "Linux",
            "GITHUB_ACTOR": "trunk",
            "GITHUB_REF": "refs/heads/main",
            "GITHUB_EVENT_NAME": "schedule",
            "GITHUB_SHA": "74518d470d8cfeb41408a85cf6097bb7f09ad902",
            "GITHUB_WORKFLOW": "Sleeper Test",
            "GITHUB_REF_PROTECTED": "true",
            "CI": "true",
            "GITHUB_RUN_NUMBER": "4507",
            "GITHUB_ACTIONS": "true",
            "GITHUB_RUN_ID": "9949497745",
        },
        "upload_time_epoch": 1721095230,
        "test_command": None,
        "os_info": "linux",
        "group_is_quarantined": None,
        "quarantined_tests": [],
    }

    encoded_meta = json.dumps(expected_meta).encode()

    meta_tarball_compressed: bytes = b""
    with tempfile.TemporaryDirectory() as tempdir:
        meta_file_path = f"{tempdir}/meta.json"
        with open(meta_file_path, "wb") as f:
            f.write(encoded_meta)
        meta_tarball_path = f"{tempdir}/meta.tar"
        with tarfile.open(meta_tarball_path, "w") as tar:
            tar.add(meta_file_path, "meta.json")
            for i in range(1000):
                tar.add(meta_file_path, f"junk{str(i)}")
        with open(meta_tarball_path, "rb") as f:
            encoder = zstd.ZstdCompressor(level=6)
            meta_tarball_compressed = encoder.compress(f.read())

    raw_stream = StreamingBody(
        io.BytesIO(meta_tarball_compressed), len(meta_tarball_compressed)
    )

    versioned_bundle = parse_meta_from_tarball(raw_stream)

    # Check that the entire stream isn't read
    assert raw_stream.tell() < len(meta_tarball_compressed)

    assert versioned_bundle.get_v0_5_34() is None
    assert versioned_bundle.get_v0_6_2() is None

    bundle_meta = versioned_bundle.get_v0_5_29()
    assert bundle_meta is not None
    assert bundle_meta.base_props.bundle_upload_id == expected_meta["bundle_upload_id"]
