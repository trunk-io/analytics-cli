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
