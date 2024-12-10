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
        def run_with_trunk(testreport)
          tr = RSpec::Trunk.new(self, testreport).run
        end
      end
    end
  end
end

module RSpec
  class Trunk
    def self.setup
      testreport = TestReport.new("rspec")
      RSpec.configure do |config|
        # clear the testreport between contexts
        config.append_before(:all) do
          testreport = TestReport.new("rspec")
        end
        config.around(:example) do |ex|
          ex.run_with_trunk(testreport)
        end
        config.append_after(:all) do |ex|
          puts testreport.save
          puts testreport.to_s
        end
      end
    end

    attr_reader :context, :ex

    def initialize(example, testreport)
      @example = example
      @testreport = testreport
    end

    require 'json'
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

    def is_description_generated
      auto_generated_exp = /^\sis expected to eq .*$/
      full_description = @example.example.full_description
      parent_description = @example.example_group.description
      checked_description = full_description.sub(parent_description, '')
      auto_generated_exp.match(checked_description) != nil
    end

    def generate_id
      if is_description_generated
        return "#{@example.example.id}-#{@example.example.location}"
      end

      ''
    end

    # TODO implement
    # check if quarantined
    def override_result
    end

    require 'json'
    def add_test_case
      # finished at and status are missing
      if @example.exception
        failure_message = @example.exception.message
        # failure details is far more robust than the message, but noiser
        failure_details = @example.example.exception.backtrace.join("\n")
      end
      # TODO - should we use concatenated string or alias when auto-generated description?
      name = @example.example.full_description
      file = escape(@example.example.metadata[:file_path])
      classname = file.sub(%r{\.[^/.]+\Z}, '').gsub('/', '.').gsub(/\A\.+|\.+\Z/, '')
      line = @example.example.location
      started_at = @example.example.execution_result.started_at.to_i
      finished_at = @example.example.metadata[@finished_at].to_i
      id = generate_id

      # TODO - we need to track this directly and not rely on the example group
      if @example.example_group.instance_variable_defined?(:@retry_attempts)
        attempt = @example.example_group.retry_attempts
      else
        attempt = 0
      end
      # TODO - status
      status = failure_message ? 'failure' : 'success'
      @testreport.add_test(id, name, classname, file, @example.example_group.description, line.to_i, status, attempt,
                           started_at, finished_at, failure_message || '')
    end
  end
end

RSpec::Trunk.setup
