# frozen_string_literal: true

require 'mkmf'
require 'rb_sys/mkmf'

create_rust_makefile('context_ruby/context_ruby') do |r|
  r.env = { "RUBY_CONFIGURE_OPTS" => "--with-openssl-dir=/opt/local/" }
end
