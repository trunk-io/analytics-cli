#!/usr/bin/env python3
"""Dump the per-test failure summaries out of an .xcresult bundle.

`xcrun xcresulttool get object --legacy` returns one object at a time and the
failure summaries hang several references deep, so walking to them takes a
handful of calls. This prints a JSON array — one entry per test that has
failure summaries — with the pieces the Rust attribution logic reads:
`fileName`, `sourceCodeContext.location.filePath`, and the call stack's
symbol names and file paths.

    ./dump-failure-summaries.py <Scenario>.xcresult > <scenario>.failure-summaries.json
"""

import json
import subprocess
import sys


def get_object(path, object_id=None):
    cmd = [
        "xcrun",
        "xcresulttool",
        "get",
        "object",
        "--path",
        path,
        "--format",
        "json",
        "--legacy",
    ]
    if object_id:
        cmd += ["--id", object_id]
    return json.loads(subprocess.run(cmd, check=True, capture_output=True).stdout)


def value(node):
    """Unwrap the legacy schema's `{"_value": ...}` boxes."""
    if isinstance(node, dict):
        return node.get("_value")
    return None


def values(node):
    if isinstance(node, dict):
        return node.get("_values", [])
    return []


def walk_tests(node, ancestors):
    """Yield every (name-path, summaryRef id, status) leaf under a test node."""
    name = value(node.get("name"))
    subtests = node.get("subtests")
    if subtests is not None:
        for subtest in values(subtests):
            yield from walk_tests(subtest, ancestors + [name])
        return
    summary_ref = node.get("summaryRef")
    if summary_ref is not None:
        yield (
            ancestors + [name],
            value(summary_ref.get("id")),
            value(node.get("testStatus")),
            value(node.get("identifier")),
        )


def source_code_context(context):
    if context is None:
        return None
    location = context.get("location") or {}
    return {
        "location.filePath": value(location.get("filePath")),
        "location.lineNumber": value(location.get("lineNumber")),
        "callStack": [
            {
                "symbolName": value((frame.get("symbolInfo") or {}).get("symbolName")),
                "filePath": value(
                    (((frame.get("symbolInfo") or {}).get("location")) or {}).get(
                        "filePath"
                    )
                ),
                "imageName": value((frame.get("symbolInfo") or {}).get("imageName")),
            }
            for frame in values(context.get("callStack"))
        ],
    }


def main():
    path = sys.argv[1]
    root = get_object(path)

    out = []
    for action in values(root.get("actions")):
        action_result = action.get("actionResult") or {}

        for issue in values(
            (action_result.get("issues") or {}).get("testFailureSummaries")
        ):
            document_location = issue.get("documentLocationInCreatingWorkspace") or {}
            out.append(
                {
                    "kind": "actionResult.issues.testFailureSummaries",
                    "testCaseName": value(issue.get("testCaseName")),
                    "producingTarget": value(issue.get("producingTarget")),
                    "documentLocationInCreatingWorkspace.url": value(
                        document_location.get("url")
                    ),
                }
            )

        tests_ref = action_result.get("testsRef")
        if tests_ref is None:
            continue
        plan = get_object(path, value(tests_ref.get("id")))
        for plan_summary in values(plan.get("summaries")):
            for testable in values(plan_summary.get("testableSummaries")):
                for test in values(testable.get("tests")):
                    for names, summary_id, status, identifier in walk_tests(test, []):
                        if status == "Success":
                            continue
                        summary = get_object(path, summary_id)
                        for failure in values(summary.get("failureSummaries")):
                            out.append(
                                {
                                    "kind": "test.failureSummaries",
                                    "testBundle": value(testable.get("name")),
                                    "namePath": names,
                                    "identifier": identifier,
                                    "testStatus": status,
                                    "message": value(failure.get("message")),
                                    "fileName": value(failure.get("fileName")),
                                    "lineNumber": value(failure.get("lineNumber")),
                                    "isPerformanceFailure": value(
                                        failure.get("isPerformanceFailure")
                                    ),
                                    "sourceCodeContext": source_code_context(
                                        failure.get("sourceCodeContext")
                                    ),
                                }
                            )

    json.dump(out, sys.stdout, indent=2)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
