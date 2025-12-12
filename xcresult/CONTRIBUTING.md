# Contributing to xcresult

The `xcresult` crate exists to handle converting between xcresult and JUnit formats. It parses Apple's xcresult bundle format and converts it to JUnit XML, which is a standard format for test results.

## Purpose

This crate serves two main purposes:

1. **Format Conversion**: Converts xcresult bundles (produced by Xcode test runs) into JUnit XML format for compatibility with various CI/CD systems and test reporting tools.

2. **Conditional File Path Specification**: While there are other xcresult parses, this crate handles specifying file paths in the JUnit output, which are conditionally present based on whether a failure (not error) has occurred. File paths are only included in the JUnit output when a test case has failed, as they are extracted from failure summaries in the xcresult bundle. This also handles generating stable identfiers because, by default, one of the values we generate IDs from is the file path. Without this crate, we wouldn't be able to safely map files to tests nor have codeowners support for xcresult.

## Running the Binary

The crate provides a binary called `xcresult-to-junit` that can be used to convert xcresult bundles to JUnit XML.

### Basic Usage

```bash
# Build the binary
cargo build --bin xcresult-to-junit

# Run with a basic xcresult path (outputs to stdout)
cargo run --bin xcresult-to-junit -- /path/to/test.xcresult

# Run with output to a file
cargo run --bin xcresult-to-junit -- /path/to/test.xcresult --output-file-path junit.xml

# Run with repository information
cargo run --bin xcresult-to-junit -- \
  /path/to/test.xcresult \
  --org-url-slug=trunk-io \
  --repo-url=https://github.com/trunk-io/analytics-cli \
  --output-file-path junit.xml
```

### Command Line Options

- `xcresult` (required, positional): Path to the `.xcresult` directory or bundle to parse
- `--org-url-slug`: Organization URL slug (optional)
- `--repo-url`: Repository URL, e.g. `https://github.com/trunk-io/analytics-cli` (optional)
- `--output-file-path`: JUnit XML output file path (optional, defaults to stdout)
- `--use-experimental-failure-summary`: Use experimental failure summary parsing (optional boolean flag)

## JSON Schema Generation

The crate uses two Python scripts to generate JSON schema files that define the types we accept from xcresult. These schemas are then used by the build script (`build.rs`) to generate Rust types using the `typify` crate.

### Scripts

1. **`create-xcrun-xcresulttool-formatDescription-get---format-json---legacy-json-schema.py`**

   - Generates: `xcrun-xcresulttool-formatDescription-get---format-json---legacy-json-schema.json`
   - Purpose: Creates a JSON schema from Apple's xcresult format description (legacy format)
   - How it works: Calls `xcrun xcresulttool formatDescription get --format json --legacy` and converts the format description into a JSON schema format
   - Generates types: `ActionsInvocationRecord`, `ActionTestPlanRunSummaries`

2. **`create-xcrun-xcresulttool-get-test-results-tests-json-schema.py`**
   - Generates: `xcrun-xcresulttool-get-test-results-tests-json-schema.json`
   - Purpose: Creates a JSON schema from Apple's test results schema
   - How it works: Calls `xcrun xcresulttool get test-results tests --schema` and normalizes the schema format
   - Generates types: `Tests`

### Running the Scripts

To regenerate the JSON schema files:

```bash
# Generate the legacy format description schema
python3 create-xcrun-xcresulttool-formatDescription-get---format-json---legacy-json-schema.py

# Generate the test results schema
python3 create-xcrun-xcresulttool-get-test-results-tests-json-schema.py
```

**Note**: These scripts require macOS and `xcrun` to be available, as they call Apple's `xcresulttool` command-line tool.

### When to Update

The JSON schema files (`*.json`) only need to be updated periodically—specifically when an xcresult update is pushed out by Apple. This typically happens when:

- A new version of Xcode is released
- Apple updates the xcresult bundle format
- New types or fields are added to the xcresult schema

If you encounter parsing errors or missing fields when processing xcresult bundles, it may be time to regenerate these schemas by running the Python scripts with the latest version of Xcode installed.

## Build Process

During the build process, `build.rs` reads the JSON schema files and uses `typify` to generate Rust type definitions. These generated types are placed in the build output directory and used by the crate to deserialize xcresult data.

The generated types are used in:

- `src/types.rs` - Type definitions and schema modules
- `src/xcresult.rs` - Main conversion logic
- `src/xcresult_legacy.rs` - Legacy format handling

## Testing

Tests are located in `tests/xcresult.rs` and use sample xcresult bundles from `tests/data/`. To run tests:

```bash
# Run all tests
cargo test

# Run tests for this crate specifically
cargo test -p xcresult
```

Note: Almost all tests are macOS-specific (marked with `#[cfg(target_os = "macos")]`) as they require `xcrun` to be available.
