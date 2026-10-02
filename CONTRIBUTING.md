# Contributing

These are instructions for building, running, and testing the Rust CLI locally. Note that any changes are tightly coupled with Trunk Flaky Test services.

## Prerequisites

- Install a nightly version of Cargo using [rustup](https://doc.rust-lang.org/cargo/getting-started/installation.html)
- Run `trunk tools install`

## Build

### Build Everything

```bash
cargo build
```

The CLI will be built to `target/debug/trunk-analytics-cli`

### Package-Specific Build Instructions

For detailed build instructions for each supported package, see their respective README files:

- **Python Bindings**: See [context-py/README.md](context-py/README.md)
- **JavaScript/TypeScript Bindings**: See [context-js/README.md](context-js/README.md)
- **Ruby Gem (RSpec Plugin)**: See [rspec-trunk-flaky-tests/README.md](rspec-trunk-flaky-tests/README.md)

## Run

```bash
cargo build
./target/debug/trunk-analytics-cli upload --org-url-slug=trunk-io --token=${API_TOKEN} --junit-paths=junit.xml
```

You can generate sample junit files by running

```bash
cargo run --bin junit-mock .
```

You can change the API endpoint by setting `TRUNK_PUBLIC_API_ADDRESS=https://api.trunk.io`. To use localhost, you should use `TRUNK_PUBLIC_API_ADDRESS=http://localhost:9010 DEBUG_STRIP_VERSION_PREFIX=true`

## Test

### Using nextest (Recommended)

This project uses [nextest](https://nexte.st/) for running Rust tests. It provides faster test execution, better output, and more reliable test runs.

Install nextest:

```bash
cargo install cargo-nextest --locked
```

Run tests with nextest:

```bash
# Run all tests
cargo nextest run

# Run tests with CI profile (includes JUnit output)
cargo nextest run --profile ci

# Run tests for a specific package
cargo nextest run -p <package-name>
```

### Using cargo test

You can also use the standard `cargo test` command if you really want:

```bash
cargo test
```

## Logging and Output

### Tracing Library

The `tracing` library is used for:

- **Sentry reporting**: Errors and warnings are automatically sent to Sentry for monitoring and debugging
- **Debug logging**: Internal debug information that helps with development and troubleshooting

**Important**: Tracing output does **not** surface to users unless they use the `--verbose` flag. By default, no tracing messages are shown in the console. They are primarily for Sentry reporting and debugging.

### Organization Slug Is Required for Telemetry

`--org-url-slug` (env `TRUNK_ORG_URL_SLUG`) is declared as a non-optional `String` on `UploadArgs`, which makes it a hard requirement for both `upload` and `test`. Keep it that way: the slug is what attributes a run to an organization across every kind of telemetry we collect.

- **Sentry**: `setup_logger` in `cli/src/main.rs` attaches it as an `org_url_slug` tag on every `tracing::error!()` event forwarded to Sentry, alongside `command_name` and `repo_root`. Those tags are how Trunk narrows Sentry to a single customer's CI runs when someone reports a failing upload — without the slug, an error report can't be tied back to an organization and we have to ask the user to reproduce.
- **Our own upload telemetry**: the slug is stored on the bundle as `base_props.org` and scopes the `UploadMetrics` we report to the telemetry endpoint at the end of a run (`cli/src/upload_command.rs`, `ApiClient::telemetry_upload_metrics`). Timing, quarantine outcome, and failure-reason metrics are only useful if we know which org they came from.

The slug also travels with the upload requests, builds the org-scoped links we print for uploads and tests (`api/src/urls.rs`), and backs the settings-page hint shown on unauthorized errors (`cli/src/error_report.rs`).

`validate` is the one exception. It runs entirely locally and does not accept the flag, so `Cli::org_url_slug()` returns the placeholder `"not used"` for that subcommand — errors from `validate` land in Sentry with that placeholder tag rather than a real org.

### Superconsole

The `superconsole` library is used for **surfacing information to users**. This is the primary mechanism for displaying:

- Progress messages
- Status updates
- Final results
- User-facing error messages

### Error Handling

When errors occur:

1. **Always capture errors with `tracing::error!()`** - This ensures errors are logged to Sentry for monitoring
2. **Surface blocking errors to users via `superconsole`** - If an error blocks execution, it should be displayed to the user through the display system (see `cli/src/error_report.rs` for examples)

The pattern is:

- Use `tracing` for observability and debugging (Sentry + optional verbose output)
- Use `superconsole` for user-facing communication
- Errors that block execution should use both: `tracing` for logging and `superconsole` for user display

## Releasing

Releases are published in two places: GitHub releases, and `https://trunk.io/releases/analytics-cli/prod/` (the `trunk-releases` S3 bucket). Trunk's own consumers (the `analytics-uploader` action and the `trunk` CLI's `flakytests` command) read the S3 copy. GitHub releases stay, because the frozen legacy `trunk` launcher and customers' own scripts download from them.

1. **Cut**: run the `Release` workflow with the version. It builds, smoke-tests, creates a GitHub **prerelease**, and uploads the same assets to `releases/analytics-cli/prod/<version>/`. A version is immutable once published.
2. **Promote**: run the `Promote Release` workflow with the version. It points `releases/analytics-cli/prod/channel.json` at it and marks it latest on GitHub. Do **not** mark a release latest in the GitHub UI, since that leaves `channel.json` behind.
3. **Backfill**: run `Mirror Releases to S3` to copy releases that predate the S3 lane (or that a failed run left incomplete). Pass space-separated tags or `all`. Re-running it is safe because complete versions are skipped.

Layout under `releases/analytics-cli/prod/`:

- `channel.json`: `{"latest": "<version>"}`, the only mutable object (`Cache-Control: max-age=60`).
- `<version>/<asset>`: each GitHub release asset under the same filename, e.g. `trunk-analytics-cli-x86_64-unknown-linux.tar.gz`.
- `<version>/manifest.json`: `{"version": "<version>", "artifacts": {"<asset>": {"sha256": "<hex>"}}}`, uploaded last, so its presence means the version is complete.

One-time prerequisite: the publishing workflows assume the IAM role `trunk-analytics-cli-release-role` in AWS account 443897627245 over GitHub OIDC, through the repository variable `TRUNK_RELEASES_PUBLISH_ROLE_ARN`. The role is granted only the `releases/analytics-cli/*` prefix, plus `s3:ListBucket` on it so a missing key reads as 404 rather than 403. Because this is a public repository, its trust should be restricted to `repo:trunk-io/analytics-cli:ref:refs/heads/main`, not `repo:trunk-io/analytics-cli:*`. See trunk2's `docs/runbooks/trunk-releases-publish-roles.md` for how these roles are created.
