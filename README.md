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

## Credentials

`--token` / `TRUNK_API_TOKEN` takes one of two credentials. The CLI reads which one before it calls Trunk:

- **A v2 API key** (`trunk_v2_...`). Create one in the Trunk web app under Settings → Developer → API Keys, and give it the **Upload test results** (`upload_test_results`) permission. The CLI sends it as `Authorization: Bearer`.
- **A Trunk JWT**, such as `trunk login --api-key` prints. The CLI sends it on `x-trunk-token`. It lasts 15 minutes, so mint it in the step right before the upload, not at the start of the job: a long `trunk-analytics-cli test` run, or a test-runner plugin that looks up quarantined tests early and uploads at the end, can outlive it. The [analytics-uploader][uploader] v3 does this for you. When Trunk refuses an expired JWT, the CLI says so; it does not refresh it.

analytics-cli 1.0.0 refuses any other token, the organization token included, before it calls Trunk. Pin analytics-cli 0.17.x to keep using the organization token.

If Trunk answers `403` because the key lacks the Upload test results permission, edit the key on the API Keys screen to add it.

`--allow-forked-pr-uploads` uploads with no token, only from a GitHub Actions run of a pull request from a fork: the CLI swaps the job's `GITHUB_TOKEN` for a Trunk login.

## Development

For more information about how to build and run the Rust CLI, please see [CONTRIBUTING.md][contributing].
