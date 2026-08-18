#!/usr/bin/env python3
"""Assert that a captured .xcresult actually exhibits the shape it was captured for.

A fixture that no longer reproduces its shape is worse than no fixture — it keeps
passing while guarding nothing (which is what happened to the older
`test-swift-snapshot-testing` bundle, whose `fileName` points at the test's own
source). `regenerate.sh` runs this before packaging anything, and refuses to
package a bundle that fails.

    ./verify-failure-summaries.py <scenario> <scenario>.failure-summaries.json
"""

import json
import sys

# Directory segments that mark vendored dependency sources. Kept in sync with
# `DEPENDENCY_PATH_SEGMENTS` in `xcresult/src/xcresult_legacy.rs`.
DEPENDENCY_PATH_SEGMENTS = ["/.build/", "/checkouts/", "/DerivedData/"]

# One entry per failure the scenario must produce, keyed by the test's identifier.
#
#   raised_from: where `fileName` / the source code context's location must point
#     - "dependency": a vendored path, which is what makes the fixture a regression
#       test — every non-call-stack source is unusable
#     - "in_repo_helper": a real path that is *not* the test's own file
#     - "absent": the failure carries no file at all
#   test_frame_symbol: the call-stack symbol that names the test itself, which is
#     the only thing that can identify the test's file. `None` means the stack must
#     *not* reach the test — the crash and teardown shapes, where no file may be
#     reported at all.
#   other_frames: whether the stack must also contain frames that are not the
#     test's, i.e. whether picking the last frame instead of the test's would land
#     somewhere else.
SCENARIOS = {
    "dependency-raises-failure": [
        {
            "identifier": "DependencyRaisesFailureTests/failsInsideDependency()",
            "raised_from": "dependency",
            "test_frame_symbol": "DependencyRaisesFailureTests.failsInsideDependency()",
            "other_frames": True,
        },
    ],
    "in-repo-helper-raises-failure": [
        {
            "identifier": "InRepoHelperRaisesFailureTests/failsInsideHelper()",
            "raised_from": "in_repo_helper",
            "test_frame_symbol": "InRepoHelperRaisesFailureTests.failsInsideHelper()",
            "other_frames": True,
        },
    ],
    "crash-in-dependency": [
        {
            "identifier": "CrashInDependencyTests/testCrashesInsideDependency()",
            "raised_from": "absent",
            "test_frame_symbol": None,
            "other_frames": False,
        },
        {
            "identifier": "TeardownFailureTests/failsAfterItsOwnFrameIsGone()",
            "raised_from": "dependency",
            "test_frame_symbol": None,
            "other_frames": True,
        },
    ],
    "objc-xctest": [
        {
            "identifier": "ObjcXCTestTests/testFailsInsideSharedHelper",
            "raised_from": "in_repo_helper",
            "test_frame_symbol": "-[ObjcXCTestTests testFailsInsideSharedHelper]",
            "other_frames": True,
        },
    ],
    "toplevel-swift-testing": [
        {
            "identifier": "failsInsideHelperWithoutASuite()",
            "raised_from": "in_repo_helper",
            "test_frame_symbol": "failsInsideHelperWithoutASuite()",
            "other_frames": True,
        },
    ],
}


def is_dependency_path(path):
    return path is not None and any(seg in path for seg in DEPENDENCY_PATH_SEGMENTS)


class Failures(list):
    def check(self, condition, message):
        if not condition:
            self.append(message)


def check_summary(failures, expectation, summary):
    identifier = expectation["identifier"]
    context = summary["sourceCodeContext"] or {}
    frames = context.get("callStack", [])
    raised_from = [summary["fileName"], context.get("location.filePath")]
    named = [
        frame
        for frame in frames
        if expectation["test_frame_symbol"] is not None
        and frame["symbolName"] == expectation["test_frame_symbol"]
    ]

    if expectation["raised_from"] == "dependency":
        for source in raised_from:
            failures.check(
                is_dependency_path(source),
                f"{identifier}: expected the failure to be raised from a dependency "
                f"path, got {source!r}",
            )
    elif expectation["raised_from"] == "absent":
        for source in raised_from:
            failures.check(
                source is None,
                f"{identifier}: expected no file source at all, got {source!r}",
            )
    else:
        for source in raised_from:
            failures.check(
                source is not None and not is_dependency_path(source),
                f"{identifier}: expected the failure to be raised from an in-repo "
                f"helper, got {source!r}",
            )

    if expectation["test_frame_symbol"] is None:
        failures.check(
            not any(
                frame["filePath"] and not is_dependency_path(frame["filePath"])
                for frame in frames
            ),
            f"{identifier}: expected the stack never to reach the test, but it has "
            f"a frame outside the dependency",
        )
    else:
        failures.check(
            len(named) == 1,
            f"{identifier}: expected exactly one frame named "
            f"{expectation['test_frame_symbol']!r}, found {len(named)}",
        )
        for frame in named:
            failures.check(
                frame["filePath"] is not None
                and not is_dependency_path(frame["filePath"]),
                f"{identifier}: the test's own frame must carry a non-dependency "
                f"file path, got {frame['filePath']!r}",
            )
            failures.check(
                frame["filePath"] not in raised_from,
                f"{identifier}: the test's own frame points at the same file the "
                f"failure was raised from ({frame['filePath']!r}), so the fixture "
                f"would pass without the fix",
            )

    if expectation["other_frames"]:
        symbolicated = [frame for frame in frames if frame["symbolName"]]
        failures.check(
            len(symbolicated) > len(named),
            f"{identifier}: expected at least one frame besides the test's own",
        )


def main():
    scenario, dump_path = sys.argv[1], sys.argv[2]
    expectations = SCENARIOS[scenario]
    dump = json.load(open(dump_path))
    summaries = {
        entry["identifier"]: entry
        for entry in dump
        if entry["kind"] == "test.failureSummaries"
    }

    failures = Failures()
    for expectation in expectations:
        summary = summaries.get(expectation["identifier"])
        if summary is None:
            failures.append(
                f"{expectation['identifier']}: no failure summary in the bundle "
                f"(found {sorted(summaries)})"
            )
            continue
        check_summary(failures, expectation, summary)

    if failures:
        print(f"{scenario}: bundle does not exhibit its shape", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        sys.exit(1)

    print(f"{scenario}: verified {len(expectations)} failure summary shape(s)")


if __name__ == "__main__":
    main()
