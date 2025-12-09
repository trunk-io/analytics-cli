# frozen_string_literal: true

Gem::Specification.new do |s|
  # trunk-ignore(rubocop/Gemspec/RequiredRubyVersion)
  s.required_ruby_version = '>= 3.0'
  s.name        = 'rspec_trunk_flaky_tests'
  s.version     = '0.0.0'
  # trunk-ignore(rubocop/Layout/LineLength)
  s.summary     = 'RSpec plugin for Trunk Flaky Tests - automatically uploads test results to detect and quarantine flaky tests'
  # trunk-ignore(rubocop/Layout/LineLength)
  s.description = 'Integrates RSpec with Trunk Flaky Tests to automatically upload test results from your CI jobs. Enables accurate flaky test detection, quarantining, and analytics.'
  s.authors     = ['Trunk Technologies, Inc.']
  s.email       = 'support@trunk.io'
  s.files       = Dir['lib/**/*.rb', 'ext/**/*.{rs,rb}', '**/Cargo.*']
  s.add_runtime_dependency('rspec-core', '>3.3')
  s.add_dependency('rb_sys', '=0.9.103')
  s.add_development_dependency('rspec')
  s.homepage    = 'https://docs.trunk.io/flaky-tests/get-started/frameworks/rspec'
  s.license     = 'MIT'
  s.executables = []
  s.require_paths = ['lib']
  s.extensions = ['ext/rspec_trunk_flaky_tests/extconf.rb']
end
