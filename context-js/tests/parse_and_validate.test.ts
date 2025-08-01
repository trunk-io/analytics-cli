import dayjs from "dayjs";
import fs from "fs";
import path from "path";
import utc from "dayjs/plugin/utc";
import { assert, describe, expect, it } from "vitest";

import {
  BundleRepo,
  CIPlatform,
  EnvValidationLevel,
  JunitValidationLevel,
  JunitValidationType,
  RepoUrlParts,
  RepoValidationLevel,
  env_parse,
  env_validate,
  junit_parse,
  junit_validate,
  repo_validate,
  parse_branch_class,
  BranchClass,
  GitLabMergeRequestEventType,
  bin_parse,
} from "../pkg/context_js";

// eslint-disable-next-line vitest/require-hook
dayjs.extend(utc);

describe("context-js", () => {
  it("parses and validates env variables", () => {
    expect.hasAssertions();

    const env_vars = {
      GITHUB_ACTIONS: "true",
      GITHUB_REF: "abc",
      GITHUB_ACTOR: "Spikey",
      GITHUB_REPOSITORY: "analytics-cli",
      GITHUB_RUN_ID: "12345",
      GITHUB_WORKFLOW: "test-workflow",
      GITHUB_JOB: "test-job",
    };

    const ciInfo = env_parse(env_vars, ["main", "master"]);
    assert(ciInfo);
    const envValidation = env_validate(ciInfo);

    expect(ciInfo.platform).toBe(CIPlatform.GitHubActions);
    expect(ciInfo.workflow).toBe("test-workflow");
    expect(ciInfo.job).toBe("test-job");
    expect(envValidation.max_level()).toBe(EnvValidationLevel.SubOptimal);
    expect(
      envValidation.issues_flat().map(({ error_message }) => error_message),
    ).toStrictEqual([
      "CI info author email too short",
      "CI info author name too short",
      "CI info commit message too short",
      "CI info committer email too short",
      "CI info committer name too short",
      "CI info title too short",
    ]);
  });

  it("parses and validates junit files", () => {
    expect.hasAssertions();

    const validTimestamp = dayjs.utc().toISOString();
    const validJunitXml = `
      <testsuites name="my-test-run" tests="1" failures="1" errors="0">
        <testsuite name="my-test-suite" tests="1" disabled="0" errors="0" failures="1" timestamp="${validTimestamp}">
          <testcase name="failure-case" file="test.py" classname="MyClass" timestamp="${validTimestamp}" time="1">
            <failure/>
            <system-out/>
            <system-err/>
          </testcase>
        </testsuite>
      </testsuites>
    `;

    let parse_result = junit_parse(Buffer.from(validJunitXml, "utf-8"));
    assert(parse_result.report);

    let junitReportValidation = junit_validate(parse_result.report);

    expect(junitReportValidation.max_level()).toBe(JunitValidationLevel.Valid);

    const staleTimestamp = dayjs.utc().subtract(30, "hour").toISOString();
    const suboptimalJunitXml = `
      <testsuites name="my-test-run" tests="1" failures="1" errors="0">
        <testsuite name="my-test-suite" tests="1" disabled="0" errors="0" failures="1" timestamp="${staleTimestamp}">
          <testcase name="failure-case" file="test.py" classname="MyClass" timestamp="${staleTimestamp}" time="1">
            <failure/>
          </testcase>
        </testsuite>
      </testsuites>
    `;

    parse_result = junit_parse(Buffer.from(suboptimalJunitXml, "utf-8"));
    assert(parse_result.report);

    junitReportValidation = junit_validate(parse_result.report);

    expect(junitReportValidation.max_level()).toBe(
      JunitValidationLevel.SubOptimal,
    );
    expect(junitReportValidation.num_suboptimal_issues()).toBe(1);
    expect(
      junitReportValidation
        .all_issues_owned()
        .filter((issue) => issue.error_type === JunitValidationType.Report),
    ).toHaveLength(1);

    junitReportValidation = junit_validate(parse_result.report, {
      resolved_status: "Passed",
      resolved_start_time_epoch_ms: dayjs.utc().subtract(5, "minute").valueOf(),
      resolved_end_time_epoch_ms: dayjs.utc().subtract(2, "minute").valueOf(),
    });

    expect(junitReportValidation.max_level()).toBe(JunitValidationLevel.Valid);

    const nestedJunitXml = `<?xml version="1.0" encoding="UTF-8"?>
      <testsuites>
          <testsuite name="/home/runner/work/flake-farm/flake-farm/php/phpunit/phpunit.xml" tests="2" assertions="2" errors="0" failures="0" skipped="0" timestamp="${validTimestamp}" time="0.001161">
              <testsuite name="Project Test Suite" tests="2" assertions="2" errors="0" failures="0" skipped="0" time="0.001161" timestamp="${validTimestamp}">
                  <testsuite name="" file="/home/runner/work/flake-farm/flake-farm/php/phpunit/tests/EmailTest.php" tests="2" assertions="2" errors="0" failures="0" skipped="0" time="0.001161" timestamp="${validTimestamp}">
                      <testcase name="testCanBeCreatedFromValidEmail" file="/home/runner/work/flake-farm/flake-farm/php/phpunit/tests/EmailTest.php" line="6" class="EmailTest" classname="EmailTest" assertions="1" time="0.000860" timestamp="${validTimestamp}"/>
                      <testcase name="testCannotBeCreatedFromInvalidEmail" file="/home/runner/work/flake-farm/flake-farm/php/phpunit/tests/EmailTest.php" line="15" class="EmailTest" classname="EmailTest" assertions="1" time="0.000301" timestamp="${validTimestamp}"/>
                  </testsuite>
              </testsuite>
          </testsuite>
      </testsuites>`;

    parse_result = junit_parse(Buffer.from(nestedJunitXml, "utf-8"));
    assert(parse_result.report);

    junitReportValidation = junit_validate(parse_result.report);

    expect(junitReportValidation.max_level()).toBe(JunitValidationLevel.Valid);
  });

  it("validates repos", () => {
    expect.hasAssertions();

    const repo = new RepoUrlParts("github", "trunk-io", "analytics-cli");

    const bundleRepo = new BundleRepo(
      repo,
      ".",
      "https://github.com/trunk-io/analytics-cli",
      "abc",
      "abc",
      "main",
      BigInt(dayjs.utc().unix()),
      "commit",
      "Spikey",
      "spikey@trunk.io",
    );

    const repoValidation = repo_validate(bundleRepo);

    expect(repoValidation.max_level()).toBe(RepoValidationLevel.Valid);
  });

  it("validates branch class", () => {
    expect.hasAssertions();

    expect(parse_branch_class("main", ["main", "master"])).toBe(
      BranchClass.ProtectedBranch,
    );

    expect(
      parse_branch_class("testOwner/testFeature", ["main", "master"], 123),
    ).toBe(BranchClass.PullRequest);

    expect(parse_branch_class("", [])).toBe(BranchClass.None);
  });

  it("validates merge branches", () => {
    expect.hasAssertions();

    expect(parse_branch_class("main", ["main", "master"])).toBe(
      BranchClass.ProtectedBranch,
    );

    expect(
      parse_branch_class(
        "testOwner/testFeature",
        ["main", "master"],
        123,
        GitLabMergeRequestEventType.MergeTrain,
      ),
    ).toBe(BranchClass.Merge);

    expect(parse_branch_class("", [])).toBe(BranchClass.None);
  });

  it("validates stable branches", () => {
    expect.hasAssertions();

    expect(parse_branch_class("main", ["main", "master"])).toBe(
      BranchClass.ProtectedBranch,
    );

    expect(parse_branch_class("main", ["master"])).toBe(BranchClass.None);

    expect(
      parse_branch_class("my-dev-branch", [
        "another-stable-branch",
        "my-dev-branch",
      ]),
    ).toBe(BranchClass.ProtectedBranch);
  });

  it("parses internal.bin", () => {
    expect.hasAssertions();

    const file_path = path.resolve(__dirname, "./test_internal.bin");
    const file = fs.readFileSync(file_path);
    const bindingsReports = bin_parse(file);

    expect(bindingsReports).toHaveLength(1);

    const result = bindingsReports.at(0);

    expect(result?.tests).toBe(13);
    expect(result?.test_suites).toHaveLength(2);

    const contextRubySuite = result?.test_suites.find(
      ({ name }) => name === "context_ruby",
    );

    expect(contextRubySuite).toBeDefined();
    expect(contextRubySuite?.test_cases).toHaveLength(5);

    const rspecExpectationsSuite = result?.test_suites.find(
      ({ name }) => name === "RSpec Expectations",
    );

    expect(rspecExpectationsSuite).toBeDefined();
    expect(rspecExpectationsSuite?.test_cases).toHaveLength(8);
  });
});
