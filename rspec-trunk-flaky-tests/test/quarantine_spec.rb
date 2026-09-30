# frozen_string_literal: true

require 'rspec_trunk_flaky_tests'
require_relative '../spec/spec_helper'
require_relative 'support/trunk_harness'

# How quarantine outcomes carry through to what RSpec reports and what Trunk
# records, in the corners where RSpec's own control flow gets in the way.
#
# trunk-ignore(rubocop/Metrics/BlockLength)
RSpec.describe 'quarantine outcomes' do
  # Runs a sandboxed group; returns what group.run returned (which feeds RSpec's
  # exit status), its examples, and the Run.
  def run_group(lookup, &block)
    outcome = nil
    TrunkHarness.with_trunk(lookup) do |_config, run|
      group = RSpec.describe('sandboxed', &block)
      outcome = [group.run(RSpec::Core::NullReporter), group.examples, run]
    end
    outcome
  end

  def listener_records(examples, run)
    listener = RSpec::Trunk::AnalyticsListener.new(run)
    examples.each { |example| listener.example_finished(RSpec::Core::Notifications::ExampleNotification.for(example)) }
    listener.close(nil)
  end

  context 'when a lookup failure aborts the run' do
    around do |example|
      previous = ENV.fetch('TRUNK_QUARANTINE_QUERY_FAILURE_EXIT', nil)
      ENV['TRUNK_QUARANTINE_QUERY_FAILURE_EXIT'] = 'true'
      example.run
    ensure
      ENV['TRUNK_QUARANTINE_QUERY_FAILURE_EXIT'] = previous
    end

    # rspec-retry ignores RSpec.world.wants_to_quit and re-runs the example in
    # place; the abort must not turn the retry into a skip that hides the failure.
    it 'keeps failing an example that is re-run in place' do
      body_runs = 0
      passed, examples, = run_group(:failed) do
        around do |ex|
          ex.run
          ex.example.instance_variable_set(:@exception, nil) # what rspec-retry's clear_exception does
          ex.run
        end
        it('fails') do
          body_runs += 1
          raise 'the real failure'
        end
      end

      expect(passed).to be(false)
      expect(examples.first.execution_result.status).to eq(:failed)
      expect(examples.first.exception.message).to eq('the real failure')
      expect(body_runs).to eq(1)
    end
  end

  context 'when a before(:context) hook raises' do
    let(:group_body) do
      proc do
        before(:context) { raise 'boom in before(:context)' }
        it('a') { expect(1).to eq(1) }
        it('b') { expect(1).to eq(1) }
      end
    end

    it 'passes the group when every example is quarantined' do
      passed, examples, = run_group(:quarantined, &group_body)
      expect(examples.map { |example| example.execution_result.status }).to eq(%i[passed passed])
      expect(passed).to be(true)
    end

    it 'still fails the group when the examples are not quarantined' do
      passed, examples, = run_group(:not_quarantined, &group_body)
      expect(examples.map { |example| example.execution_result.status }).to eq(%i[failed failed])
      expect(passed).to be(false)
    end
  end

  it 'records every failure of a quarantined example, not just the last' do
    passed, examples, = run_group(:quarantined) do
      after { raise 'cleanup error' }
      it('fails') { raise 'the real failure' }
    end

    expect(passed).to be(true)
    recorded = examples.first.metadata[:trunk_quarantined_exception]
    expect(recorded).to be_a(RSpec::Core::MultipleExceptionError)
    expect(recorded.all_exceptions.map(&:message)).to eq(['the real failure', 'cleanup error'])
  end

  it 'records and uploads nothing for --dry-run' do
    run = nil
    TrunkHarness.with_trunk(:not_quarantined) do |config, trunk_run|
      config.dry_run = true
      group = RSpec.describe('sandboxed') { it('fails') { raise 'never runs' } }
      group.run(RSpec::Core::NullReporter)
      listener_records(group.examples, trunk_run)
      run = trunk_run
    end
    expect(run.report.added).to be_empty
    expect(run.report.published).to be(false)
  end

  it 'records start and finish times with their sub-second part' do
    _, examples, run = run_group(:not_quarantined) { it('passes') { expect(1).to eq(1) } }
    listener_records(examples, run)

    result = examples.first.execution_result
    started_at, finished_at = run.report.added.first.values_at(8, 9)
    expect([started_at, finished_at]).to eq([result.started_at.to_f, result.finished_at.to_f])
    expect(run.report.published).to be(true)
  end
end
