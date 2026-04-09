# frozen_string_literal: true

begin
  ruby_version = /(\d+\.\d+)/.match(RUBY_VERSION)
  require_relative "rspec_trunk_flaky_tests/#{ruby_version}/rspec_trunk_flaky_tests"
rescue LoadError
  begin
    require_relative 'rspec_trunk_flaky_tests/rspec_trunk_flaky_tests'
  rescue LoadError
    raise LoadError, <<~MSG
      Could not load the native extension for rspec_trunk_flaky_tests.

      You are likely installing the pure ruby variant of this gem, which does not
      include a precompiled native extension. To fix this, add your platform to
      your Gemfile.lock:

          bundle lock --add-platform x86_64-linux    # Standard Linux
          bundle lock --add-platform aarch64-linux   # ARM Linux
          bundle lock --add-platform arm64-darwin    # Apple Silicon macOS
          bundle lock --add-platform x86_64-darwin   # Intel macOS

      Then run `bundle install` again. Bundler will automatically select the
      precompiled native gem for your platform.

      Supported platforms: x86_64-linux, aarch64-linux, arm64-darwin, x86_64-darwin
      Current platform: #{RUBY_PLATFORM}
      Current Ruby version: #{RUBY_VERSION}
    MSG
  end
end
