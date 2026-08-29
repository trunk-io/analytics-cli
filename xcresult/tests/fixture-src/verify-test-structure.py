#!/usr/bin/env python3
"""Assert that a captured .xcresult exhibits the *structure* it was captured for.

The sibling `verify-failure-summaries.py` covers scenarios whose shape is a failure
raised somewhere other than the test. Neither a suite nested in a suite nor a test
that simply passed is visible in a failure summary at all.

    ./verify-test-structure.py <scenario> <bundle.xcresult>
"""

import json
import subprocess
import sys

# `nested_suite` is an (outer, inner) pair; without one the flattening is unexercised.
SCENARIOS = {
    "nested-and-passing": {
        "nested_suite": ("OuterSuite", "InnerSuite"),
        "passing": [
            "OuterSuite/outerPasses()",
            "OuterSuite/InnerSuite/innerPasses()",
            "topLevelPasses()",
        ],
        "failing": ["OuterSuite/InnerSuite/innerFails()"],
    },
}


def walk(node, parent, suites, cases):
    kind = node.get("nodeType")
    if kind == "Test Suite":
        suites.append((parent, node.get("name")))
        parent = node.get("name")
    elif kind == "Test Case":
        cases.append(node)
    for child in node.get("children", []) or []:
        walk(child, parent, suites, cases)


def main():
    scenario, bundle = sys.argv[1], sys.argv[2]
    expected = SCENARIOS[scenario]
    tests = json.loads(
        subprocess.run(
            ["xcrun", "xcresulttool", "get", "test-results", "tests", "--path", bundle],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    )

    suites, cases = [], []
    for node in tests.get("testNodes", []):
        walk(node, None, suites, cases)
    by_identifier = {case.get("nodeIdentifier"): case for case in cases}

    failures = []
    if expected["nested_suite"] not in suites:
        failures.append(
            f"expected a nested suite {expected['nested_suite']}, found {suites}"
        )

    for identifier in expected["passing"] + expected["failing"]:
        case = by_identifier.get(identifier)
        if case is None:
            failures.append(
                f"{identifier}: not in the bundle (found {sorted(by_identifier)})"
            )
            continue
        failed = case.get("result") == "Failed"
        if identifier in expected["failing"] and not failed:
            failures.append(f"{identifier}: expected it to have failed")
        if identifier in expected["passing"] and failed:
            failures.append(f"{identifier}: expected it to have passed")
        # Falling back to `nodeIdentifier` would move every test case's id in the product.
        if not case.get("nodeIdentifierURL"):
            failures.append(f"{identifier}: no nodeIdentifierURL to derive an id from")

    if failures:
        print(f"{scenario}: bundle does not exhibit its structure", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        sys.exit(1)

    print(
        f"{scenario}: verified a nested suite and "
        f"{len(expected['passing'])} passing test(s)"
    )


if __name__ == "__main__":
    main()
