# frozen_string_literal: true

require 'rspec/core/sandbox'
require 'stringio'

# The unit suite runs with Trunk disabled, so setup never applied the gem's patches. Applying
# them here is safe: they change nothing unless RSpec::Trunk.current_run is set.
module TrunkHarness
  LookupResult = Struct.new(:quarantine_lookup_failed, :quarantining_disabled_for_repo, :test_is_quarantined)

  # Stands in for the native TestReport.
  class FakeReport
    attr_reader :added, :submitted, :lookups

    def initialize(result)
      @result = result
      @added = []
      @submitted = false
      @lookups = 0
    end

    # trunk-ignore(rubocop/Naming/PredicatePrefix)
    def is_quarantined(*)
      @lookups += 1
      @result
    end

    def add_test(*args)
      @added << args
    end

    def publish
      @submitted = true
    end

    def try_save(_dir)
      @submitted = true
    end
  end

  # rspec-retry's in-place re-run, without its :retry_attempts metadata.
  RERUN_IN_PLACE = proc do |ex|
    ex.run
    ex.example.instance_variable_set(:@exception, nil)
    ex.run
  end

  module_function

  def report_for(lookup)
    case lookup
    when :quarantined then FakeReport.new(LookupResult.new(false, false, true))
    when :not_quarantined then FakeReport.new(LookupResult.new(false, false, false))
    when :failed then FakeReport.new(LookupResult.new(true, false, false))
    end
  end

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
