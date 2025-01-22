# frozen_string_literal: true

require 'mkmf'
require 'rb_sys/mkmf'

create_rust_makefile('context_ruby/context_ruby') do |r|
  r.env = { 'OPENSSL_INCLUDE_DIR' => '/usr/include', 'OPENSSL_DIR' => '/usr', 'OPENSSL_LIB_DIR' => '/usr/lib' }
end
