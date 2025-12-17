# github-actions

This crate provides functionality to extract the external ID from GitHub Actions worker processes. This information is not natively surfaced by GitHub Actions, but is required to cross-reference CI jobs with uploads later in the analytics pipeline.

## Purpose

GitHub Actions does not natively expose:

- The **external ID** that uniquely identifies a job execution and surfaced in check-run webhooks
- The underlying **job ID** used internally by the GitHub Actions runner

However, this information is critical for correlating CI job executions with test result uploads. It isn't pretty, but this crate works around this limitation by:

1. Locating the `Runner.Worker` process that executes GitHub Actions jobs
2. Extracting the runner directory from the process command
3. Searching worker log files in the `_diag` directory for the external ID
4. Returning the external ID that can be used for cross-referencing

## Usage

### As a Library

```rust
use github_actions::extract_github_external_id;

match extract_github_external_id()? {
    Some(external_id) => {
        println!("Found external ID: {}", external_id);
    }
    None => {
        println!("Not running in GitHub Actions or external ID not found");
    }
}
```

### As a Binary

The crate also provides a standalone binary:

```bash
github-actions [--verbose]
```

The binary will exit with code 0 if an external ID is found, or code 1 if not found. This binary is used as a canary determine if GitHub has broken this logic for us. It isn't meant to actually be used locally. It is used in .github/workflows/pull_request.yml.

## Requirements

This crate must be run within a GitHub Actions environment (detected via the `GITHUB_ACTIONS` environment variable) and requires access to the runner's worker process and log files. It often won't make sense to run it locally.

## Known Footguns

Make sure that any tests added take into account that the environment variables specified when running in GitHub actions will be different than those run locally. You'll want to make sure those are cleared in the test context.
