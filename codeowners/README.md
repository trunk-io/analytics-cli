# codeowners

A Rust library and CLI tool for parsing and querying CODEOWNERS files from both GitHub and GitLab.

## Overview

This crate provides functionality to parse CODEOWNERS files in both GitHub and GitLab formats, allowing you to determine code owners for specific file paths. It was created because there wasn't an existing crate that solved all of our needs, particularly the requirement to support both GitHub and GitLab CODEOWNERS syntaxes with their respective differences.

## GitHub and GitLab Support

This crate supports both GitHub and GitLab CODEOWNERS file formats:

### GitHub Support

The GitHub parser supports the standard GitHub CODEOWNERS format, which includes:

- Pattern matching using glob syntax
- Owner specifications in the form of `@username`, `@org/team`, or email addresses
- Comments (lines starting with `#`)
- Pattern precedence rules (last matching pattern wins)

### GitLab Support

The GitLab parser supports GitLab's CODEOWNERS format, which is a superset of GitHub's format and includes additional features:

- All GitHub syntax features
- **Sections**: GitLab allows organizing CODEOWNERS rules into named sections (e.g., `[Documentation]`, `[Database]`)
- Section-level default owners
- More flexible path matching rules
- Support for escaped characters (e.g., `\#` for files with `#` in the name)

The parser automatically detects and handles these GitLab-specific features while maintaining compatibility with GitHub-style CODEOWNERS files.

## Binary: `check-codeowners`

The crate includes a binary named `check-codeowners` that allows you to run a CODEOWNERS check on a specific path. This is useful for validating ownership rules or determining who owns a particular file or directory.

### Building the Binary

To build the binary, use Cargo:

```bash
cargo build --release --bin check-codeowners
```

The binary will be available at `target/release/check-codeowners`.

### Usage

The `check-codeowners` binary takes three required arguments:

- `--codeowners-type`: Specifies how to parse the CODEOWNERS file. Must be either `github` or `gitlab`.
- `--codeowners-path`: Path to the CODEOWNERS file to parse.
- `--test-case-path`: The file or directory path to check against the CODEOWNERS rules.

### Example

```bash
check-codeowners \
  --codeowners-type github \
  --codeowners-path .github/CODEOWNERS \
  --test-case-path src/main.rs
```

This will:

1. Parse the CODEOWNERS file at `.github/CODEOWNERS` using GitHub format rules
2. Check which owners match the path `src/main.rs`
3. Print the list of owners found for that path
4. Exit with code 1 if no owners are found, or 0 if owners are found

### Output

If owners are found, the binary prints:

```bash
Owners found for src/main.rs:
@team-frontend
@user-example
```

If no owners are found, it prints an error message and exits with code 1

### Known Footguns

- Finding CODEOWNERS
  - The CODEOWNERS file found by the CLI is relative to the claimed repo root, which often ends up being wherever the CLI was invoked. If you are seeing issues with the file not being found then it is most likely caused by not running the CLI in the root directory of the repo. We should really fix this, but it hasn't been a big of an issue to deal with.
- Bad parsing
  - The glob parsing we do isn't always 1:1 with how GitHub supports CODEOWNERS parsing. We've added various edge case handlers to deal with this. It isn't an active issue, but something you should keep in mind.
