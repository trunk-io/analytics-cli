# frozen_string_literal: true

require_relative '../lib/rspec_trunk_flaky_tests'
require_relative '../spec/spec_helper'
require 'rspec/core/sandbox'

# RSpec inverts pass/fail for `pending` examples, so trunk_spec_helper's
# TrunkAnalyticsListener#status_and_exception reverses them when deciding what to
# report to Trunk. These tests run real pending/skip examples in an RSpec sandbox
# (so the "fixed pending" failure does not fail this suite) and assert the mapping:
#
#   skip / xit (never ran)      -> skipped
#   pending, body still failing -> success (the "should fail" expectation was met)
#   pending, body now passing   -> failure (PendingExampleFixedError; breaks build)
#
# trunk-ignore(rubocop/Metrics/BlockLength)
RSpec.describe 'pending/skip status mapping' do
  let(:listener) { TrunkAnalyticsListener.new }

  # Define and run examples in an isolated RSpec world, returning the executed
  # Example objects (with their execution_result populated) for inspection.
  def run_examples(&block)
    examples = []
    RSpec::Core::Sandbox.sandboxed do |_config|
      group = RSpec.describe('sandboxed', &block)
      group.run(RSpec::Core::NullReporter)
      examples = group.examples
    end
    examples
  end

  def status_of(example)
    listener.status_and_exception(example).first.to_s
  end

  it 'reports skip as skipped' do
    example = run_examples do
      it('s') do
        skip('not ready')
        expect(1).to eq(2)
      end
    end.first
    expect(status_of(example)).to eq('skipped')
  end

  it 'reports xit as skipped' do
    example = run_examples { xit('x') { expect(1).to eq(2) } }.first
    expect(status_of(example)).to eq('skipped')
  end

  it 'reports a still-failing pending example as success' do
    example = run_examples do
      it('p') do
        pending('known broken')
        expect(1).to eq(2)
      end
    end.first
    expect(status_of(example)).to eq('success')
  end

  it 'reports a fixed (now passing) pending example as failure, carrying the fixed error' do
    example = run_examples do
      it('f') do
        pending('should still fail')
        expect(1).to eq(1)
      end
    end.first
    status, exception = listener.status_and_exception(example)
    expect(status.to_s).to eq('failure')
    expect(exception).to be_a(RSpec::Core::Pending::PendingExampleFixedError)
  end

  it 'leaves ordinary pass/fail unchanged' do
    examples = run_examples do
      it('ok') { expect(1).to eq(1) }
      it('bad') { expect(1).to eq(2) }
    end
    expect(status_of(examples[0])).to eq('success')
    expect(status_of(examples[1])).to eq('failure')
  end
end
