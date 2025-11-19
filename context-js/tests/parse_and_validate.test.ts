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

    const ciInfo = env_parse(env_vars, ["main", "master"], undefined);
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

  it("repo fills in missing CI info values when use_uncloned_repo is None/False", () => {
    expect.hasAssertions();

    const env_vars = {
      GITHUB_ACTIONS: "true",
      GITHUB_REF: "refs/heads/feature-branch",
      GITHUB_ACTOR: "env-actor",
      GITHUB_REPOSITORY: "analytics-cli",
      GITHUB_RUN_ID: "12345",
      GITHUB_WORKFLOW: "test-workflow",
      GITHUB_JOB: "test-job",
    };

    const repo = new RepoUrlParts("github.com", "trunk-io", "analytics-cli");
    const bundleRepo = new BundleRepo(
      repo,
      ".",
      "https://github.com/trunk-io/analytics-cli",
      "abc123def456",
      "abc123d",
      "repo-branch-name",
      BigInt(1234567890),
      "This is a commit message from repo",
      "Repo Author Name",
      "repo-author@example.com",
      undefined, // use_uncloned_repo is undefined
    );

    const ciInfo = env_parse(env_vars, ["main", "master"], bundleRepo);
    assert(ciInfo);

    expect(ciInfo.platform).toBe(CIPlatform.GitHubActions);
    // Branch should come from env vars (not repo) since use_uncloned_repo is undefined
    expect(ciInfo.branch).toBe("feature-branch");
    // Actor should come from env vars
    expect(ciInfo.actor).toBe("env-actor");
    // Commit message should come from repo since it's missing in env vars
    expect(ciInfo.commit_message).toBe("This is a commit message from repo");
    // Author fields should come from repo since they're missing in env vars
    expect(ciInfo.author_name).toBe("Repo Author Name");
    expect(ciInfo.author_email).toBe("repo-author@example.com");
    expect(ciInfo.committer_name).toBe("Repo Author Name");
    expect(ciInfo.committer_email).toBe("repo-author@example.com");
  });

  it("repo overrides env vars when use_uncloned_repo is True", () => {
    expect.hasAssertions();

    const env_vars = {
      GITHUB_ACTIONS: "true",
      GITHUB_REF: "refs/heads/feature-branch",
      GITHUB_ACTOR: "env-actor",
      GITHUB_REPOSITORY: "analytics-cli",
      GITHUB_RUN_ID: "12345",
      GITHUB_WORKFLOW: "test-workflow",
      GITHUB_JOB: "test-job",
    };

    const repo = new RepoUrlParts("github.com", "trunk-io", "analytics-cli");
    const bundleRepoOverride = new BundleRepo(
      repo,
      ".",
      "https://github.com/trunk-io/analytics-cli",
      "abc123def456",
      "abc123d",
      "repo-override-branch",
      BigInt(1234567890),
      "Repo override commit message",
      "Repo Override Author",
      "repo-override@example.com",
      true, // use_uncloned_repo is true
    );

    const ciInfoOverride = env_parse(
      env_vars,
      ["main", "master"],
      bundleRepoOverride,
    );
    assert(ciInfoOverride);

    expect(ciInfoOverride.platform).toBe(CIPlatform.GitHubActions);
    // Branch should come from repo (overrides env var) when use_uncloned_repo is true
    expect(ciInfoOverride.branch).toBe("repo-override-branch");
    // Actor should come from repo (overrides env var)
    expect(ciInfoOverride.actor).toBe("repo-override@example.com");
    // Commit message should come from repo
    expect(ciInfoOverride.commit_message).toBe("Repo override commit message");
    // Author fields should come from repo
    expect(ciInfoOverride.author_name).toBe("Repo Override Author");
    expect(ciInfoOverride.author_email).toBe("repo-override@example.com");
    expect(ciInfoOverride.committer_name).toBe("Repo Override Author");
    expect(ciInfoOverride.committer_email).toBe("repo-override@example.com");
    // Branch class should be recalculated based on repo branch
    expect(ciInfoOverride.branch_class).toBeDefined();
  });

  it("repo fills in missing values when env vars are minimal/empty", () => {
    expect.hasAssertions();

    const env_vars_minimal = {
      GITHUB_ACTIONS: "true",
      GITHUB_REPOSITORY: "analytics-cli",
      GITHUB_RUN_ID: "12345",
    };

    const repo = new RepoUrlParts("github.com", "trunk-io", "analytics-cli");
    const bundleRepo = new BundleRepo(
      repo,
      ".",
      "https://github.com/trunk-io/analytics-cli",
      "abc123def456",
      "abc123d",
      "repo-branch-name",
      BigInt(1234567890),
      "This is a commit message from repo",
      "Repo Author Name",
      "repo-author@example.com",
      undefined, // use_uncloned_repo is undefined
    );

    const ciInfoMinimal = env_parse(
      env_vars_minimal,
      ["main", "master"],
      bundleRepo,
    );
    assert(ciInfoMinimal);

    // Branch should come from repo since it's missing in env vars
    expect(ciInfoMinimal.branch).toBe("repo-branch-name");
    // Actor should come from repo since it's missing in env vars
    expect(ciInfoMinimal.actor).toBe("repo-author@example.com");
    // Commit message should come from repo
    expect(ciInfoMinimal.commit_message).toBe(
      "This is a commit message from repo",
    );
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
      undefined, // use_uncloned_repo
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

  it("parses test_internal.bin", () => {
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

  it("parses test_internal_v2.bin", () => {
    expect.hasAssertions();

    const file_path = path.resolve(__dirname, "./test_internal_v2.bin");
    const file = fs.readFileSync(file_path);
    const bindingsReports = bin_parse(file);

    expect(bindingsReports).toHaveLength(1);

    const result = bindingsReports.at(0);

    expect(result?.bazel_build_information?.label).toBe(
      "//trunk/hello_world/cc:hello_test",
    );
    expect(result?.tests).toBe(1);
    expect(result?.test_suites).toHaveLength(1);

    const testSuite = result?.test_suites.at(0);

    expect(testSuite).toBeDefined();
    expect(testSuite?.test_cases).toHaveLength(1);
  });
});
