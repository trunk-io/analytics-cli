import dayjs from "dayjs";
import utc from "dayjs/plugin/utc";

import { describe, expect, it } from "vitest";

import {
  BundleRepo,
  CIPlatform,
  EnvValidationLevel,
  JunitValidationLevel,
  RepoUrlParts,
  RepoValidationLevel,
  env_parse,
  env_validate,
  junit_parse,
  junit_validate,
  repo_validate,
} from "../pkg/context_js";

dayjs.extend(utc);

describe("context-js", () => {
  it("parses and validates env variables", () => {
    const env_vars = {
      GITHUB_ACTIONS: "true",
      GITHUB_REF: "abc",
      GITHUB_ACTOR: "Spikey",
      GITHUB_REPOSITORY: "analytics-cli",
      GITHUB_RUN_ID: "12345",
    };

    const ciInfo = env_parse(env_vars);
    if (!ciInfo) throw Error("ciInfo is undefined");
    const envValidation = env_validate(ciInfo);
    expect(ciInfo.platform).toBe(CIPlatform.GitHubActions);
    expect(envValidation.max_level()).toBe(EnvValidationLevel.SubOptimal);
    expect(
      envValidation.issues_flat().map(({ error_message }) => error_message),
    ).toEqual([
      "CI info author email too short",
      "CI info author name too short",
      "CI info commit message too short",
      "CI info committer email too short",
      "CI info committer name too short",
      "CI info title too short",
    ]);
  });

  it("parses and validates junit files", () => {
    const timestamp = dayjs.utc().toISOString();
    const junitXml = `
      <testsuites name="my-test-run" tests="1" failures="1" errors="0">
        <testsuite name="my-test-suite" tests="1" disabled="0" errors="0" failures="1" timestamp="${timestamp}">
          <testcase name="failure-case" file="test.py" classname="MyClass" timestamp="${timestamp}" time="1">
            <failure/>
          </testcase>
        </testsuite>
      </testsuites>
    `;

    const report = junit_parse(Buffer.from(junitXml, "utf-8"));
    const junitReportValidation = junit_validate(report[0]);

    junitReportValidation
      .test_suites_owned()
      .forEach((test_suite_validation) => {
        test_suite_validation.issues_flat().forEach((issue) => {
          console.log(issue.error_message);
        });
        test_suite_validation
          .test_cases_owned()
          .forEach((test_case_validation) => {
            test_case_validation.issues_flat().forEach((issue) => {
              console.log(issue.error_message);
            });
          });
      });

    expect(junitReportValidation.max_level()).toBe(JunitValidationLevel.Valid);
  });

  it("validates repos", () => {
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

    repoValidation.issues_flat().forEach((issue) => {
      console.log(issue.error_message);
    });
    expect(repoValidation.max_level()).toBe(RepoValidationLevel.Valid);
  });
});
