import dayjs from "dayjs";
import utc from "dayjs/plugin/utc";
import { describe, expect, it } from "vitest";

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

    const ciInfo = env_parse(env_vars);
    // NOTE: Need to narrow type here
    // eslint-disable-next-line vitest/no-conditional-in-test
    if (!ciInfo) throw Error("ciInfo is undefined");
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
          </testcase>
        </testsuite>
      </testsuites>
    `;

    let report = junit_parse(Buffer.from(validJunitXml, "utf-8"));
    let junitReportValidation = junit_validate(report[0]);

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

    report = junit_parse(Buffer.from(suboptimalJunitXml, "utf-8"));
    junitReportValidation = junit_validate(report[0]);

    expect(junitReportValidation.max_level()).toBe(
      JunitValidationLevel.SubOptimal,
    );
    expect(junitReportValidation.num_suboptimal_issues()).toBe(1);
    expect(
      junitReportValidation
        .all_issues_owned()
        .filter((issue) => issue.error_type === JunitValidationType.Report),
    ).toHaveLength(1);
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

    expect(parse_branch_class("main")).toBe(BranchClass.ProtectedBranch);

    expect(parse_branch_class("testOwner/testFeature", 123)).toBe(
      BranchClass.PullRequest,
    );

    expect(() => parse_branch_class("")).toThrow(
      "could not parse branch class",
    );
  });

  it("validates merge branches", () => {
    expect.hasAssertions();

    expect(parse_branch_class("main")).toBe(BranchClass.ProtectedBranch);

    expect(
      parse_branch_class(
        "testOwner/testFeature",
        123,
        GitLabMergeRequestEventType.MergeTrain,
      ),
    ).toBe(BranchClass.Merge);

    expect(() => parse_branch_class("")).toThrow(
      "could not parse branch class",
    );
  });
});
