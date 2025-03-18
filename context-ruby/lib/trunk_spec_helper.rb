# frozen_string_literal: true

require 'rspec/core'
require 'time'
require 'context_ruby'

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

# TODO move this into the bindings
def generate_id(example)
  if description_generated?(example)
    # trunk-ignore(rubocop/Style/SoleNestedConditional)
    return "trunk:#{example.id}-#{example.location}" if description_generated?(example)
  end
end

class String
  def red; "\e[31m#{self}\e[0m" end
  def yellow; "\e[33m#{self}\e[0m" end
  def green; "\e[32m#{self}\e[0m" end
end

module RSpec
  module Core
    # Example is the class that represents a test case
    class Example
      # keep the original method around so we can call it
      alias set_exception_core set_exception
      # RSpec uses the existance of an exception to determine if the test failed
      # We need to override this to allow us to capture the exception and then
      # decide if we want to fail the test or not
      # trunk-ignore(rubocop/Naming/AccessorMethodName)
      def set_exception(exception)
        id = generate_id(self)
        test_report = TestReport.new('rspec', "#{$PROGRAM_NAME} #{ARGV.join(' ')}")
        name = self.full_description
        parent_name = self.example_group.metadata[:description]
        parent_name = parent_name.empty? ? 'rspec' : parent_name
        file = escape(self.metadata[:file_path])
        classname = file.sub(%r{\.[^/.]+\Z}, '').gsub('/', '.').gsub(/\A\.+|\.+\Z/, '')
        puts "checking if test is quarantined: `#{name}` in `#{parent_name}`".yellow
        if test_report.is_quarantined(id, name, parent_name, classname, file)
          # monitor the override in the metadata
          self.metadata[:quarantined_exception] = exception
          puts "Quarantined test: `#{name}` in `#{parent_name}`".yellow
          puts "Exception: #{exception}".red
          nil
        else
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
    @testreport = TestReport.new('rspec', "#{$PROGRAM_NAME} #{ARGV.join(' ')}")
  end

  def example_finished(notification)
    add_test_case(notification.example)
  end

  def close(_notification)
    res = true
    # @testreport.publish
    if res
      puts 'Flaky tests report upload complete'.green
    else
      puts 'Failed to publish flaky tests report'.red
    end
  end

  # trunk-ignore(rubocop/Metrics/AbcSize,rubocop/Metrics/MethodLength,rubocop/Metrics/CyclomaticComplexity)
  def add_test_case(example)
    if example.exception
      failure_message = example.exception.message
      # failure details is far more robust than the message, but noiser
      # if example.exception.backtrace
      # failure_details = example.exception.backtrace.join('\n')
      # end
    end
    if example.metadata[:quarantined_exception]
      failure_message = "#{example.metadata[:quarantined_exception]}"
    end
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
    case example.execution_result.status
    when :passed
      status = Status.new('success')
    when :failed
      status = Status.new('failure')
    when :pending
      status = Status.new('skipped')
    end
    if example.metadata[:quarantined_exception]
      status = Status.new('failure')
    end
    parent_name = example.example_group.metadata[:description]
    parent_name = parent_name.empty? ? 'rspec' : parent_name
    @testreport.add_test(id, name, classname, file, parent_name, line, status, attempt_number,
                         started_at, finished_at, failure_message || '')
  end
end

RSpec.configure do |c|
  next if ENV['DISABLE_RSPEC_TRUNK_FLAKY_TESTS'] == 'true'

  c.reporter.register_listener TrunkAnalyticsListener.new, :example_finished, :close
end

RSpec::Trunk.setup
