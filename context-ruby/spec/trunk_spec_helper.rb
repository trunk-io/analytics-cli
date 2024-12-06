require 'rspec/core'
require 'time'
require 'context_ruby/context_ruby'

def escape(str)
  str.dump[1..-2]
end

module RSpec
  module Core
    class Example
      class Procsy
        def run_with_trunk(testreport = nil)
          RSpec::Trunk.new(self, testreport).run
        end
      end
    end
  end
end

module RSpec
  class Trunk
    def self.setup
      testreport = TestReport.new()
      RSpec.configure do |config|
        # clear the testreport between contexts
        config.append_before(:all) do
          testreport = TestReport.new()
        end
        config.around(:example) do |ex|
          ex.run_with_trunk(testreport)
        end
        config.append_after(:all) do
          testreport.publish
          puts testreport.to_s
        end
      end
    end

    attr_reader :context, :ex

    def initialize(example, testreport = TestReport.new())
      @example = example
      @testreport = testreport
    end

    def run
      # run the test
      @example.run
      # add the test to the report
      add_test_case
      # update the report
      override_result
      # return the report
      @testreport
    end

    # TODO
    def generate_id
      "#{@example.example.id}-#{@example.example.location}"
    end

    def override_result
      # TODO check if quarantined
      # override to success if quarantined
    end

    def add_test_case
      if @example.exception
        failure_message = @example.exception.message
        failure_details = ''
        # failure_details = @example.example.exception.backtrace.join("\n")
      end
      # TODO - sanitize the name to detect if description is auto generated
      # if it is then use the rspec provided id and location, stripping the auto generated pieces out
      # if it isn't then use the full description
      name = @example.example.full_description
      file = escape(@example.example.metadata[:file_path])
      classname = file.sub(%r{\.[^/.]+\Z}, '').gsub('/', '.').gsub(/\A\.+|\.+\Z/, '')
      line = @example.example.location
      start_time = @example.example.execution_result.started_at.to_i
      end_time = @example.example.execution_result.finished_at.to_i

      # TODO - we need to track this and not rely on the example group
      if @example.example_group.instance_variable_defined?(:@retry_attempts)
        attempt = @example.example_group.retry_attempts
      else
        attempt = 0
      end
      @testreport.add_test(name, classname, file, @example.example_group.description, line.to_i,
                           failure_message ? 'failed' : 'passed', attempt, start_time, end_time, failure_message || '')
    end
  end
end

RSpec::Trunk.setup
