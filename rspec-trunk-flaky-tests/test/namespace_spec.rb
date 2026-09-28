# frozen_string_literal: true

require 'json'
require 'open3'
require 'rbconfig'
require_relative '../spec/spec_helper'

# The gem is loaded into other people's test suites, so everything it defines --
# in Ruby or in the native extension -- must stay under RSpec::Trunk. This loads
# the gem, with Trunk enabled, into a fresh Ruby process and diffs the whole
# object space before and after: no new top-level constants or methods, no
# $globals, and no methods or constants added to anything the gem doesn't own
# (String, Object, Kernel, RSpec::Core::Example, ...).
#
# RuboCop (Style/GlobalVars, Style/TopLevelMethodDefinition) catches some of this
# in the Ruby source; this also covers the native extension and core-class patches.
#
# trunk-ignore(rubocop/Metrics/BlockLength)
RSpec.describe 'global namespace' do
  # Runs in the child process; prints what loading the gem added outside RSpec::Trunk.
  let(:probe) do
    <<~'RUBY'
      # Dependencies the helper requires; what they define is not ours.
      require 'rspec/core'
      require 'rspec/core/formatters/exception_presenter'
      require 'time'
      require 'json'

      owned = ->(name) { name == 'RSpec::Trunk' || name.start_with?('RSpec::Trunk::') }
      # RubyGems and Bundler define things lazily while resolving any require.
      tooling = ->(name) { name.start_with?('Gem', 'Bundler') }
      snapshot = lambda do
        ObjectSpace.each_object(Module).each_with_object({}) do |mod, acc|
          name = mod.name
          next if name.nil? || owned.(name) || tooling.(name)

          acc[name] = {
            'constants' => mod.constants(false).map(&:to_s),
            'methods' => (mod.instance_methods(false) + mod.private_instance_methods(false)).map(&:to_s),
            'singleton_methods' => mod.singleton_methods(false).map(&:to_s)
          }
        end
      end

      before = snapshot.call
      globals_before = global_variables

      require 'rspec_trunk_flaky_tests'
      require 'trunk_spec_helper'
      raise 'RSpec::Trunk.setup did not run' unless RSpec::Trunk.current_run

      after = snapshot.call
      leaks = []
      after.each do |name, now|
        was = before[name]
        next leaks << "new module #{name}" unless was

        now.each do |kind, members|
          (members - was[kind]).each { |member| leaks << "#{name} #{kind}: #{member}" }
        end
      end
      (global_variables - globals_before).each { |var| leaks << "global variable #{var}" }
      puts JSON.generate(leaks)
    RUBY
  end

  let(:lib_dir) { File.dirname($LOAD_PATH.resolve_feature_path('trunk_spec_helper').last) }

  it 'defines nothing outside RSpec::Trunk' do
    env = {
      'TRUNK_ORG_URL_SLUG' => 'namespace-spec',
      'TRUNK_API_TOKEN' => 'namespace-spec',
      'DISABLE_RSPEC_TRUNK_FLAKY_TESTS' => nil
    }
    stdout, stderr, status = Open3.capture3(env, RbConfig.ruby, '-I', lib_dir, '-e', probe)
    expect(status).to be_success, stderr

    # RSpec gaining its Trunk constant is the one expected addition.
    expect(JSON.parse(stdout.lines.last)).to contain_exactly('RSpec constants: Trunk')
  end
end
