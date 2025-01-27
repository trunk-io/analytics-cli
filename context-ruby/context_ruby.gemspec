# frozen_string_literal: true

Gem::Specification.new do |s|
  s.required_ruby_version = '>= 3.1'
  s.name        = 'trunk_analytics'
  s.version     = '0.0.8'
  s.summary     = 'trunk analytics helper gem'
  s.authors     = ['Trunk Technologies, Inc.']
  s.email       = 'support@trunk.io'
  s.files       = Dir['lib/**/*.rb', 'ext/**/*.{rs,rb}', '**/Cargo.*']
  s.add_runtime_dependency(%{rspec-core}, '>3.3')
  s.add_development_dependency %q{rspec}
  s.homepage    = 'https://trunk.io'
  s.license     = 'MIT'
  s.add_dependency 'rb_sys'
  s.executables = []
  s.require_paths = ['lib']
  s.extensions = ['ext/context_ruby/extconf.rb']
end
