# frozen_string_literal: true

begin
  ruby_version = /(\d+\.\d+)/.match(RUBY_VERSION)
  require_relative "context_ruby/#{ruby_version}/context_ruby"
rescue LoadError
  require_relative 'context_ruby/context_ruby'
end
