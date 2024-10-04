def test_env_parse_and_validate():
  from context_py import env_parse, env_validate, CIPlatform, EnvValidationLevel

  env_vars = {
    'GITHUB_ACTIONS': 'true',
    'GITHUB_REF': 'abc',
    'GITHUB_ACTOR': 'Spikey',
    'GITHUB_REPOSITORY': 'analytics-cli',
    'GITHUB_RUN_ID': '12345',
  }

  ci_info = env_parse(env_vars)
  env_validation = env_validate(ci_info)

  assert ci_info.platform == CIPlatform.GitHubActions
  assert env_validation.max_level() == EnvValidationLevel.SubOptimal
  assert [
    issue.error_message
    for issue in env_validation.issues_flat()
  ] == [
    "CI info author email too short",
    "CI info author name too short",
    "CI info commit message too short",
    "CI info committer email too short",
    "CI info committer name too short",
    "CI info title too short",
  ], '\n' + '\n'.join([
    issue.error_message
    for issue in env_validation.issues_flat()
  ])

def test_junit_parse_and_validate():
  import datetime
  from context_py import junit_parse, junit_validate, JunitValidationLevel

  timestamp=datetime.datetime.now().astimezone(datetime.timezone.utc).isoformat()
  junit_xml = f"""
    <testsuites name="my-test-run" tests="1" failures="1" errors="0">
      <testsuite name="my-test-suite" tests="1" disabled="0" errors="0" failures="1" timestamp="{timestamp}">
        <testcase name="failure-case" file="test.py" classname="MyClass" timestamp="{timestamp}" time="1">
          <failure/>
        </testcase>
      </testsuite>
    </testsuites>
   """;

  report = junit_parse(str.encode(junit_xml))
  junit_report_validation = junit_validate(report[0])

  assert junit_report_validation.max_level() == JunitValidationLevel.Valid, '\n' + '\n'.join([
    issue.error_message
    for test_suite in junit_report_validation.test_suites_owned()
    for test_case in test_suite.test_cases_owned()
    for issue in test_case.issues_flat()
  ])

def test_repo_validate():
  import math
  import time
  from context_py import repo_validate, BundleRepo, RepoUrlParts, RepoValidationLevel

  repo = RepoUrlParts("github", "trunk-io", "analytics-cli")
  bundle_repo = BundleRepo(
    repo,
    ".",
    "https://github.com/trunk-io/analytics-cli",
    "abc",
    "main",
    math.floor(time.time()),
    "commit",
    "Spikey",
    "spikey@trunk.io",
  )

  repo_validation = repo_validate(bundle_repo)

  assert repo_validation.max_level() == RepoValidationLevel.Valid, '\n' + '\n'.join([
    issue.error_message
    for issue in repo_validation.issues_flat()
  ])
