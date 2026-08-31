# `swift test --xunit-output` fixture

A SwiftPM package whose test target covers every shape `swift test --xunit-output` emits,
and the XML it produced, checked in as `../../data/swift-test-xunit.junit.xml`.

Unlike the `.xcresult` scenarios next to this one, nothing here needs Xcode. `swift test`
produces no result bundle, `sourcekit-lsp` ships with the Swift toolchain on Linux, and the
xunit XML **contains no file path at all** — so a declaration is the only way to attribute
a test to a file on that platform.

## Regenerating

```sh
cp -R . /tmp/swift-test-xunit && cd /tmp/swift-test-xunit
swift test --parallel --xunit-output /tmp/out.xml
cp /tmp/out-swift-testing.xml ../../data/swift-test-xunit.junit.xml
cp /tmp/out.xml               ../../data/swift-test-xunit-xctest.junit.xml
```

`--parallel` is **required**: without it `--xunit-output` emits nothing for XCTest, and only
the swift-testing file appears.

Copy it out first: building in place leaves a `.build` directory inside the fixture, and the
declaration scan would then walk it (`.build` is in `SKIPPED_DIRECTORIES`, so it is ignored,
but it should not be committed either).

Note that one run writes **two files**, and neither is named what you asked for in the
XCTest case being the only one at `<name>`:

| file                       | holds                             |
| -------------------------- | --------------------------------- |
| `<name>-swift-testing.xml` | swift-testing (`@Test`, `@Suite`) |
| `<name>`                   | XCTest (`XCTestCase` subclasses)  |

A project with both frameworks has to upload both.

## What each shape proves

| `classname`                   | `name`           | declared in           | why it is here                                                                                                                          |
| ----------------------------- | ---------------- | --------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `MyCLITests`                  | `helloworld()`   | `TopLevel.swift`      | a top-level `@Test func` has no suite, so classname collapses to the bare target and only the suiteless lookup can find it              |
| `MyCLITests.AlphaSuite`       | `shared()`       | `Suites.swift`        | the ordinary case: innermost classname component is the declaring type                                                                  |
| `MyCLITests.AlphaSuite.Inner` | `deep()`         | `Suites.swift`        | a nested suite is fully qualified, and only the innermost component declares the method                                                 |
| `MyCLITests.BetaSuite`        | `shared()`       | `BetaSuite.swift`     | same case name as `AlphaSuite`'s in a **different file**, so collapsing the classname would make one borrow the other's file            |
| `MyCLITests.ParamSuite`       | `squares(n:)`    | `Parameterized.swift` | a parameterised test keeps its argument labels and appears **once**, not once per argument, so the single entry is the declaration site |
| `MyCLITests.ParamSuite`       | `pairs(s:flag:)` | `Parameterized.swift` | two labels, same shape                                                                                                                  |

## XCTest needs `--parallel`, and needs no special handling

`Legacy.swift` holds an `XCTestCase`, and it lands in `<name>` rather than
`<name>-swift-testing.xml`:

```xml
<testcase classname="MyCLITests.LegacyXCTests" name="testOldStyle" />
```

Which is the same `Module.Type` + method shape swift-testing uses, minus the `()` — so the
same parse resolves it, and `an_xctest_case_resolves_to_the_class_that_declares_it` proves
that against this package.

Worth knowing: **without `--parallel` this file is not written at all**. The XCTest case runs
either way (the console reports `-[MyCLITests.LegacyXCTests testOldStyle] passed`), so a
project that omits `--parallel` silently uploads only its swift-testing results.
