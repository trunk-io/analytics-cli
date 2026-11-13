# Upgrading from 0.17.x to 1.0.0

analytics-cli 1.0.0 uses only the Trunk API v2. It uploads only to test collections and takes only a v2 API key or a Trunk JWT. Most runs need three changes: a v2 API key, a test collection id, and no removed flags.

0.17.x stays available. Pin it to keep the organization token while you move: it gets important fixes on the `0.17.x` branch, and no new features.

## If you use analytics-uploader

- **v3** installs the trunk CLI and runs `trunk flakytests`, which resolves analytics-cli 1.x. Move to it to get 1.0.0.
- **v2.1.8 and later**, and the floating `@v2` tag, stay on 0.x: their `latest` reads `channel-0.json`.
- **v2.1.7 and earlier, pinned by SHA or exact tag,** resolve `latest` to the newest release and so get 1.0.0. Their uploads then fail with an organization token, without `test-collection-id`, or with the `quarantine` or `hide-banner` input set, which they pass as flags 1.0.0 removed. Move to v2.1.8 or v3 before 1.0.0 is the latest release, or set `cli-version: 0.17.0`.

## What you have to change

### Credentials

`--token` / `TRUNK_API_TOKEN` must hold one of:

- **A v2 API key** (`trunk_v2_...`) with the **Upload test results** permission. Create it in the Trunk web app under Settings → Developer → API Keys.
- **A Trunk JWT**, such as the one `trunk flakytests upload` passes. A JWT lasts at most 15 minutes and the CLI does not refresh it, so use a v2 API key for a long `test` run or a test-runner plugin.

  1.0.0 refuses the organization token before it calls Trunk; `test` refuses before it runs your tests. A key without Upload test results gets a `403` that names the permission. See [Credentials](README.md#credentials).

### Test collection

`--test-collection-id` / `TRUNK_TEST_COLLECTION_ID` is required for `upload` and `test`. A blank value counts as missing. 0.17.x inferred the collection from the repository; 1.0.0 does not. The rspec gem stays off, with a warning, without one.

### Removed flags and variables

| Removed                                                             | Instead                                                   |
| ------------------------------------------------------------------- | --------------------------------------------------------- |
| `--use-quarantining`                                                | Nothing for `true`; `--disable-quarantining` for `false`. |
| `--team`, `--print-files`, `--hide-banner`                          | Drop them; they did nothing.                              |
| `--public-repo-id` / `TRUNK_PUBLIC_REPO_ID`                         | `--allow-forked-pr-uploads` with `--test-collection-id`.  |
| `--hide-test-collection-links` / `TRUNK_HIDE_TEST_COLLECTION_LINKS` | Drop it. Every link is now a test collection link.        |

`upload` and `validate` exit with code 2 on a removed flag. `test` reads an unknown flag before the test command as part of the command, so the run fails with exit code 70.

### Fork pull requests

`--allow-forked-pr-uploads` with no token works only in a GitHub Actions run of a pull request from a fork. The CLI swaps the job's `GITHUB_TOKEN` for a Trunk login, so the job needs it in the environment. There is no fallback any more: a refused swap, a missing `GITHUB_TOKEN`, or the flag outside a fork pull request uploads nothing and says why.

### Output

- The summary goes to **stdout**; logs, notices and progress go to stderr. A script that reads only stderr loses the summary.
- `--output-format json` (or `TRUNK_OUTPUT_FORMAT=json`) on `upload` and `validate` prints the summary as the JSON that `--summary-output-file` writes.
- Test links use the test case's GUID (`/flaky-tests/collections/<id>/tests/<guid>`) when Trunk returns the collection, and the `/t/<id>?repo=` short link otherwise. The repository-scoped `/flaky-tests/test/<id>` link is gone.

### Network

1.0.0 sends its telemetry to `api.trunk.io` (`/v2/cloud/telemetry/v1/logs`), the host it already uploads to, instead of `telemetry.trunk.io`. A firewall allowlist needs only `api.trunk.io` and the storage host uploads go to.

## Test-runner plugins

`test_report` and the rspec gem share the CLI's API client, so the same rules apply: a v2 API key (or a JWT that outlasts the run) and `TRUNK_TEST_COLLECTION_ID`. Without either, the gem prints a warning and stays off.
