# frozen_string_literal: true

require 'mkmf'
require 'rb_sys/mkmf'

create_rust_makefile('context_ruby/context_ruby') do |r|
  # For darwin multiple ranlibs are being packaged and the container chooses the wrong one.
  # This is a workaround to force the correct ranlib to be used.
  # https://github.com/cross-rs/cross/issues/1243#issuecomment-2102742482
  if RUBY_PLATFORM == 'arm64-darwin'
    r.env = { 'RANLIB' => '/opt/osxcross/target/bin/arm64e-apple-darwin-ranlib' }
  elsif RUBY_PLATFORM == 'x86_64-darwin'
    r.env = { 'RANLIB' => '/opt/osxcross/target/bin/x86_64-apple-darwin-ranlib' }
  end
  # r.env = { 'RANLIB'=>'/opt/osxcross/target/bin/arm64e-apple-darwin-ranlib'}
  # 'OPENSSL_INCLUDE_DIR' => '/usr/include', 'OPENSSL_DIR' => '/usr', 'OPENSSL_LIB_DIR' => '/usr/lib' }
end
