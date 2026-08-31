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

| `classname`                   | `name`           | declared in           | why it is here                                                                                                                                        |
| ----------------------------- | ---------------- | --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `MyCLITests`                  | `helloworld()`   | `TopLevel.swift`      | a top-level `@Test func` has no suite, so classname collapses to the bare target and only the suiteless lookup can find it                            |
| `MyCLITests.AlphaSuite`       | `shared()`       | `Suites.swift`        | the ordinary case: innermost classname component is the declaring type                                                                                |
| `MyCLITests.AlphaSuite.Inner` | `deep()`         | `Suites.swift`        | a nested suite is fully qualified, and only the innermost component declares the method                                                               |
| `MyCLITests.BetaSuite`        | `shared()`       | `BetaSuite.swift`     | same case name as `AlphaSuite`'s in a **different file**, so collapsing the classname would make one borrow the other's file                          |
| `MyCLITests.ParamSuite`       | `squares(n:)`    | `Parameterized.swift` | a parameterised test keeps its argument labels and appears **once**, not once per argument, so the single entry is the declaration site               |
| `MyCLITests.ParamSuite`       | `pairs(s:flag:)` | `Parameterized.swift` | two labels, same shape                                                                                                                                |
| `MyCLITests.OverloadSuite`    | `check()`        | `OverloadA.swift`     | the no-argument member of an overload set                                                                                                             |
| `MyCLITests.OverloadSuite`    | `check(a:)`      | `OverloadA.swift`     | differs from `check(b:)` **only by argument label**                                                                                                   |
| `MyCLITests.OverloadSuite`    | `check(b:)`      | `OverloadB.swift`     | declared in another file via an extension, so a normalisation that dropped labels would silently merge two distinct tests and give one the wrong file |

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

## Parens and argument labels are part of a test's identity

Both inputs report the same three overloads, and agree on how they spell them:

|                           | `check()`               | `check(a:)`               | `check(b:)`               |
| ------------------------- | ----------------------- | ------------------------- | ------------------------- |
| xcresult `nodeIdentifier` | `OverloadSuite/check()` | `OverloadSuite/check(a:)` | `OverloadSuite/check(b:)` |
| xunit `name`              | `check()`               | `check(a:)`               | `check(b:)`               |

`normalized_case` trims only _trailing_ parens, so `check()` becomes `check` while
`check(a:)` becomes the unbalanced `check(a:`. Ugly but correct: the same function is applied
to the language server's symbol name and to the test identifier, so what matters is that it
is _identical on both sides_, not that it is tidy. Because labels survive it, the three
overloads key distinctly and resolve to their own declarations — including across files,
which `overloads_differing_only_by_argument_label_resolve_separately` pins.

The one place the inputs genuinely disagree is an XCTest method: `xcodebuild` reports
`testOldStyle()` and `swift test` reports `testOldStyle`. Keying normalises that away so both
resolve to the same file, but the names differ in the uploaded JUnit — see
`the_two_inputs_spell_an_xctest_method_differently`.
