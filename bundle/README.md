# bundle

A Rust library and CLI tool for working with bundle files, including parsing and inspecting the contents of `internal.bin` files.

## Overview

This crate provides functionality to work with bundle files, which are compressed tarballs containing test context data. The primary use case is to inspect and debug the contents of `internal.bin` files, which contain protobuf-encoded test report data.

## Binary: `proto-bin-to-json`

The crate includes a binary named `proto-bin-to-json` that allows you to quickly see what is stored within `internal.bin` files by converting the protobuf-encoded binary data into human-readable JSON format.

### Obtaining `internal.bin`

To inspect the contents of `internal.bin`, you first need to obtain the file. The easiest way is to use the `--dry-run` flag when running upload commands, which will save the bundle locally instead of uploading it to the server.

#### Using the CLI

When running the upload command with the `--dry-run` flag:

```bash
trunk upload --dry-run --junit-path test-results.xml
```

Alternatively, you can use the `TRUNK_DRY_RUN` environment variable:

```bash
TRUNK_DRY_RUN=true trunk upload --junit-path test-results.xml
```

This will create a `bundle_upload` directory in your current working directory. The `internal.bin` file will be located at the top level of this directory:

```bash
bundle_upload/
  ├── internal.bin
  └── meta.json
```

#### Using the Ruby Gem

For the Ruby gem, you can use the `TRUNK_DRY_RUN` environment variable:

```bash
TRUNK_DRY_RUN=true bundle exec rspec
```

This will also create a `bundle_upload` directory with `internal.bin` at the top level, allowing you to directly inspect the contents of the upload.

### Building the Binary

To build the binary, use Cargo with the `build-binary` feature:

```bash
cargo build --release --bin proto-bin-to-json --features build-binary
```

The binary will be available at `target/release/proto-bin-to-json`.

### Usage

The `proto-bin-to-json` binary takes one required argument:

- `proto_bin_file`: Path to the protobuf bin file to convert (e.g., `internal.bin`)

### Example

After obtaining `internal.bin` using the `--dry-run` flag (see above), you can inspect it:

```bash
proto-bin-to-json bundle_upload/internal.bin
```

This will:

1. Read the `internal.bin` file
2. Parse the protobuf-encoded test report data
3. Convert it to pretty-printed JSON
4. Print the JSON to stdout

### Output

The binary outputs the parsed test report data as formatted JSON, making it easy to inspect the contents of `internal.bin` files for debugging purposes.
