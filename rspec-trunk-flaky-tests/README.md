# rspec-trunk-flaky-tests

RSpec plugin for Trunk Flaky Tests. This gem automatically uploads test results to detect and quarantine flaky tests. It integrates with RSpec to provide automatic flaky test detection, quarantining, and analytics. You can find the uploaded gem in [rubygems](https://rubygems.org/gems/rspec_trunk_flaky_tests).

## Overview

This gem provides an RSpec plugin that:

- Automatically uploads test results from your CI jobs
- Enables accurate flaky test detection
- Automatically quarantines flaky tests
- Provides analytics on test stability

The gem includes a native Rust extension (`rspec_trunk_flaky_tests`) that provides core parsing and validation functionality.

## Prerequisites

- Ruby 3.0 or later
- Bundler
- Rust and Cargo (for building the native extension)
- `rb-sys` gem (installed automatically via dependencies)

## Setup

### Installation

Install dependencies:

```bash
bundle install
```

## Building

### Build the Native Extension

Build the Rust extension:

```bash
bundle exec rake compile
```

This will compile the Rust code and generate the native extension in `lib/rspec_trunk_flaky_tests/`.

### Build for Release

Build the gem package:

```bash
bundle exec rake build
```

The gem will be built and available in `pkg/`.

### Cross-Platform Building

Build the native extension for a specific platform:

```bash
bundle exec rake native[x86_64-linux]
```

Supported platforms:

- `x86_64-linux`
- `aarch64-linux`
- `arm64-darwin`
- `x86_64-darwin`

## Testing

Run the test suite:

```bash
bundle exec rake test
```

This will:

1. Compile the native extension (if needed)
2. Run all RSpec tests in the `test/` directory

The default rake task runs the tests:

```bash
bundle exec rake
```

## Usage

### In Your RSpec Project

Add the gem to your `Gemfile`:

```ruby
gem 'rspec_trunk_flaky_tests'
```

Then require it in your `spec_helper.rb` or `rails_helper.rb`:

```ruby
require 'trunk_spec_helper'
```

### Environment Variables

For a complete list of environment variables that the gem accepts, see [`lib/trunk_spec_helper.rb`](lib/trunk_spec_helper.rb). The gem uses the same environment variables as the Trunk Analytics CLI for configuration overrides.

#### `TRUNK_LOCAL_UPLOAD_DIR` (Experimental)

> **⚠️ Experimental Feature**: This feature is experimental. Please reach out to support@trunk.io before attempting to use it.

When `TRUNK_LOCAL_UPLOAD_DIR` is set to a directory path, the RSpec gem will generate an `internal.bin` file and save it locally, relative to where the test was invoked. This disables the automatic upload to Trunk servers.

**Use case**: This is useful when uploads are failing directly within the context of RSpec, but you still want to use quarantining. By generating the `internal.bin` file locally, you can then upload it separately using the Trunk Analytics CLI.

**Usage**:

```bash
TRUNK_LOCAL_UPLOAD_DIR=./test-results bundle exec rspec
```

This will create an `internal.bin` file in the `./test-results` directory (relative to where the command was run).

**Uploading with the CLI**:

After generating the `internal.bin` file, you can upload it using the Trunk Analytics CLI. The CLI supports taking `internal.bin` files as input when running the upload command:

```bash
trunk upload --test-reports ./test-results/internal.bin
```

The CLI will automatically detect that the file is an `internal.bin` file (by its `.bin` extension) and process it accordingly, allowing you to still benefit from quarantining and other features.

## Project Structure

- `lib/` - Ruby library code
  - `rspec_trunk_flaky_tests.rb` - Main library entry point
  - `trunk_spec_helper.rb` - RSpec integration
  - `rspec_trunk_flaky_tests/` - Native extension binaries
- `ext/rspec_trunk_flaky_tests/` - Rust source code for the native extension
- `test/` - RSpec test files
- `spec/` - Test configuration
- `rspec_trunk_flaky_tests.gemspec` - Gem specification

## Development

### Building from Source

1. Clone the repository
2. Install dependencies: `bundle install`
3. Build the extension: `bundle exec rake compile`
4. Run tests: `bundle exec rake test`

### Debugging

The extension uses the `dev` profile by default when running tests. To use a different profile, set the `RB_SYS_CARGO_PROFILE` environment variable:

```bash
RB_SYS_CARGO_PROFILE=release bundle exec rake test
```

### Release

The gem is released using the GitHub Actions workflow at [`.github/workflows/release_ruby_gem.yml`](../.github/workflows/release_ruby_gem.yml).

To release a new version:

1. Trigger the workflow manually via GitHub Actions UI or API
2. Provide the release tag (version number) as input
3. The workflow will:
   - Build the gem for all supported platforms (`x86_64-linux`, `aarch64-linux`, `arm64-darwin`, `x86_64-darwin`)
   - Test the gem on all platforms with Ruby versions 3.0, 3.1, 3.2, 3.3, and 3.4
   - Publish the gem to RubyGems if all tests pass

The workflow automatically handles cross-compilation, testing, and publishing for all supported platforms.
