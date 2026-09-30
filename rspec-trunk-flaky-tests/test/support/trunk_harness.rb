# frozen_string_literal: true

require 'rspec/core/sandbox'
require 'stringio'

# Runs sandboxed examples with Trunk active against a fake report, so specs can
# drive quarantine outcomes without the API. The unit suite runs with Trunk
# disabled, so RSpec::Trunk.setup never applied the gem's patches; they do
# nothing while no run is current, so applying them here is safe.
module TrunkHarness
  LookupResult = Struct.new(:quarantine_lookup_failed, :quarantining_disabled_for_repo, :test_is_quarantined)

  # Stands in for the native TestReport: answers every lookup with `result` and
  # records what the listener would upload.
  class FakeReport
    attr_reader :added, :published

    def initialize(result)
      @result = result
      @added = []
      @published = false
    end

    # trunk-ignore(rubocop/Naming/PredicatePrefix)
    def is_quarantined(*)
      @result
    end

    def add_test(*args)
      @added << args
    end

    def publish
      @published = true
    end
  end

  module_function

  def report_for(lookup)
    case lookup
    when :quarantined then FakeReport.new(LookupResult.new(false, false, true))
    when :not_quarantined then FakeReport.new(LookupResult.new(false, false, false))
    when :failed then FakeReport.new(LookupResult.new(true, false, false))
    end
  end

  # Yields a sandboxed RSpec configuration with Trunk installed on it and a Run
  # whose report answers every quarantine lookup with `lookup`. The gem's console
  # output is swallowed.
  # trunk-ignore(rubocop/Metrics/MethodLength)
  def with_trunk(lookup)
    RSpec::Core::Example.prepend(RSpec::Trunk::ExampleExtension)
    RSpec::Core::ExampleGroup.singleton_class.prepend(RSpec::Trunk::ExampleGroupExtension)
    run = RSpec::Trunk::Run.new(report_for(lookup))
    previous_run = RSpec::Trunk.current_run
    previous_stdout = $stdout
    RSpec::Trunk.instance_variable_set(:@current_run, run)
    $stdout = StringIO.new
    RSpec::Core::Sandbox.sandboxed do |config|
      RSpec::Trunk.install(config, run)
      yield config, run
    end
  ensure
    $stdout = previous_stdout
    RSpec::Trunk.instance_variable_set(:@current_run, previous_run)
  end
end
