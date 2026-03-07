<!-- markdownlint-disable first-line-heading -->

[![docs](https://img.shields.io/badge/-docs-darkgreen?logo=readthedocs&logoColor=ffffff)][docs]
[![contributing](https://img.shields.io/badge/contributing-darkgreen?logo=readthedocs&logoColor=ffffff)][contributing]
[![slack](https://img.shields.io/badge/-slack-611f69?logo=slack)][slack]
[![dependency status](https://deps.rs/repo/github/trunk-io/analytics-cli/status.svg)](https://deps.rs/repo/trunk-io/analytics-cli)

[app]: https://app.trunk.io/?intent=flaky-tests
[contributing]: ./CONTRIBUTING.md
[docs]: https://docs.trunk.io/flaky-tests/ci-providers/other-ci-providers-quickstart
[flaky-tests]: https://docs.trunk.io/flaky-tests
[launcher]: https://docs.trunk.io/flaky-tests/uploader#installing-the-cli
[slack]: https://slack.trunk.io
[uploader]: https://github.com/trunk-io/analytics-uploader

# Trunk Analytics CLI

Rust CLI for uploading test output to [Trunk Flaky Tests][flaky-tests]. Please follow the instructions to [start uploading][app] or check out our [docs].
This is downloaded automatically using the [Trunk launcher][launcher] and when using the [analytics-uploader GitHub Action][uploader].

## CLI Usage

```
trunk-analytics-cli <COMMAND> [OPTIONS]
```

### Commands

| Command    | Description                                                                                     |
| ---------- | ----------------------------------------------------------------------------------------------- |
| `upload`   | Upload test results to Trunk Flaky Tests. Use when you've already run tests and have result files. |
| `test`     | Run a test command and upload results to Trunk Flaky Tests. Automatically detects and reports flaky tests. |
| `validate` | Validate test report files for compatibility with Trunk Flaky Tests. Runs locally without uploading data. |

### Upload / Test Options

These options are available for both the `upload` and `test` subcommands. The `test` subcommand additionally accepts a trailing positional argument for the test command to execute (e.g., `-- pytest tests/`).

#### Test Result Inputs

At least one of the following must be provided:

| Flag | Description |
| --- | --- |
| `--junit-paths <PATHS>` | Comma-separated list of glob patterns to locate JUnit XML files (e.g., `**/test-results/**/*.xml`). |
| `--bazel-bep-path <PATH>` | Path to Bazel Build Event Protocol JSON file. |
| `--test-reports <PATHS>` | Comma-separated list of glob patterns for test report files. Supports JUnit XML, Bazel BEP, and XCResult formats. |
| `--xcresult-path <PATH>` | Path to Xcode XCResult bundle directory (macOS only). |

#### Authentication & Organization

| Flag | Env Var | Description |
| --- | --- | --- |
| `--token <TOKEN>` | `TRUNK_API_TOKEN` | **Required.** Organization API token. |
| `--org-url-slug <SLUG>` | `TRUNK_ORG_URL_SLUG` | Organization URL slug. |

#### Repository Overrides

These flags override values that are normally auto-detected from the local git repository.

| Flag | Env Var | Description |
| --- | --- | --- |
| `--repo-root <PATH>` | `TRUNK_REPO_ROOT` | Path to repository root. Defaults to current directory. |
| `--repo-url <URL>` | `TRUNK_REPO_URL` | Override the repository URL. |
| `--repo-head-sha <SHA>` | `TRUNK_REPO_HEAD_SHA` | Override the HEAD commit SHA (max 40 characters). |
| `--repo-head-branch <BRANCH>` | `TRUNK_REPO_HEAD_BRANCH` | Override the HEAD branch name. |
| `--repo-head-commit-epoch <EPOCH>` | `TRUNK_REPO_HEAD_COMMIT_EPOCH` | Override the HEAD commit timestamp (seconds since Unix epoch). |
| `--repo-head-author-name <NAME>` | `TRUNK_REPO_HEAD_AUTHOR_NAME` | Override the HEAD commit author name. |
| `--pr-number <NUMBER>` | `TRUNK_PR_NUMBER` | Override the PR number. |
| `--use-uncloned-repo` | `TRUNK_USE_UNCLONED_REPO` | Enable upload for repos not cloned locally. Requires `--repo-url`, `--repo-head-sha`, `--repo-head-branch`, and `--repo-head-author-name`. |

#### Quarantining & Test Behavior

| Flag | Env Var | Description |
| --- | --- | --- |
| `--disable-quarantining[=BOOL]` | `TRUNK_DISABLE_QUARANTINING` | Disable test quarantining. Default: `false`. |
| `--allow-empty-test-results[=BOOL]` | `TRUNK_ALLOW_EMPTY_TEST_RESULTS` | Allow upload when no test results are found. Default: `true`. |
| `--test-process-exit-code <CODE>` | `TRUNK_TEST_PROCESS_EXIT_CODE` | Override the test process exit code. |
| `--codeowners-path <PATH>` | `TRUNK_CODEOWNERS_PATH` | Override path to a CODEOWNERS file. |
| `--variant <NAME>` | `TRUNK_VARIANT` | Variant name for test results (e.g., `linux`, `macos`). Max 64 characters. |

#### Output & Debugging

| Flag | Env Var | Description |
| --- | --- | --- |
| `--dry-run` | `TRUNK_DRY_RUN` | Write the bundle to a local file instead of uploading. |
| `--validation-report <LEVEL>` | `TRUNK_VALIDATION_REPORT` | Validation reporting verbosity: `limited` (default), `full`, or `none`. |
| `--show-failure-messages` | `TRUNK_SHOW_FAILURE_MESSAGES` | Include test failure messages in output. |
| `-v`, `-vv`, `-vvv` | | Increase output verbosity. |

### Validate Options

| Flag | Description |
| --- | --- |
| `--junit-paths <PATHS>` | Comma-separated list of glob patterns to locate JUnit XML files. |
| `--bazel-bep-path <PATH>` | Path to Bazel Build Event Protocol JSON file. |
| `--test-reports <PATHS>` | Comma-separated list of glob patterns for test report files. |
| `--codeowners-path <PATH>` | Override path to a CODEOWNERS file. |

### Examples

Upload JUnit XML results:

```bash
trunk-analytics-cli upload \
  --token $TRUNK_API_TOKEN \
  --org-url-slug my-org \
  --junit-paths "**/test-results/**/*.xml"
```

Run tests and upload results:

```bash
trunk-analytics-cli test \
  --token $TRUNK_API_TOKEN \
  --org-url-slug my-org \
  --junit-paths "**/test-results/**/*.xml" \
  -- pytest tests/
```

Validate test reports locally:

```bash
trunk-analytics-cli validate --junit-paths "**/test-results/**/*.xml"
```

## Development

For more information about how to build and run the Rust CLI, please see [CONTRIBUTING.md][contributing].
