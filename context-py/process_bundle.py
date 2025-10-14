#!/usr/bin/env python3
"""
Script to process bundle files using bin_parse and junit_parse functions.

This script takes a directory path containing an unzipped bundle and:
1. Applies bin_parse to the internal.bin file
2. Applies junit_parse to each junit file in the junit subdirectory
3. Prints both representations of the parsed formats
"""

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any, Dict, List

from context_py import bin_parse, junit_parse


def read_binary_file(file_path: Path) -> bytes:
    """Read a binary file and return its contents as bytes."""
    try:
        with open(file_path, "rb") as f:
            return f.read()
    except FileNotFoundError:
        print(f"Error: File not found: {file_path}")
        sys.exit(1)
    except Exception as e:
        print(f"Error reading file {file_path}: {e}")
        sys.exit(1)


def read_text_file(file_path: Path) -> bytes:
    """Read a text file and return its contents as bytes."""
    try:
        with open(file_path, "rb") as f:
            return f.read()
    except FileNotFoundError:
        print(f"Error: File not found: {file_path}")
        sys.exit(1)
    except Exception as e:
        print(f"Error reading file {file_path}: {e}")
        sys.exit(1)


def read_meta_json(bundle_dir: Path) -> Dict[str, Any]:
    """Read and parse the meta.json file from the bundle directory."""
    meta_path = bundle_dir / "meta.json"

    if not meta_path.exists():
        print(f"Error: meta.json not found in {bundle_dir}")
        return {}

    try:
        with open(meta_path, "r", encoding="utf-8") as f:
            return json.load(f)
    except Exception as e:
        print(f"Error reading meta.json: {e}")
        return {}


def find_junit_files_from_meta(
    bundle_dir: Path, meta_data: Dict[str, Any]
) -> List[Path]:
    """Find junit files using the file_sets information from meta.json."""
    junit_files = []

    file_sets = meta_data.get("file_sets", [])

    for file_set in file_sets:
        file_set_type = file_set.get("file_set_type", "")
        if file_set_type.lower() == "junit":
            files = file_set.get("files", [])
            for file_info in files:
                path = file_info.get("path", "")
                if path:
                    junit_file_path = bundle_dir / path
                    if junit_file_path.exists():
                        junit_files.append(junit_file_path)
                    else:
                        print(
                            f"Warning: Junit file not found at path: {junit_file_path}"
                        )

    return sorted(junit_files)


def format_bindings_suite(suite: Any) -> Dict[str, Any]:
    """Convert a BindingsTestSuite to a dictionary for pretty printing."""
    try:
        suite_dict = {
            "name": getattr(suite, "name", None),
            "tests": getattr(suite, "tests", None),
            "failures": getattr(suite, "failures", None),
            "errors": getattr(suite, "errors", None),
            "skipped": getattr(suite, "skipped", None),
            "time": getattr(suite, "time", None),
            "timestamp": getattr(suite, "timestamp", None),
            "timestamp_micros": getattr(suite, "timestamp_micros", None),
            "test_cases": [],
        }

        # Process test cases
        test_cases = getattr(suite, "test_cases", [])
        for case in test_cases:
            case_dict = {
                "name": getattr(case, "name", None),
                "classname": getattr(case, "classname", None),
                "file": getattr(case, "file", None),
                "line": getattr(case, "line", None),
                "time": getattr(case, "time", None),
                "timestamp": getattr(case, "timestamp", None),
                "timestamp_micros": getattr(case, "timestamp_micros", None),
                "status": (
                    str(getattr(case, "status", None))
                    if hasattr(case, "status")
                    else None
                ),
                "id": getattr(case, "id", None),
                "parent_name": getattr(case, "parent_name", None),
            }
            suite_dict["test_cases"].append(case_dict)

        return suite_dict
    except Exception as e:
        return {"error": f"Failed to format suite: {e}", "raw": str(suite)}


def format_bindings_report(report: Any) -> Dict[str, Any]:
    """Convert a BindingsReport to a dictionary for pretty printing."""
    try:
        # Try to access the report attributes
        result = {
            "tests": getattr(report, "tests", None),
            "failures": getattr(report, "failures", None),
            "errors": getattr(report, "errors", None),
            "skipped": getattr(report, "skipped", None),
            "time": getattr(report, "time", None),
            "timestamp": getattr(report, "timestamp", None),
            "timestamp_micros": getattr(report, "timestamp_micros", None),
            "variant": getattr(report, "variant", None),
            "test_suites": [],
        }

        # Process test suites
        test_suites = getattr(report, "test_suites", [])
        for suite in test_suites:
            suite_dict = {
                "name": getattr(suite, "name", None),
                "tests": getattr(suite, "tests", None),
                "failures": getattr(suite, "failures", None),
                "errors": getattr(suite, "errors", None),
                "skipped": getattr(suite, "skipped", None),
                "time": getattr(suite, "time", None),
                "timestamp": getattr(suite, "timestamp", None),
                "timestamp_micros": getattr(suite, "timestamp_micros", None),
                "test_cases": [],
            }

            # Process test cases
            test_cases = getattr(suite, "test_cases", [])
            for case in test_cases:
                case_dict = {
                    "name": getattr(case, "name", None),
                    "classname": getattr(case, "classname", None),
                    "file": getattr(case, "file", None),
                    "line": getattr(case, "line", None),
                    "time": getattr(case, "time", None),
                    "timestamp": getattr(case, "timestamp", None),
                    "timestamp_micros": getattr(case, "timestamp_micros", None),
                    "status": (
                        str(getattr(case, "status", None))
                        if hasattr(case, "status")
                        else None
                    ),
                }
                suite_dict["test_cases"].append(case_dict)

            result["test_suites"].append(suite_dict)

        return result
    except Exception as e:
        return {"error": f"Failed to format report: {e}", "raw": str(report)}


def format_parse_result(parse_result: Any) -> Dict[str, Any]:
    """Convert a BindingsParseResult to a dictionary for pretty printing."""
    try:
        result = {"issues": []}

        # Process issues
        issues = getattr(parse_result, "issues", [])
        for issue in issues:
            issue_dict = {
                "level": (
                    str(getattr(issue, "level", None))
                    if hasattr(issue, "level")
                    else None
                ),
                "error_message": getattr(issue, "error_message", None),
            }
            result["issues"].append(issue_dict)

        # Process report
        report = getattr(parse_result, "report", None)
        if report is not None:
            result["report"] = format_bindings_report(report)
        else:
            result["report"] = None

        return result
    except Exception as e:
        return {
            "error": f"Failed to format parse result: {e}",
            "raw": str(parse_result),
        }


def extract_test_case_key(test_case: Dict[str, Any]) -> str:
    """Extract a unique key for a test case to enable order-agnostic comparison."""
    name = test_case.get("name", "")
    parent_name = test_case.get("parent_name", "")
    classname = test_case.get("classname", "")
    file = test_case.get("file", "")
    id = test_case.get("id", "")
    return id if id else f"{file}.{classname}.{name}.{parent_name}"


def extract_test_suite_key(test_suite: Dict[str, Any]) -> str:
    """Extract a unique key for a test suite to enable order-agnostic comparison."""
    name = test_suite.get("name", "")
    return name


def normalize_test_case(test_case: Dict[str, Any]) -> Dict[str, Any]:
    """Normalize a test case for comparison by removing order-dependent fields."""
    return {
        "name": test_case.get("name"),
        "classname": test_case.get("classname"),
        "file": test_case.get("file"),
        "line": test_case.get("line"),
        "time": test_case.get("time"),
        "status": test_case.get("status"),
    }


def normalize_test_suite(test_suite: Dict[str, Any]) -> Dict[str, Any]:
    """Normalize a test suite for comparison by removing order-dependent fields."""
    normalized_cases = {}
    test_cases = test_suite.get("test_cases", [])

    for case in test_cases:
        key = extract_test_case_key(case)
        normalized_cases[key] = normalize_test_case(case)

    return {
        "name": test_suite.get("name"),
        "tests": test_suite.get("tests"),
        "failures": test_suite.get("failures"),
        "errors": test_suite.get("errors"),
        "skipped": test_suite.get("skipped"),
        "time": test_suite.get("time"),
        "test_cases": normalized_cases,
    }


def safe_get_numeric(value: Any, default: float = 0.0) -> float:
    """Safely extract a numeric value, handling None and other types."""
    if value is None:
        return default
    try:
        return float(value)
    except (ValueError, TypeError):
        return default


def collapse_reports(reports: List[Any], source_type: str) -> Dict[str, Any]:
    """Collapse multiple reports into a single aggregated report."""
    if not reports:
        return {
            "tests": 0,
            "failures": 0,
            "errors": 0,
            "skipped": 0,
            "time": 0.0,
            "test_suites": [],
            "source_type": source_type,
            "report_count": 0,
        }

    # Aggregate totals
    total_tests = 0
    total_failures = 0
    total_errors = 0
    total_skipped = 0
    total_time = 0.0
    all_test_suites = []

    for i, report in enumerate(reports):
        # Aggregate counts
        total_tests += safe_get_numeric(getattr(report, "tests", None), 0)
        total_failures += safe_get_numeric(getattr(report, "failures", None), 0)
        total_errors += safe_get_numeric(getattr(report, "errors", None), 0)
        total_skipped += safe_get_numeric(getattr(report, "skipped", None), 0)
        total_time += safe_get_numeric(getattr(report, "time", None), 0.0)

        # Collect test suites
        test_suites = getattr(report, "test_suites", [])
        for suite in test_suites:
            formatted_suite = format_bindings_suite(suite)
            suite_with_source = {
                "source_report": f"{source_type}_report_{i}",
                "name": formatted_suite.get(
                    "name", ""
                ),  # Make sure name is directly accessible
                "suite": formatted_suite,
            }
            all_test_suites.append(suite_with_source)

    return {
        "tests": int(total_tests),
        "failures": int(total_failures),
        "errors": int(total_errors),
        "skipped": int(total_skipped),
        "time": total_time,
        "test_suites": all_test_suites,
        "source_type": source_type,
        "report_count": len(reports),
    }


def compare_collapsed_reports(
    junit_collapsed: Dict[str, Any], bin_collapsed: Dict[str, Any]
) -> Dict[str, Any]:
    """Compare two collapsed reports and highlight differences."""
    comparison = {
        "summary_comparison": {},
        "test_suite_comparison": {},
        "differences_found": False,
    }

    # Compare summary statistics
    summary_diffs = {}
    for field in ["tests", "failures", "errors", "skipped", "time"]:
        junit_val = junit_collapsed.get(field, 0)
        bin_val = bin_collapsed.get(field, 0)
        if junit_val != bin_val:
            summary_diffs[field] = {
                "junit": junit_val,
                "bin_parse": bin_val,
                "diff": junit_val - bin_val,
            }
            comparison["differences_found"] = True

    # Compare report counts
    junit_count = junit_collapsed.get("report_count", 0)
    bin_count = bin_collapsed.get("report_count", 0)
    # Note: report_count differences are expected and not meaningful since
    # junit files are individual XML files while internal.bin is a single binary file

    comparison["summary_comparison"] = {
        "junit": {
            "tests": junit_collapsed.get("tests", 0),
            "failures": junit_collapsed.get("failures", 0),
            "errors": junit_collapsed.get("errors", 0),
            "skipped": junit_collapsed.get("skipped", 0),
            "time": junit_collapsed.get("time", 0.0),
        },
        "bin_parse": {
            "tests": bin_collapsed.get("tests", 0),
            "failures": bin_collapsed.get("failures", 0),
            "errors": bin_collapsed.get("errors", 0),
            "skipped": bin_collapsed.get("skipped", 0),
            "time": bin_collapsed.get("time", 0.0),
        },
        "differences": summary_diffs,
    }

    # Compare test suites
    junit_suites = {}
    bin_suites = {}

    # Normalize test suites by name for comparison
    for suite_info in junit_collapsed.get("test_suites", []):
        # Use the directly accessible 'name' field
        suite_name = suite_info.get("name", "unnamed")
        suite = suite_info.get("suite", {})
        if suite_name not in junit_suites:
            junit_suites[suite_name] = []
        junit_suites[suite_name].append(suite)

    for suite_info in bin_collapsed.get("test_suites", []):
        # Use the directly accessible 'name' field
        suite_name = suite_info.get("name", "unnamed")
        suite = suite_info.get("suite", {})
        if suite_name not in bin_suites:
            bin_suites[suite_name] = []
        bin_suites[suite_name].append(suite)

    # Find differences in test suites
    all_suite_names = set(junit_suites.keys()) | set(bin_suites.keys())
    suite_differences = {}

    for suite_name in all_suite_names:
        junit_suite_list = junit_suites.get(suite_name, [])
        bin_suite_list = bin_suites.get(suite_name, [])

        if not junit_suite_list:
            # Get test cases from bin suites
            bin_test_cases = []
            for suite in bin_suite_list:
                test_cases = suite.get("test_cases", [])
                for case in test_cases:
                    case_key = extract_test_case_key(case)
                    bin_test_cases.append(case_key)

            suite_differences[suite_name] = {
                "status": "only_in_bin_parse",
                "bin_parse_count": len(bin_suite_list),
                "test_cases_in_bin": bin_test_cases,
            }
            comparison["differences_found"] = True
        elif not bin_suite_list:
            # Get test cases from junit suites
            junit_test_cases = []
            for suite in junit_suite_list:
                test_cases = suite.get("test_cases", [])
                for case in test_cases:
                    case_key = extract_test_case_key(case)
                    junit_test_cases.append(case_key)

            suite_differences[suite_name] = {
                "status": "only_in_junit",
                "junit_count": len(junit_suite_list),
                "test_cases_in_junit": junit_test_cases,
            }
            comparison["differences_found"] = True
        else:
            # Compare suite statistics
            suite_diff = {}

            # Aggregate statistics for this suite name
            junit_tests = sum(
                safe_get_numeric(suite.get("tests", 0), 0) for suite in junit_suite_list
            )
            bin_tests = sum(
                safe_get_numeric(suite.get("tests", 0), 0) for suite in bin_suite_list
            )

            junit_failures = sum(
                safe_get_numeric(suite.get("failures", 0), 0)
                for suite in junit_suite_list
            )
            bin_failures = sum(
                safe_get_numeric(suite.get("failures", 0), 0)
                for suite in bin_suite_list
            )

            junit_errors = sum(
                safe_get_numeric(suite.get("errors", 0), 0)
                for suite in junit_suite_list
            )
            bin_errors = sum(
                safe_get_numeric(suite.get("errors", 0), 0) for suite in bin_suite_list
            )

            junit_skipped = sum(
                safe_get_numeric(suite.get("skipped", 0), 0)
                for suite in junit_suite_list
            )
            bin_skipped = sum(
                safe_get_numeric(suite.get("skipped", 0), 0) for suite in bin_suite_list
            )

            junit_time = sum(
                safe_get_numeric(suite.get("time", 0), 0.0)
                for suite in junit_suite_list
            )
            bin_time = sum(
                safe_get_numeric(suite.get("time", 0), 0.0) for suite in bin_suite_list
            )

            # Check for differences
            if junit_tests != bin_tests:
                suite_diff["tests"] = {
                    "junit": junit_tests,
                    "bin_parse": bin_tests,
                    "diff": junit_tests - bin_tests,
                }
                comparison["differences_found"] = True

            if junit_failures != bin_failures:
                suite_diff["failures"] = {
                    "junit": junit_failures,
                    "bin_parse": bin_failures,
                    "diff": junit_failures - bin_failures,
                }
                comparison["differences_found"] = True

            if junit_errors != bin_errors:
                suite_diff["errors"] = {
                    "junit": junit_errors,
                    "bin_parse": bin_errors,
                    "diff": junit_errors - bin_errors,
                }
                comparison["differences_found"] = True

            if junit_skipped != bin_skipped:
                suite_diff["skipped"] = {
                    "junit": junit_skipped,
                    "bin_parse": bin_skipped,
                    "diff": junit_skipped - bin_skipped,
                }
                comparison["differences_found"] = True

            if junit_time != bin_time:
                suite_diff["time"] = {
                    "junit": junit_time,
                    "bin_parse": bin_time,
                    "diff": junit_time - bin_time,
                }
                comparison["differences_found"] = True

            # Compare suite counts
            if len(junit_suite_list) != len(bin_suite_list):
                suite_diff["suite_count"] = {
                    "junit": len(junit_suite_list),
                    "bin_parse": len(bin_suite_list),
                    "diff": len(junit_suite_list) - len(bin_suite_list),
                }
                comparison["differences_found"] = True

            # Compare test cases within the suite
            junit_cases = {}
            bin_cases = {}

            # Collect all test cases from all suites with this name
            for suite in junit_suite_list:
                test_cases = suite.get("test_cases", [])
                for case in test_cases:
                    case_key = extract_test_case_key(case)
                    if case_key not in junit_cases:
                        junit_cases[case_key] = []
                    junit_cases[case_key].append(case)

            for suite in bin_suite_list:
                test_cases = suite.get("test_cases", [])
                for case in test_cases:
                    case_key = extract_test_case_key(case)
                    if case_key not in bin_cases:
                        bin_cases[case_key] = []
                    bin_cases[case_key].append(case)

            # Find test case differences
            all_case_keys = set(junit_cases.keys()) | set(bin_cases.keys())
            case_differences = {}

            for case_key in all_case_keys:
                junit_case_list = junit_cases.get(case_key, [])
                bin_case_list = bin_cases.get(case_key, [])

                if not junit_case_list:
                    case_differences[case_key] = {
                        "status": "only_in_bin_parse",
                        "bin_parse_count": len(bin_case_list),
                    }
                    comparison["differences_found"] = True
                elif not bin_case_list:
                    case_differences[case_key] = {
                        "status": "only_in_junit",
                        "junit_count": len(junit_case_list),
                    }
                    comparison["differences_found"] = True
                else:
                    # Compare case details (time, status, etc.)
                    case_diff = {}

                    # Aggregate case statistics
                    junit_time = sum(
                        safe_get_numeric(case.get("time", 0), 0.0)
                        for case in junit_case_list
                    )
                    bin_time = sum(
                        safe_get_numeric(case.get("time", 0), 0.0)
                        for case in bin_case_list
                    )

                    if junit_time != bin_time:
                        case_diff["time"] = {
                            "junit": junit_time,
                            "bin_parse": bin_time,
                            "diff": junit_time - bin_time,
                        }
                        comparison["differences_found"] = True

                    # TODO: Compare status (if available)
                    # # Compare status (if available)
                    # junit_statuses = [case.get('status') for case in junit_case_list if case.get('status')]
                    # bin_statuses = [case.get('status') for case in bin_case_list if case.get('status')]

                    # if junit_statuses != bin_statuses:
                    #     case_diff['status'] = {'junit': junit_statuses, 'bin_parse': bin_statuses}
                    #     comparison['differences_found'] = True

                    # Compare case counts
                    if len(junit_case_list) != len(bin_case_list):
                        case_diff["case_count"] = {
                            "junit": len(junit_case_list),
                            "bin_parse": len(bin_case_list),
                            "diff": len(junit_case_list) - len(bin_case_list),
                        }
                        comparison["differences_found"] = True

                    if case_diff:
                        case_differences[case_key] = case_diff

            if case_differences:
                suite_diff["test_cases"] = case_differences

            if suite_diff:
                suite_differences[suite_name] = suite_diff

    comparison["test_suite_comparison"] = {
        "junit_suites": len(junit_suites),
        "bin_parse_suites": len(bin_suites),
        "differences": suite_differences,
    }

    return comparison


def process_bundle_directory(bundle_dir: Path) -> None:
    """Process the bundle directory and apply parsing functions."""
    print(f"Processing bundle directory: {bundle_dir}")
    print("=" * 60)

    # Read meta.json to understand the bundle structure
    print("\n0. Reading meta.json...")
    print("-" * 40)
    meta_data = read_meta_json(bundle_dir)

    if meta_data:
        print("Successfully loaded meta.json")
        bundle_upload_id = meta_data.get("bundle_upload_id", "Unknown")
        print(f"Bundle Upload ID: {bundle_upload_id}")

        # Show file sets information
        file_sets = meta_data.get("file_sets", [])
        print(f"Found {len(file_sets)} file sets:")
        for i, file_set in enumerate(file_sets):
            file_set_type = file_set.get("file_set_type", "Unknown")
            files = file_set.get("files", [])
            print(f"  {i + 1}. {file_set_type}: {len(files)} files")
    else:
        print("Warning: Could not load meta.json, falling back to directory scanning")

    # Check if internal.bin exists
    internal_bin_path = bundle_dir / "internal.bin"
    bin_reports = []

    if internal_bin_path.exists():
        print(f"\n1. Processing internal.bin file: {internal_bin_path}")
        print("-" * 40)

        try:
            internal_bin_data = read_binary_file(internal_bin_path)
            print(f"File size: {len(internal_bin_data)} bytes")

            # Apply bin_parse
            print("\nApplying bin_parse...")
            bin_reports = bin_parse(internal_bin_data)

            print(f"\nbin_parse results ({len(bin_reports)} reports)")
            for i, report in enumerate(bin_reports):
                # print(f"\nReport {i + 1}:")
                formatted_report = format_bindings_report(report)
                # print(json.dumps(formatted_report, indent=2, default=str))

        except Exception as e:
            print(f"Error processing internal.bin: {e}")
    else:
        print(f"\nWarning: internal.bin not found in {bundle_dir}")

    # Process junit files using meta.json information
    print(f"\n2. Processing junit files...")
    print("-" * 40)

    if meta_data:
        junit_files = find_junit_files_from_meta(bundle_dir, meta_data)
    else:
        # Fallback to directory scanning if meta.json is not available
        junit_dir = bundle_dir / "junit"
        junit_files = []
        if junit_dir.exists():
            # Simple fallback - look for XML files in junit directory
            junit_files = list(junit_dir.glob("*.xml"))

    if not junit_files:
        print("No junit files found")
        return

    print(f"Found {len(junit_files)} junit files:")
    for file_path in junit_files:
        print(f"  - {file_path.relative_to(bundle_dir)}")

    # Process all junit files and collect results
    all_junit_results = []
    total_issues = []

    for i, junit_file in enumerate(junit_files):
        print(
            f"\nProcessing junit file {i + 1}/{len(junit_files)}: {junit_file.relative_to(bundle_dir)}"
        )

        try:
            junit_data = read_text_file(junit_file)
            print(f"File size: {len(junit_data)} bytes")

            # Apply junit_parse
            parse_result = junit_parse(junit_data)

            # Collect the results
            file_result = {
                "file_path": str(junit_file.relative_to(bundle_dir)),
                "file_size": len(junit_data),
                "parse_result": parse_result,
            }
            all_junit_results.append(file_result)

            # Collect issues
            if hasattr(parse_result, "issues"):
                for issue in parse_result.issues:
                    issue_with_file = {
                        "file": str(junit_file.relative_to(bundle_dir)),
                        "level": (
                            str(getattr(issue, "level", None))
                            if hasattr(issue, "level")
                            else None
                        ),
                        "error_message": getattr(issue, "error_message", None),
                    }
                    total_issues.append(issue_with_file)

            print("✓ Processed successfully")

        except Exception as e:
            print(f"✗ Error processing {junit_file.name}: {e}")
            # Still add to results with error info
            all_junit_results.append(
                {"file_path": str(junit_file.relative_to(bundle_dir)), "error": str(e)}
            )

    # Print collapsed junit results
    print(f"\n" + "=" * 60)
    print("COLLAPSED JUNIT RESULTS")
    print("=" * 60)

    # Summary statistics
    successful_files = [r for r in all_junit_results if "error" not in r]
    failed_files = [r for r in all_junit_results if "error" in r]

    print(f"\nSummary:")
    print(f"  Total files processed: {len(all_junit_results)}")
    print(f"  Successful: {len(successful_files)}")
    print(f"  Failed: {len(failed_files)}")
    print(f"  Total issues found: {len(total_issues)}")

    # Show failed files if any
    if failed_files:
        print(f"\nFailed files:")
        for failed in failed_files:
            print(f"  ✗ {failed['file_path']}: {failed['error']}")

    # Show issues if any
    if total_issues:
        print(f"\nIssues found:")
        for issue in total_issues:
            print(f"  - {issue['file']}: [{issue['level']}] {issue['error_message']}")

    # Collapse all successful reports into a single summary
    junit_summary = None
    if successful_files:
        # print(f"\nCollapsed Report Summary:")
        # print("-" * 40)

        # Aggregate statistics
        total_tests = 0
        total_failures = 0
        total_errors = 0
        total_skipped = 0
        total_time = 0.0
        all_test_suites = []

        for file_result in successful_files:
            parse_result = file_result["parse_result"]
            if hasattr(parse_result, "report") and parse_result.report is not None:
                report = parse_result.report

                # Aggregate counts with safe numeric handling
                total_tests += safe_get_numeric(getattr(report, "tests", None), 0)
                total_failures += safe_get_numeric(getattr(report, "failures", None), 0)
                total_errors += safe_get_numeric(getattr(report, "errors", None), 0)
                total_skipped += safe_get_numeric(getattr(report, "skipped", None), 0)
                total_time += safe_get_numeric(getattr(report, "time", None), 0.0)

                # Collect test suites with file info
                test_suites = getattr(report, "test_suites", [])
                for suite in test_suites:
                    formatted_suite = format_bindings_suite(suite)
                    suite_with_file = {
                        "source_file": file_result["file_path"],
                        "suite_name": getattr(suite, "name", None),
                        "name": formatted_suite.get(
                            "name", ""
                        ),  # Make sure name is directly accessible
                        "suite": formatted_suite,
                        "test_cases": getattr(suite, "test_cases", []),
                    }
                    all_test_suites.append(suite_with_file)

        # Create aggregated summary
        junit_summary = {
            "total_tests": total_tests,
            "total_failures": total_failures,
            "total_errors": total_errors,
            "total_skipped": total_skipped,
            "total_time": total_time,
            "test_suites_count": len(all_test_suites),
            "test_suites": all_test_suites,
        }

        # print(json.dumps(junit_summary, indent=2, default=str))

    # Generate comparison report
    if bin_reports and all_junit_results:
        print(f"\n" + "=" * 60)
        print("COMPARISON REPORT")
        print("=" * 60)

        # Extract junit reports from successful files
        junit_reports = []
        for file_result in all_junit_results:
            if "error" not in file_result:
                parse_result = file_result["parse_result"]
                if hasattr(parse_result, "report") and parse_result.report is not None:
                    junit_reports.append(parse_result.report)

        # Collapse both sets of reports
        print("\nCollapsing reports for comparison...")
        junit_collapsed = collapse_reports(junit_reports, "junit")
        bin_collapsed = collapse_reports(bin_reports, "bin_parse")

        print(
            f"Junit collapsed: {junit_collapsed['report_count']} reports -> {junit_collapsed['tests']} tests"
        )
        print(
            f"Bin collapsed: {bin_collapsed['report_count']} reports -> {bin_collapsed['tests']} tests"
        )

        # Compare the collapsed reports
        comparison = compare_collapsed_reports(junit_collapsed, bin_collapsed)

        if not comparison["differences_found"]:
            print("\n✅ PERFECT MATCH!")
            print("The junit files and internal.bin contain identical test data.")
        else:
            print("\n⚠️  DIFFERENCES FOUND!")
            print(
                "The following differences were detected between junit files and internal.bin:"
            )

            # Summary differences
            summary_diffs = comparison["summary_comparison"].get("differences", {})
            if summary_diffs:
                print(f"\n📊 Summary Statistics Differences:")
                for field, diff in summary_diffs.items():
                    print(f"  {field}:")
                    print(f"    Junit: {diff['junit']}")
                    print(f"    Bin:   {diff['bin_parse']}")
                    print(f"    Diff:  {diff['diff']:+}")

            # Test suite differences
            suite_diffs = comparison["test_suite_comparison"].get("differences", {})
            if suite_diffs:
                print(f"\n🧪 Test Suite Differences:")
                for suite_name, diff in suite_diffs.items():
                    print(f"\n  Suite: {suite_name}")

                    if "status" in diff:
                        if diff["status"] == "only_in_junit":
                            print(
                                f"    Status: Only found in junit files ({diff['junit_count']} instances)"
                            )
                            # Show test cases in this suite
                            if "test_cases_in_junit" in diff:
                                test_cases = diff["test_cases_in_junit"]
                                print(
                                    f"    Test cases in this suite ({len(test_cases)} cases):"
                                )
                                for case in test_cases:
                                    print(f"      - {case}")
                        elif diff["status"] == "only_in_bin_parse":
                            print(
                                f"    Status: Only found in internal.bin ({diff['bin_parse_count']} instances)"
                            )
                            # Show test cases in this suite
                            if "test_cases_in_bin" in diff:
                                test_cases = diff["test_cases_in_bin"]
                                print(
                                    f"    Test cases in this suite ({len(test_cases)} cases):"
                                )
                                for case in test_cases:
                                    print(f"      - {case}")
                    else:
                        # Compare suite statistics
                        for field, field_diff in diff.items():
                            if field == "test_cases":
                                # Group test cases by their status
                                only_in_junit = []
                                only_in_bin = []
                                different_values = []

                                for case_name, case_diff in field_diff.items():
                                    # Check if this case has a 'status' field indicating it's missing
                                    if (
                                        isinstance(case_diff, dict)
                                        and "status" in case_diff
                                    ):
                                        if case_diff["status"] == "only_in_junit":
                                            only_in_junit.append(
                                                (
                                                    case_name,
                                                    case_diff.get("junit_count", 1),
                                                )
                                            )
                                        elif case_diff["status"] == "only_in_bin_parse":
                                            only_in_bin.append(
                                                (
                                                    case_name,
                                                    case_diff.get("bin_parse_count", 1),
                                                )
                                            )
                                    elif isinstance(case_diff, dict):
                                        # This case exists in both but has different values
                                        different_values.append((case_name, case_diff))

                                # Show missing test cases
                                if only_in_junit:
                                    print(
                                        f"    ❌ Test Cases ONLY in Junit ({len(only_in_junit)} cases):"
                                    )
                                    for case_name, count in only_in_junit:
                                        print(f"       - {case_name} (count: {count})")

                                if only_in_bin:
                                    print(
                                        f"    ❌ Test Cases ONLY in Bin ({len(only_in_bin)} cases):"
                                    )
                                    for case_name, count in only_in_bin:
                                        print(f"       - {case_name} (count: {count})")

                                # Show test cases with different values
                                if different_values:
                                    print(
                                        f"    ⚠️  Test Cases with Different Values ({len(different_values)} cases):"
                                    )
                                    for case_name, case_diff in different_values:
                                        print(f"       {case_name}:")
                                        for (
                                            case_field,
                                            case_field_diff,
                                        ) in case_diff.items():
                                            if case_field == "status":
                                                print(f"         {case_field}:")
                                                print(
                                                    f"           Junit: {case_field_diff['junit']}"
                                                )
                                                print(
                                                    f"           Bin:   {case_field_diff['bin_parse']}"
                                                )
                                            else:
                                                print(f"         {case_field}:")
                                                print(
                                                    f"           Junit: {case_field_diff['junit']}"
                                                )
                                                print(
                                                    f"           Bin:   {case_field_diff['bin_parse']}"
                                                )
                                                if "diff" in case_field_diff:
                                                    print(
                                                        f"           Diff:  {case_field_diff['diff']:+}"
                                                    )
                            else:
                                print(f"    {field}:")
                                print(f"      Junit: {field_diff['junit']}")
                                print(f"      Bin:   {field_diff['bin_parse']}")
                                print(f"      Diff:  {field_diff['diff']:+}")

                        # Additional note if there are no test case differences but suite stats differ
                        if "test_cases" not in diff:
                            print(
                                f"        ℹ️ Note: Test cases match perfectly, but suite-level statistics differ"
                            )
                            print(
                                f"        This suggests the suite metadata (counts/times) don't match the actual test cases"
                            )
        # print(f"\n" + "=" * 60)
        # print("DETAILED COMPARISON DATA")
        # print("=" * 60)
        # print(json.dumps(comparison, indent=2, default=str))


def main():
    """Main function to handle command line arguments and process the bundle."""
    parser = argparse.ArgumentParser(
        description="Process bundle files using bin_parse and junit_parse functions"
    )
    parser.add_argument(
        "bundle_dir",
        type=Path,
        help="Path to the directory containing the unzipped bundle",
    )

    args = parser.parse_args()

    bundle_dir = args.bundle_dir.resolve()

    if not bundle_dir.exists():
        print(f"Error: Directory does not exist: {bundle_dir}")
        sys.exit(1)

    if not bundle_dir.is_dir():
        print(f"Error: Path is not a directory: {bundle_dir}")
        sys.exit(1)

    try:
        process_bundle_directory(bundle_dir)
    except KeyboardInterrupt:
        print("\nOperation cancelled by user")
        sys.exit(1)
    except Exception as e:
        print(f"Unexpected error: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()
