# Contributing to xcresult

The `xcresult` crate exists to handle converting between xcresult and JUnit formats. It parses Apple's xcresult bundle format and converts it to JUnit XML, which is a standard format for test results.

## Purpose

This crate serves three main purposes:

1. **Format Conversion**: Converts xcresult bundles (produced by Xcode test runs) into JUnit XML format for compatibility with various CI/CD systems and test reporting tools.

2. **Suite flattening**: JUnit has no nested `<testsuite>`, so a suite nested inside another
   becomes its own with a dot-qualified name (`Bundle.Outer.Inner`). This is not cosmetic —
   emitting only the outer suite silently drops every test the inner ones declare, which is
   what used to happen to nested swift-testing suites.

3. **Conditional File Path Specification**: While there are other xcresult parses, this crate handles specifying file paths in the JUnit output, which are conditionally present based on whether a failure (not error) has occurred. File paths are only included in the JUnit output when a test case has failed, as they are extracted from failure summaries in the xcresult bundle. This also handles generating stable identfiers because, by default, one of the values we generate IDs from is the file path. Without this crate, we wouldn't be able to safely map files to tests nor have codeowners support for xcresult.

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
- `--use-experimental-xcresult-test-locations`: Take each test's file from where it is declared rather than from a failure (optional boolean flag, also settable via `TRUNK_USE_EXPERIMENTAL_XCRESULT_TEST_LOCATIONS`)
- `--repo-root`: Checkout to resolve declarations in, defaults to the working directory (clap `requires` the flag above)

## Experimental: test locations from declarations

`--use-experimental-xcresult-test-locations` replaces the file-attribution half of this
crate, and is the same flag on `trunk-analytics-cli upload` (hidden, and equally
xcresult-only — it does nothing for JUnit or Bazel BEP uploads).

**What it changes.** Everything else here answers "where was this failure raised", because
that is all an `.xcresult` records: there is no per-test declaration site anywhere in the
bundle, and a passing test's summary carries no path at all. This flag asks a language
server instead — `sourcekit-lsp` for Swift, `clangd` for Objective-C, both of which ship
in the Command Line Tools as well as Xcode — for `textDocument/documentSymbol` over the
checkout's own sources, and joins the `(suite, case)` it gets back to the xcresult
identifier. So a failure raised inside a helper is attributed to the test's file rather
than the helper's, a crash with no call stack is attributed at all, and a **passing** test
gets a file for the first time.

**What it also changes, and is easy to miss.** The declaration path makes exactly one
`xcresulttool` call for results (`get test-results tests`) plus one for the run start time
(`get test-results summary`). It never issues `get object --legacy`, so the unbounded
per-test summary fetch — measured at 6 GB of JSON and a 48 GB peak footprint for a single
timed-out test — is not reachable from it.

**Where it is worse, and it is not a superset.** Tests registered at runtime (Quick's
`class_addMethod`, `+testInvocations`) have no declaration to find; the two approaches fail
in disjoint situations. Such a test falls back to the modern API's own `sourceLocation`,
vetted against the same vendored-path rules as everything in `src/file_attribution.rs`.

That fallback is thinner than it looks: `sourceLocation` is in the modern schema
(`TestNode.sourceLocation`) but is emitted in **none** of the bundles in `tests/data/`, so
in practice it never fires and such a test gets no file at all — where the failure-summary
path, which reads the call stack this path never fetches, would name one. `generate_junits`
logs the split (`N from a declaration, N from the fallback, N unresolved`) so it is visible
whether the fallback ever fires against real bundles.

**Collisions are broken by target.** The index is built from a checkout scan, not the build
log, so two same-named suites in different modules can both declare the same
`(suite, case)`. Scan order decides which is seen first and that order is arbitrary, so
`record` prefers a candidate lying under a directory named for the target that actually ran
the test — `nodeIdentifierURL` is
`test://com.apple.xcode/<scheme>/<target>/<suite>/<case>`, so the target is already in hand
from the field the ids come from, and no extra `xcresulttool` call is needed to get it.
Where no candidate is under the target, or the test has no target, the first file scanned
still wins. This matters because being confidently wrong is worse for codeowners than
reporting nothing, and it is the one way this path can be wrong where the failure-summary
path cannot, since that one reads the frame that actually ran.

**Incompatible with `--use-experimental-failure-summary`,** which tunes a code path this
one does not run, so clap rejects the pair rather than letting one silently win. One wart
comes with that: clap treats an env-supplied value as present regardless of what it says,
so `TRUNK_USE_EXPERIMENTAL_XCRESULT_TEST_LOCATIONS=false` **and**
`--use-experimental-failure-summary` is a hard conflict error. Unset the variable to roll
back rather than setting it to `false`.

**Every read goes through a copy.** `xcresulttool` migrates a pre-`database.sqlite3` bundle
in place the first time it is read, which writes into a directory the uploader was only
asked to read, fails outright with `exit 64` when that directory is not writable, and makes
two concurrent readers of one bundle race. Both constructors copy the bundle into a
`TempDir` first and read that, so the caller's bundle is never touched. This is on the
shared path, so it applies to the default one too.

**Ids.** `generate_id` prefers `nodeIdentifierURL` here, which is the legacy record's
`identifierURL` under another name, so ids do not move between the two paths.

**An xcresult and a `swift test` xunit give the same test different ids, deliberately for
now.** The two take different branches in `context/src/junit/parser.rs`: an xcresult test
case carries an `id` extra, a v5 UUID over `org#repo#identifierURL`, which is used verbatim;
a `swift test` xunit carries none, so identity falls to `gen_info_id` over
`(org, repo, file, classname, parent_name, name)` — the same scheme every other JUnit
uploader uses. Measured on one package captured both ways:

    shared()   xcresult e40658ab-…  xunit 73301b89-…

So a repository uploading both formats sees every test twice, and moving a repository from
one format to the other resets its history.

They are not unified yet because neither half is free. `identifierURL` leads with the
**xcodebuild scheme name**, which appears nowhere in xunit output, so the xunit path cannot
reproduce an xcresult id; and rederiving the xcresult id from normalised components would
reset history for every repository uploading `.xcresult` today. Giving the xunit path its own
third scheme in the meantime would cost those users two resets rather than one. The intent is
a single migration that moves both onto one format.

Two consequences worth knowing while that is outstanding. `file` is a `gen_info_id` input, so
turning `--swift-test-xunit-paths` on for a repository that previously uploaded the same file
via `--junit-paths` changes every id once, and a test whose declaration later moves to
another file changes id again — the xcresult scheme is immune to both, since its id ignores
`file`. And `name` is an input too, which is why
`the_two_inputs_spell_an_xctest_method_differently` pins `testOldStyle` against
`testOldStyle()`: that difference feeds the checksum.

**Cost.** Roughly 13 ms per file parsed. The scan is ranked so files named after a suite go
first and stops as soon as every test resolves; `Limits` caps files parsed, total wall
clock, and per-request time, and a server that stops answering is killed rather than waited
on.

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

The declaration path adds `src/lsp.rs` (a minimal JSON-RPC client) and
`src/test_locations.rs` (the `(suite, case) -> file:line` index), neither of which reads a
generated schema.

## Testing

Tests are located in `tests/xcresult.rs` and use sample xcresult bundles from `tests/data/`. To run tests:

```bash
# Run all tests
cargo test

# Run tests for this crate specifically
cargo test -p xcresult
```

Note: Almost all tests are macOS-specific (marked with `#[cfg(target_os = "macos")]`) as they require `xcrun` to be available.

The split is deliberate, because the parts that need macOS are narrower than they look:

- `src/test_locations.rs` unit-tests the symbol mapping, inheritance walk and source scan
  against canned `documentSymbol` responses.
- `src/xcresult.rs` unit-tests suite flattening and the attribution join against a canned
  `Tests` value and a seeded `TestLocationIndex` (`TestLocationIndex::declaring`, test-only).
  This is where "a **passing** test gets a file", "a failure raised in a helper or a
  dependency is still attributed to the test's file", and "no nested suite is dropped" are
  proven — none of which needs `xcrun`.
- `tests/xcresult.rs` holds the macOS tests that actually drive `sourcekit-lsp` and
  `clangd` over the checked-in packages in `tests/fixture-src/`. They pass a scenario's
  package directory as the repo root, which is why they assert the file a test is _written
  in_ rather than the absolute path baked into the bundle at capture time.

Both shapes that once had no fixture — a passing test and a nested suite — are covered by
`nested-and-passing`, captured from a package whose inner `@Suite` lives in a different
file from the suite containing it. It is the only scenario whose shape is structural
rather than a failure, so `regenerate.sh` verifies it with `verify-test-structure.py`.

Against that bundle the pre-fix traversal emits `tests="2" failures="0"`: the inner suite
is never visited, so its two tests are dropped and the run reports no failures despite
having one. Its three passing tests get a file only on the declaration path, since a test
that did not fail has no failure summary to name one.
