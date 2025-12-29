# Bazel Build Event Protocol (BEP) Parser

This module parses Bazel Build Event Protocol (BEP) files to extract test execution information for identifying test states. The BEP is a stream of events that Bazel emits during a build, providing detailed information about targets, test execution, and build artifacts.

## Overview

The BEP parser extracts information from two main event types:

- **TestResult events**: Individual test execution results
- **TestSummary events**: Aggregated test execution summaries

The parser supports both JSON and binary proto formats, with automatic fallback from JSON to binary parsing if JSON parsing fails.

## Information Extracted from BEP

### 1. Target Information (Labels)

**Source**: `TestResult` and `TestSummary` event IDs

Each test target in Bazel has a unique label (e.g., `//trunk/hello_world/cc:hello_test`). The parser extracts these labels to:

- Group test results by target
- Associate JUnit XML files with their corresponding targets
- Track which targets were executed
- Use for Codeowners associations

### 2. JUnit XML File Paths

**Source**: `TestResult.test_action_output` fields

The parser extracts paths to JUnit XML test result files. These paths can be:

- **Local file paths**: Absolute paths to XML files on the filesystem

  - Example: `/tmp/hello_test/test.xml`
  - Example: `/tmp/hello_test/test_attempts/attempt_1.xml` (for retries)

- **Remote bytestream URIs**: References to files stored in remote caches
  - Example: `bytestream://build.example.io/blobs/1234/567`

**Multiple files per target**: A single target may produce multiple XML files in cases of:

- Test retries (each attempt generates its own XML)
- Flaky test detection (multiple attempts with different outcomes)

**File filtering**: Only files with `.xml` extension are extracted from the `test_action_output` list.

### 3. Cache State

**Source**: `TestResult.execution_info` and `TestResult.cached_locally`

The parser determines whether a test result was cached:

- **`cached_locally`**: Test result was retrieved from local cache
- **`cached_remotely`**: Test result was retrieved from remote cache (via `execution_info.cached_remotely`)

**Usage**:

- Cached test results are typically excluded from analytics (via `uncached_xml_files()` and `uncached_labels()`)
- Cache state helps distinguish between actual test executions and cached results
- XML file counts distinguish between total files and cached files

### 4. Test Status (Build Status)

**Latest status**: For tests with multiple attempts (retries), the parser tracks the status from the latest attempt.

### 5. Test Runner Report

**Source**: `TestSummary` events

The parser extracts aggregated test execution information from TestSummary events:

- **Overall status**: `overall_status` field (Passed, Failed, or Flaky)
- **Start time**: `first_start_time` - when the first test attempt started
- **End time**: `last_stop_time` - when the last test attempt completed
- **Label**: The target label associated with the summary

**Usage**: Test runner reports provide high-level timing and status information that complements the detailed JUnit XML parsing. They are used for validation and to provide context when parsing JUnit files.

### 6. Attempt Numbers

**Source**: `TestResult` event ID (`TestResult.id.attempt`)

For tests that are retried (e.g., flaky tests), each attempt is tracked with an attempt number:

- Attempt numbers start at 1
- Each retry increments the attempt number
- Multiple XML files from the same target can have different attempt numbers

**Usage**:

- Attempt numbers are attached to individual test case runs to track which attempt they came from
- Helps distinguish between different execution attempts of the same test
- Used to merge results from multiple attempts into a single test result
