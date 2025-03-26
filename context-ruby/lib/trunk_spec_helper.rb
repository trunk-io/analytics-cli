# frozen_string_literal: true

require 'rspec/core'
require 'time'
require 'context_ruby'
require 'colorize'

def escape(str)
  str.dump[1..-2]
end

def description_generated?(example)
  auto_generated_exp = /^\s?is expected to eq .*$/
  full_description = example.full_description
  parent_description = example.example_group.description
  checked_description = full_description.sub(parent_description, '')
  auto_generated_exp.match(checked_description) != nil
end

def generate_id(example)
  return "trunk:#{example.id}-#{example.location}" if description_generated?(example)
end

# we want to cache the test report so we can add to it as we go and reduce the number of API calls
$test_report = TestReport.new('rspec', "#{$PROGRAM_NAME} #{ARGV.join(' ')}")

module RSpec
  module Core
    # Example is the class that represents a test case
    class Example
      # keep the original method around so we can call it
      alias set_exception_core set_exception
      # RSpec uses the existance of an exception to determine if the test failed
      # We need to override this to allow us to capture the exception and then
      # decide if we want to fail the test or not
      # trunk-ignore(rubocop/Naming/AccessorMethodName,rubocop/Metrics/MethodLength,rubocop/Metrics/AbcSize)
      def set_exception(exception)
        id = generate_id(self)
        name = full_description
        parent_name = example_group.metadata[:description]
        parent_name = parent_name.empty? ? 'rspec' : parent_name
        file = escape(metadata[:file_path])
        classname = file.sub(%r{\.[^/.]+\Z}, '').gsub('/', '.').gsub(/\A\.+|\.+\Z/, '')
        puts "Checking if test is quarantined: `#{name}` in `#{parent_name}`".colorize(:yellow)
        if $test_report.is_quarantined(id, name, parent_name, classname, file)
          # monitor the override in the metadata
          metadata[:quarantined_exception] = exception
          puts "Test is quarantined, overriding exception: #{exception}".colorize(:green)
          nil
        else
          puts 'Test is not quarantined, continuing'.colorize(:red)
          set_exception_core(exception)
        end
      end

      # Procsy is a class that is used to wrap execution of the Example class
      class Procsy
        def run_with_trunk
          RSpec::Trunk.new(self).run
        end
      end
    end
  end
end

module RSpec
  # Trunk is a class that is used to monitor the execution of the Example class
  class Trunk
    def self.setup
      RSpec.configure do |config|
        if ENV['DISABLE_RSPEC_TRUNK_FLAKY_TESTS'] == 'true'
          config.around(:each, &:run)
        else
          config.around(:each, &:run_with_trunk)
        end
      end
    end

    def initialize(example)
      @example = example
    end

    def current_example
      @current_example ||= RSpec.current_example
    end

    def run
      # run the test
      @example.run
      # monitor attempts in the metadata
      if @example.metadata[:attempt_number]
        @example.metadata[:attempt_number] += 1
      else
        @example.metadata[:attempt_number] = 0
      end
    end
  end
end

# TrunkAnalyticsListener is a class that is used to listen to the execution of the Example class
# it generates and submits the final test reports
class TrunkAnalyticsListener
  def initialize
    @testreport = $test_report
  end

  def example_finished(notification)
    add_test_case(notification.example)
  end

  def close(_notification)
    res = @testreport.publish
    if res
      puts 'Flaky tests report upload complete'.colorize(:green)
    else
      puts 'Failed to publish flaky tests report'.colorize(:red)
    end
  end

  # trunk-ignore(rubocop/Metrics/AbcSize,rubocop/Metrics/MethodLength,rubocop/Metrics/CyclomaticComplexity)
  def add_test_case(example)
    failure_message = example.exception.to_s if example.exception
    failure_message = example.metadata[:quarantined_exception].to_s if example.metadata[:quarantined_exception]
    # TODO: should we use concatenated string or alias when auto-generated description?
    name = example.full_description
    file = escape(example.metadata[:file_path])
    classname = file.sub(%r{\.[^/.]+\Z}, '').gsub('/', '.').gsub(/\A\.+|\.+\Z/, '')
    line = example.metadata[:line_number]
    started_at = example.execution_result.started_at.to_i
    finished_at = example.execution_result.finished_at.to_i
    id = generate_id(example)

    attempt_number = example.metadata[:attempt_number] || 0
    status = example.execution_result.status.to_s
    # set the status to failure, but mark it as quarantined
    is_quarantined = example.metadata[:quarantined_exception] ? true : false
    case example.execution_result.status
    when :passed
      status = is_quarantined ? Status.new('failure') : Status.new('success')
    when :failed
      status = Status.new('failure')
    when :pending
      status = Status.new('skipped')
    end
    parent_name = example.example_group.metadata[:description]
    parent_name = parent_name.empty? ? 'rspec' : parent_name
    @testreport.add_test(id, name, classname, file, parent_name, line, status, attempt_number,
                         started_at, finished_at, failure_message || '', is_quarantined)
  end
end

RSpec.configure do |c|
  next if ENV['DISABLE_RSPEC_TRUNK_FLAKY_TESTS'] == 'true'

  c.reporter.register_listener TrunkAnalyticsListener.new, :example_finished, :close
end

RSpec::Trunk.setup
