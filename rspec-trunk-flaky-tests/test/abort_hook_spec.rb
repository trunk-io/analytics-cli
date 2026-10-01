# frozen_string_literal: true

require 'rspec_trunk_flaky_tests'
require_relative '../spec/spec_helper'
require 'rspec/core/sandbox'

# trunk-ignore(rubocop/Metrics/BlockLength)
RSpec.describe 'abort hook' do
  # trunk-ignore(rubocop/Metrics/MethodLength,rubocop/Metrics/AbcSize)
  def run_example(abort_remaining:)
    run = Object.new
    run.define_singleton_method(:abort_remaining?) { abort_remaining }
    run.define_singleton_method(:abort_failure) { |_example| nil }
    run.define_singleton_method(:start_attempt) { |_example| nil }
    hooks_ran = []
    example = nil
    RSpec::Core::Sandbox.sandboxed do |config|
      config.before(:example) { hooks_ran << :suite }
      RSpec::Trunk.install(config, run)
      group = RSpec.describe('sandboxed') do
        before { hooks_ran << :group }
        it('e') { hooks_ran << :body }
      end
      group.run(RSpec::Core::NullReporter)
      example = group.examples.first
    end
    [example, hooks_ran]
  end

  it 'skips before any other before hook runs' do
    example, hooks_ran = run_example(abort_remaining: true)
    expect(example.execution_result.status).to eq(:pending)
    expect(example.execution_result.pending_message).to eq('Quarantine lookup failed, skipping test run')
    expect(hooks_ran).to be_empty
  end

  it 'leaves the example alone otherwise' do
    example, hooks_ran = run_example(abort_remaining: false)
    expect(example.execution_result.status).to eq(:passed)
    expect(hooks_ran).to eq(%i[suite group body])
  end
end
