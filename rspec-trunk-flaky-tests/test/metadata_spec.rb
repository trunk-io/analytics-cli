# frozen_string_literal: true

require 'rspec_trunk_flaky_tests'
require_relative '../spec/spec_helper'
require 'rspec/core/sandbox'

# Metadata keys the gem sets are trunk_-prefixed; each is also written under its
# old, deprecated unprefixed name so existing readers keep working for now.
#
# trunk-ignore(rubocop/Metrics/BlockLength)
RSpec.describe 'example metadata keys' do
  it 'writes a trunk_ key under its deprecated name too' do
    metadata = {}
    RSpec::Trunk.write_metadata(metadata, :trunk_quarantined_exception, :boom)
    expect(metadata).to eq(trunk_quarantined_exception: :boom, quarantined_exception: :boom)
  end

  it 'prefixes every key it maps' do
    expect(RSpec::Trunk::DEPRECATED_METADATA_KEYS.keys).to all(satisfy { |key| key.start_with?('trunk_') })
  end

  it 'counts attempts under both names' do
    example = Struct.new(:metadata) { def run; end }.new({})
    RSpec::Trunk.run_counting_attempts(example)
    RSpec::Trunk.run_counting_attempts(example)
    expect(example.metadata).to eq(trunk_attempt_number: 1, attempt_number: 1)
  end

  it 'reports a quarantined failure from the trunk_ key' do
    example = nil
    RSpec::Core::Sandbox.sandboxed do |_config|
      group = RSpec.describe('sandboxed') { it('q') { expect(1).to eq(1) } }
      group.run(RSpec::Core::NullReporter)
      example = group.examples.first
    end
    error = StandardError.new('quarantined')
    example.metadata[:trunk_quarantined_exception] = error

    status, exception = RSpec::Trunk::AnalyticsListener.new(nil).status_and_exception(example)
    expect(status.to_s).to eq('failure')
    expect(exception).to be(error)
  end
end
