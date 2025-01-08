# frozen_string_literal: true

Gem::Specification.new do |s|
  s.required_ruby_version = '>= 2.6.0'
  s.name        = 'trunk_analytics'
  s.version     = '0.0.2'
  s.summary     = 'trunk analytics helper gem'
  s.description = 'trunk analytics helper gem'
  s.authors     = ['Trunk Technologies, Inc.']
  s.email       = 'support@trunk.io'
  s.files       = ['lib/trunk_analytics/trunk_spec_helper.rb', 'lib/context_ruby/context_ruby.so',
                   'lib/context_ruby.rb']
  s.add_dependency 'rspec', '~> 3.0'
  s.add_dependency 'rspec-core', '~> 3.0'
  s.homepage    = 'https://trunk.io'
  s.license     = 'MIT'
end
