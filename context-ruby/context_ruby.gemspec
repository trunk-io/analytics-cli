# frozen_string_literal: true

Gem::Specification.new do |s|
  # trunk-ignore(rubocop/Gemspec/RequiredRubyVersion)
  s.required_ruby_version = '>= 3.0'
  s.name        = 'rspec_trunk_flaky_tests'
  s.version     = '0.0.0'
  s.summary     = 'Trunk Flaky Tests helper gem'
  s.authors     = ['Trunk Technologies, Inc.']
  s.email       = 'support@trunk.io'
  s.files       = Dir['lib/**/*.rb', 'ext/**/*.{rs,rb}', '**/Cargo.*']
  s.add_runtime_dependency('rspec-core', '>3.3')
  s.add_dependency('colorize', '=1.1.0')
  s.add_dependency('rb_sys', '=0.9.103')
  s.add_development_dependency('rspec')
  s.homepage    = 'https://trunk.io'
  s.license     = 'MIT'
  s.executables = []
  s.require_paths = ['lib']
  s.extensions = ['ext/context_ruby/extconf.rb']
end
