# frozen_string_literal: true

spec.required_ruby_version = '>= 2.6.0'
spec.extensions = ['ext/context_ruby_gem/extconf.rb']

# only needed when developing or packaging your gem
spec.add_development_dependency 'rake-compiler', '~> 1.2.0'
