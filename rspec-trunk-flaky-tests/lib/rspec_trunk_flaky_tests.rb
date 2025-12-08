# frozen_string_literal: true

begin
  ruby_version = /(\d+\.\d+)/.match(RUBY_VERSION)
  require_relative "rspec_trunk_flaky_tests/#{ruby_version}/rspec_trunk_flaky_tests"
rescue LoadError
  require_relative 'rspec_trunk_flaky_tests/rspec_trunk_flaky_tests'
end
