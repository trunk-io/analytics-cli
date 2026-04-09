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

      You are using the pure ruby variant of this gem, which is a placeholder
      that exists so that Gemfile.lock files with `PLATFORMS ruby` can resolve
      this dependency. It does not include a precompiled native extension.

      Precompiled native gems are available for:
        x86_64-linux, aarch64-linux, arm64-darwin, x86_64-darwin

      If you are on a supported platform and seeing this error, make sure
      bundler is selecting the native variant for your platform:

          bundle lock --add-platform #{RUBY_PLATFORM}
          bundle install

      If you are on an unsupported platform, this gem cannot be used in your
      environment. Please open an issue at:
        https://github.com/trunk-io/analytics-cli/issues

      Current platform: #{RUBY_PLATFORM}
      Current Ruby version: #{RUBY_VERSION}
    MSG
  end
end
