# context-js

JavaScript/TypeScript bindings for the analytics CLI. These bindings allow you to use the CLI's parsing and validation functions from Node.js or browser environments via WebAssembly. The bindings are primarily used by internal processes and systems and should not be depended upon externally. They are unstable and can change at any point.

## Overview

This package provides JavaScript/TypeScript bindings to the analytics CLI's core functionality, including:

- `bin_parse`: Parse binary test data files (`internal.bin`)
- `junit_parse`: Parse JUnit XML test result files
- `junit_validate`: Validate JUnit test reports
- `env_parse`: Parse CI environment variables
- `env_validate`: Validate CI environment information
- `repo_validate`: Validate repository information
- `parse_meta_from_tarball`: Parse metadata from compressed bundle tarballs
- `parse_internal_bin_from_tarball`: Parse test data from compressed bundle tarballs
- Various other validation and parsing utilities

## Setup

### Prerequisites

- Node.js (v18 or later recommended)
- `pnpm` (package manager)
- Rust and Cargo (for building WebAssembly bindings)
- `wasm-pack` (installed automatically via devDependencies)
- clang ([install](https://apt.llvm.org/))

### Installation

Install dependencies:

```bash
pnpm install
```

Build the WebAssembly bindings:

```bash
pnpm run build
```

This will compile the Rust code to WebAssembly and generate the bindings in the `pkg/` directory.

## Usage

### Importing the Module

```typescript
import { bin_parse, junit_parse, env_parse } from "./pkg/context_js";
```

### Example: Parsing Binary Test Data

```typescript
import { bin_parse } from "./pkg/context_js";
import { readFileSync } from "fs";

const binData = readFileSync("path/to/internal.bin");
const reports = bin_parse(binData);
console.log(`Parsed ${reports.length} test reports`);
```

### Example: Parsing JUnit XML

```typescript
import { junit_parse } from "./pkg/context_js";
import { readFileSync } from "fs";

const xmlData = readFileSync("path/to/junit.xml");
const result = junit_parse(xmlData);
if (result.report) {
  console.log(
    `Tests: ${result.report.tests}, Failures: ${result.report.failures}`,
  );
}
if (result.issues.length > 0) {
  console.log(`Found ${result.issues.length} parsing issues`);
}
```

### Example: Parsing from Tarball Stream

```typescript
import { parse_meta_from_tarball } from "./pkg/context_js";
import { createReadStream } from "fs";

const stream = createReadStream("path/to/bundle.tar.gz");
const bundle = await parse_meta_from_tarball(stream);
console.log(`Bundle version: ${bundle.version}`);
```

## Building / Testing

All build and test commands are self-contained within `package.json`:

### Build

Build the WebAssembly bindings:

```bash
pnpm run build
```

### Test

Run the test suite with coverage:

```bash
pnpm run test
```

### Build and Test

Build the bindings and run tests in one command:

```bash
pnpm run build_and_test
```

## Development

After making changes to the Rust code in `src/lib.rs`, rebuild the bindings:

```bash
pnpm run build
```

The generated bindings will be in the `pkg/` directory and can be imported by your TypeScript/JavaScript code.

## Publishing

The bindings are automatically published to S3 when changes are merged to the `main` branch. A GitHub Actions workflow runs after each pull request merge that:

1. Builds the WebAssembly bindings
2. Packages them into a tarball (`context-js-0.1.0.tgz`)
3. Uploads the package to S3

The uploaded package is stored at a path that includes the date and commit SHA in the format `date/sha`. For example, if a build runs on `2024-01-15` with commit SHA `abc1234`, the package will be available at:

```bash
s3://<bucket>/2024-01-15/abc1234/context-js-0.1.0.tgz
```

When using the published bindings, reference them using the `date/sha` format (e.g., `2024-01-15/abc1234`) to ensure you're using the correct version. This reference is displayed in the GitHub Actions workflow summary for each build.
