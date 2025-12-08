# rspec-trunk-flaky-tests

RSpec plugin for Trunk Flaky Tests. This gem automatically uploads test results to detect and quarantine flaky tests. It integrates with RSpec to provide automatic flaky test detection, quarantining, and analytics.

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
