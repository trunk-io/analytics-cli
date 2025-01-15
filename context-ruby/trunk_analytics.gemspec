# frozen_string_literal: true

Gem::Specification.new do |s|
  s.required_ruby_version = '>= 3.2'
  s.name        = 'trunk_analytics'
  s.version     = '0.0.2'
  s.summary     = 'trunk analytics helper gem'
  s.authors     = ['Trunk Technologies, Inc.']
  s.email       = 'support@trunk.io'
  s.files       = Dir["{ext,lib}/**/*"]
  s.add_dependency 'rspec', '~> 3.0'
  s.add_dependency 'rspec-core', '~> 3.0'
  s.homepage    = 'https://trunk.io'
  s.license     = 'MIT'
  s.add_dependency "rb_sys"
  s.extensions = ["ext/context_ruby/extconf.rb"]
end
