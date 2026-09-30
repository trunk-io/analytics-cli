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

  # trunk-ignore(rubocop/Metrics/BlockLength)
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
      passed, examples, run = run_group(:failed) do
        around(&TrunkHarness::RERUN_IN_PLACE)
        it('fails') do
          body_runs += 1
          raise 'the real failure'
        end
      end

      expect(passed).to be(false)
      expect(examples.first.execution_result.status).to eq(:failed)
      expect(examples.first.exception.message).to eq('the real failure')
      expect(body_runs).to eq(1)
      # The replayed failure isn't looked up again.
      expect(run.report.lookups).to eq(1)
    end

    it 'replays the failure that aborted the run, not a later after-hook error' do
      _, examples, = run_group(:failed) do
        around(&TrunkHarness::RERUN_IN_PLACE)
        after { raise 'cleanup error' }
        it('fails') { raise 'the real failure' }
      end

      # The re-run's own after hook fails again, as it would have anyway.
      expect(examples.first.exception.all_exceptions.map(&:message)).to eq(['the real failure', 'cleanup error'])
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

    it 'still fails the group when a pending example hides the error' do
      # RSpec files the error under the pending example's pending_exception and
      # records it :passed; plain RSpec still fails the group.
      passed, examples, = run_group(:quarantined) do
        before(:context) { raise 'db down' }
        it('p', :pending) { expect(1).to eq(2) }
      end
      expect(examples.first.execution_result.status).to eq(:passed)
      expect(passed).to be(false)
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

  it 'records only the latest attempt when an example is re-run in place' do
    attempt = 0
    _, examples, = run_group(:quarantined) do
      around { |ex| 2.times { ex.run } }
      it('fails') { raise "attempt #{attempt += 1}" }
    end
    expect(examples.first.metadata[:trunk_quarantined_exception].message).to eq('attempt 2')
  end

  it 'records and submits nothing for --dry-run' do
    run = nil
    TrunkHarness.with_trunk(:not_quarantined) do |config, trunk_run|
      config.dry_run = true
      group = RSpec.describe('sandboxed') { it('fails') { raise 'never runs' } }
      group.run(RSpec::Core::NullReporter)
      listener_records(group.examples, trunk_run)
      run = trunk_run
    end
    expect(run.report.added).to be_empty
    expect(run.report.submitted).to be(false)
  end

  it 'records start and finish times with their sub-second part' do
    _, examples, run = run_group(:not_quarantined) { it('passes') { expect(1).to eq(1) } }
    listener_records(examples, run)

    result = examples.first.execution_result
    started_at, finished_at = run.report.added.first.values_at(8, 9)
    expect([started_at, finished_at]).to eq([result.started_at.to_f, result.finished_at.to_f])
    expect(run.report.submitted).to be(true)
  end
end
