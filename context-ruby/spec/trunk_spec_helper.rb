# frozen_string_literal: true

require 'rspec/core'
require 'time'
require 'context_ruby/context_ruby'

def escape(str)
  str.dump[1..-2]
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
        # TODO: this is where we'll need to override the result once the logic is ready
        # trunk-ignore(rubocop/Lint/LiteralAsCondition)
        if true
          set_exception_core(exception)
        else
          # monitor the override in the metadata
          metadata[:quarantined_exception] = exception
          nil
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
        config.around(:each, &:run_with_trunk)
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
      if @example.metadata[:attempts]
        @example.metadata[:attempts] += 1
      else
        @example.metadata[:attempts] = 1
      end

      # add the test to the report
      # return the report
      @testreport
    end
  end
end

# TrunkAnalyticsListener is a class that is used to listen to the execution of the Example class
# it generates and submits the final test reports
class TrunkAnalyticsListener
  def initialize
    @testreport = TestReport.new('rspec')
  end

  def example_finished(notification)
    add_test_case(notification.example)
  end

  def close(_notification)
    puts @testreport.save
    puts @testreport.to_s
  end

  def description_generated?(example)
    auto_generated_exp = /^\sis expected to eq .*$/
    full_description = example.full_description
    parent_description = example.example_group.description
    checked_description = full_description.sub(parent_description, '')
    auto_generated_exp.match(checked_description) != nil
  end

  def generate_id(example)
    "#{example.id}-#{example.location}" if description_generated?(example)
    ''
  end

  # trunk-ignore(rubocop/Metrics/AbcSize,rubocop/Metrics/MethodLength)
  def add_test_case(example)
    if example.exception
      failure_message = example.exception.message
      # failure details is far more robust than the message, but noiser
      # if example.exception.backtrace
      # failure_details = example.exception.backtrace.join('\n')
      # end
    end
    # TODO: should we use concatenated string or alias when auto-generated description?
    name = example.full_description
    file = escape(example.metadata[:file_path])
    classname = file.sub(%r{\.[^/.]+\Z}, '').gsub('/', '.').gsub(/\A\.+|\.+\Z/, '')
    line = example.metadata[:line_number]
    started_at = example.execution_result.started_at.to_i
    finished_at = example.execution_result.finished_at.to_i
    id = generate_id(example)

    attempts = example.metadata[:attempts] || 0
    status = example.execution_result.status.to_s
    case example.execution_result.status
    when :passed
      status = Status.new('success')
    when :failed
      status = Status.new('failure')
    when :pending
      status = Status.new('skipped')
    end
    @testreport.add_test(id, name, classname, file, example.example_group.description, line, status, attempts,
                         started_at, finished_at, failure_message || '')
  end
end

RSpec.configure do |c|
  c.reporter.register_listener TrunkAnalyticsListener.new, :example_finished, :close
end

RSpec::Trunk.setup