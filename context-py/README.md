# context-py

Python bindings for the analytics CLI. These bindings allow you to use the CLI's parsing and validation functions from Python. The bindings are primarily used by internal processes and systems and should not be depended upon externally. They are unstable and can change at any point.

## Overview

This package provides Python bindings to the analytics CLI's core functionality, including:

- `bin_parse`: Parse binary test data files (`internal.bin`)
- `junit_parse`: Parse JUnit XML test result files
- `junit_validate`: Validate JUnit test reports
- `env_parse`: Parse CI environment variables
- `env_validate`: Validate CI environment information
- `repo_validate`: Validate repository information
- `parse_meta`: Parse metadata from bytes
- `parse_meta_from_tarball`: Parse metadata from compressed bundle tarballs
- `parse_internal_bin_from_tarball`: Parse test data from compressed bundle tarballs
- `meta_validate`: Validate metadata
- `codeowners_parse`: Parse CODEOWNERS files
- `associate_codeowners_multithreaded`: Associate code owners with files
- Various other validation and parsing utilities

## Setup

### Prerequisites

- Python 3.x
- Rust and Cargo
- `uv` (Python package manager)
- `maturin` (for building Python extensions)

### Installation

1. Setup virtual environment:

```bash
uv venv
source .venv/bin/activate
```

2. Install dependencies:

```bash
uv pip install -r ./requirements-dev.txt
```

3. Build type stubs:

```bash
cargo run
```

4. Build bindings and install them into the virtual environment:

```bash
maturin dev
```

## Usage

### Importing the Module

```python
from context_py import bin_parse, junit_parse, env_parse
```

### Example: Parsing Binary Test Data

```python
from context_py import bin_parse

with open('path/to/internal.bin', 'rb') as f:
    bin_data = f.read()

reports = bin_parse(bin_data)
print(f"Parsed {len(reports)} test reports")
```

### Example: Parsing JUnit XML

```python
from context_py import junit_parse

with open('path/to/junit.xml', 'rb') as f:
    xml_data = f.read()

result = junit_parse(xml_data)
if result.report:
    print(f"Tests: {result.report.tests}, Failures: {result.report.failures}")
if result.issues:
    print(f"Found {len(result.issues)} parsing issues")
```

### Example: Parsing from Tarball

```python
from context_py import parse_meta_from_tarball

with open('path/to/bundle.tar.gz', 'rb') as f:
    bundle = parse_meta_from_tarball(f)
    print(f"Bundle version: {bundle.version}")
```

### Example: Parsing CI Environment Variables

```python
from context_py import env_parse

env_vars = {
    "CI": "true",
    "GITHUB_ACTIONS": "true",
    "GITHUB_REPOSITORY": "owner/repo",
    # ... other environment variables
}
stable_branches = ["main", "master"]

ci_info = env_parse(env_vars, stable_branches)
if ci_info:
    print(f"CI Platform: {ci_info.platform}")
    print(f"Branch: {ci_info.branch}")
```

## Developing / Testing

### Building After Code Changes

After editing Rust code, rebuild type stubs and bindings:

```bash
cargo run
maturin dev
```

### Running Tests

Run the test suite:

```bash
pytest
```
