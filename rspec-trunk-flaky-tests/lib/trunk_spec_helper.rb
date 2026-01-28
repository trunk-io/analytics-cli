# frozen_string_literal: true

# Trunk RSpec Helper
#
# This helper integrates Trunk Flaky Tests with RSpec to automatically
# quarantine flaky tests and upload test results.
#
# Required environment variables:
#   TRUNK_ORG_URL_SLUG - Your organization's URL slug
#   TRUNK_API_TOKEN - Your API token for authentication
#
# Optional environment variables for repository metadata:
#   TRUNK_REPO_ROOT - Path to repository root
#   TRUNK_REPO_URL - Repository URL (e.g., https://github.com/org/repo.git)
#   TRUNK_REPO_HEAD_SHA - HEAD commit SHA
#   TRUNK_REPO_HEAD_BRANCH - HEAD branch name
#   TRUNK_REPO_HEAD_COMMIT_EPOCH - HEAD commit timestamp (seconds since epoch)
#   TRUNK_REPO_HEAD_AUTHOR_NAME - HEAD commit author name
#
# Optional environment variables for configuration:
#   TRUNK_CODEOWNERS_PATH - Path to CODEOWNERS file
#   TRUNK_VARIANT - Variant name for test results (e.g., 'linux', 'pr-123')
#   TRUNK_DISABLE_QUARANTINING - Set to 'true' to disable quarantining
#   TRUNK_ALLOW_EMPTY_TEST_RESULTS - Set to 'true' to allow empty results
#   TRUNK_DRY_RUN - Set to 'true' to save bundle locally instead of uploading
#   TRUNK_USE_UNCLONED_REPO - Set to 'true' for uncloned repo mode
#   TRUNK_LOCAL_UPLOAD_DIR - Directory to save test results locally (disables upload)
#   TRUNK_QUARANTINED_TESTS_DISK_CACHE_TTL_SECS - Time to cache quarantined tests on disk (in seconds)
#   DISABLE_RSPEC_TRUNK_FLAKY_TESTS - Set to 'true' to completely disable Trunk
#
require 'rspec/core'
require 'time'
require 'rspec_trunk_flaky_tests'

# String is an override to the main String class that is used to colorize the output
# it is used to make the output more readable
class String
  def colorize(color_code)
    "\e[#{color_code}m#{self}\e[0m"
  end

  def red
    colorize(31)
  end

  def green
    colorize(32)
  end

  def yellow
    colorize(33)
  end
end

def escape(str)
  str.dump[1..-2]
end

# Knapsack example detector instantiates all test cases in order to determine how to shard them
# These instantiations should not generate test bundles, so we
# disable the gem when running under knapsack_pro:rspec_test_example_detector
def knapsack_detector_mode?
  knapsack_detector_command?
end

def knapsack_detector_command?
  command_line = "#{$PROGRAM_NAME} #{ARGV.join(' ')}".strip
  # donotland
  # command_line.include?('knapsack_pro:rspec_test_example_detector')
  if command_line.include?('knapsack_pro:rspec_test_example_detector')
    puts "Detected knapsack example detector command: #{command_line}\n\n".yellow
    true
  else
    puts "Detected knapsack other command: #{command_line}\n\n".yellow
    false
  end
end

def trunk_disabled
  knapsack_detector_mode? || ENV['DISABLE_RSPEC_TRUNK_FLAKY_TESTS'] == 'true' ||
    ENV['TRUNK_ORG_URL_SLUG'].nil? || ENV['TRUNK_API_TOKEN'].nil?
end

# we want to cache the test report in memory so we can add to it as we go and reduce the number of API calls
$test_report = TestReport.new('rspec', "#{$PROGRAM_NAME} #{ARGV.join(' ')}", nil)
$failure_encountered_and_quarantining_disabled = false

module RSpec
  module Core
    # Example is the class that represents a test case
    class Example
      # keep the original method around so we can call it
      alias set_exception_core set_exception
      alias assign_generated_description_core assign_generated_description
      # RSpec uses the existance of an exception to determine if the test failed
      # We need to override this to allow us to capture the exception and then
      # decide if we want to fail the test or not
      # trunk-ignore(rubocop/Naming/AccessorMethodName)
      def set_exception(exception)
        return set_exception_core(exception) if metadata[:pending]
        return set_exception_core(exception) if trunk_disabled
        return set_exception_core(exception) if metadata[:retry_attempts]&.positive?

        handle_quarantine_check(exception)
      end

      # trunk-ignore(rubocop/Metrics/AbcSize,rubocop/Metrics/MethodLength)
      def handle_quarantine_check(exception)
        id = generate_trunk_id
        name = full_description
        parent_name = example_group.metadata[:description]
        parent_name = parent_name.empty? ? 'rspec' : parent_name
        file = escape(metadata[:file_path])
        classname = file.sub(%r{\.[^/.]+\Z}, '').gsub('/', '.').gsub(/\A\.+|\.+\Z/, '')
        unless $failure_encountered_and_quarantining_disabled
          puts "Test failed, checking if it can be quarantined: `#{location}`".yellow
        end
        is_quarantined_result = $test_report.is_quarantined(id, name, parent_name, classname, file)
        if is_quarantined_result.quarantining_disabled_for_repo
          unless $failure_encountered_and_quarantining_disabled
            puts 'Quarantining is disabled for this repo, no failures will be quarantined'.yellow
            $failure_encountered_and_quarantining_disabled = true
          end
          set_exception_core(exception)
        elsif is_quarantined_result.test_is_quarantined
          # monitor the override in the metadata
          metadata[:quarantined_exception] = exception
          puts "Test is quarantined, overriding exception: #{exception}".green
          nil
        else
          puts 'Test is not quarantined, continuing'.red
          set_exception_core(exception)
        end
      end

      def assign_generated_description
        metadata[:is_description_generated] = description_generated?
        assign_generated_description_core
      end

      def description_generated?
        return metadata[:is_description_generated] unless metadata[:is_description_generated].nil?

        description == location_description
      end

      def generate_trunk_id
        return "trunk:#{id}-#{location}" if description_generated?
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
        if trunk_disabled
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

  # trunk-ignore(rubocop/Metrics/MethodLength)
  def close(_notification)
    if $failure_encountered_and_quarantining_disabled
      puts 'Note: Quarantining is disabled for this repo. Test failures were not quarantined.'.yellow
    end
    if ENV['TRUNK_LOCAL_UPLOAD_DIR']
      saved = @testreport.try_save(ENV['TRUNK_LOCAL_UPLOAD_DIR'])
      if saved
        puts 'Local Flaky tests report generated'.green
      else
        puts 'Failed to generate local flaky tests report'.red
      end
    else
      published = @testreport.publish
      if published
        puts 'Flaky tests report upload complete'.green
      else
        puts 'Failed to publish flaky tests report'.red
      end
    end
  end

  # trunk-ignore(rubocop/Metrics/CyclomaticComplexity,rubocop/Metrics/AbcSize,rubocop/Metrics/MethodLength)
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
    id = example.generate_trunk_id

    attempt_number = example.metadata[:retry_attempts] || example.metadata[:attempt_number] || 0
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
  next if trunk_disabled

  c.reporter.register_listener TrunkAnalyticsListener.new, :example_finished, :close
end

RSpec::Trunk.setup
