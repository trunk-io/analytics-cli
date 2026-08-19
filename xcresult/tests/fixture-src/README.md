# xcresult fixture sources

The `.xcresult` bundles in `../data/` are captured from the SwiftPM packages here.
Each package exists to reproduce one shape in which the file we report for a failed
test used to be a vendored dependency's rather than the test's own — see the commit
"fix(xcresult): attribute a failure to the test's own frame, not a dependency's".

Nothing in this directory is compiled by `cargo`; it is source for `regenerate.sh`,
checked in so the bundles can be rebuilt rather than being opaque binaries.

## Regenerating

Requires **macOS with Xcode** (`xcodebuild` and `xcrun xcresulttool`).

```sh
./regenerate.sh                    # every scenario
./regenerate.sh objc-xctest        # just one
```

For each scenario the script copies the package to `/tmp/xcresult-fixtures/`, turns
its `Dependency` directory into a git repository if it has one, runs

```sh
xcodebuild test -scheme <Package>-Package -destination 'platform=macOS' \
    -derivedDataPath DerivedData -resultBundlePath <Package>.xcresult
```

(nonzero exit is the expected outcome — these tests are meant to fail), strips the
bundle with `prune-bundle.py`, dumps the failure summaries with
`dump-failure-summaries.py`, checks them with `verify-failure-summaries.py`, and
only then packages the bundle into `../data/test-<scenario>.xcresult.tar.gz`.

Pruning is not an optimization; without it these are unshippable. Xcode 26 writes
about 95MB of dyld shared-cache symbolication data into every result bundle, none
of it referenced from the invocation record — a scenario whose actual test data is
under 100KB produces a 96MB bundle. `prune-bundle.py` walks the object graph from
the root (whose own id lives in `Info.plist`, not in any object) and deletes the
`Data/` entries nothing reached; for these scenarios that is 9-11 objects kept out
of ~1570 files. `regenerate.sh` captures both `xcresulttool` outputs the crate
reads — `get object --legacy` and `get test-results tests` — before and after, and
fails if pruning changed either, so the saving is verified rather than assumed.

Three details that are easy to get wrong:

- **The dependency has to be a git repository.** A `.package(path:)` dependency is
  built in place; only a git URL is checked out into
  `DerivedData/SourcePackages/checkouts/`, and that path _is_ the shape being
  reproduced. `regenerate.sh` creates the repository in the working copy so the
  checked-in sources stay a plain directory.
- **Absolute paths are baked into a bundle at capture time** and end up in the
  expected JUnit XML, which is why capture happens in a fixed directory. Set
  `FIXTURE_WORK_DIR` to move it, and expect every `file` attribute to change.
- **Recapturing changes the timestamps** in the expected JUnit XML even when
  nothing else moves. Diff with timestamps normalized to confirm that is all that
  changed before saving.

The expected JUnit XML is _not_ regenerated automatically. After recapturing, run
`cargo test -p xcresult`, read the diff, check each `file` attribute against the
table below, and only then save the new output over `../data/test-*.junit.xml`. A
`file` that lands on a dependency path is a bug to report, not output to snapshot.

## What each scenario must exhibit

`verify-failure-summaries.py` enforces the "captured shape" column and fails the
regeneration if a bundle stops reproducing it.

| Scenario                        | Captured shape                                                                                                                                                                                                                                                 | Expected `file` (experimental) | Expected `file` (legacy) |
| ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------ | ------------------------ |
| `dependency-raises-failure`     | A swift-testing test calls a dependency helper that records the issue at its own `#filePath`. `fileName`, the source code context's location and the innermost frame are all under `SourcePackages/checkouts/`; only the test's own frame names the test file. | the test's own file            | _(none)_                 |
| `in-repo-helper-raises-failure` | Same, with the helper in the test target. Nothing rejects the helper's path, so the test's own frame has to win on its own merits.                                                                                                                             | the test's own file            | the helper's file        |
| `crash-in-dependency`           | Two tests that never reach their own frame: one `fatalError`s inside the dependency (Xcode records a summary with no file at all), and one is failed by the dependency's `TestScoping` trait after its body returned (every file source is a checkout path).   | _(none)_                       | _(none)_                 |
| `objc-xctest`                   | An Objective-C `XCTestCase` whose failure is raised by `XCTFail` in a shared category in another file, so the frame is symbolicated as `-[ObjcXCTestTests testFailsInsideSharedHelper]`.                                                                       | the test's own file            | _(none)_                 |
| `toplevel-swift-testing`        | A top-level `@Test func` with no suite, failed by a helper in another file, so the frame is the bare function name.                                                                                                                                            | the test's own file            | the helper's file        |

The two columns differ because only the experimental path reads the failure
summary's call stack, which is the only source that can identify the test's own
file. The legacy path sees just the workspace document location — the file the
failure was _raised_ from — so where that is a dependency it now reports no file,
and where it is an in-repo helper it still reports the helper. `objc-xctest` has no
legacy file for an unrelated reason: the fallback is keyed by test-case name and
Xcode spells the Objective-C one `-[Suite testCase]`, which never matches the
`Suite.testCase` key the lookup builds.

To read what a captured bundle actually contains, run `dump-failure-summaries.py`
against it — the dump is derived from the bundle, so it is not checked in.

## Why not the older SnapshotTesting fixture

`../data/test-swift-snapshot-testing.xcresult.tar.gz` looks like it covers the
first scenario and does not: `assertSnapshot` takes `filePath: StaticString =
#filePath`, which is evaluated at the _call site_, so its `fileName` already points
at the test's own source. It passes with or without the fix. A helper that wants to
report its own location — which is what a trait or a page object does — has to build
the `SourceLocation` inside its body, which is what these fixtures do.
