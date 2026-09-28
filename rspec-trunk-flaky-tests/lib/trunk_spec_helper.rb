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
#   TRUNK_PR_NUMBER - PR number, if uploading from a PR (normally inferred from CI environment variables)
#
# Optional environment variables for configuration:
#   TRUNK_CODEOWNERS_PATH - Path to CODEOWNERS file
#   TRUNK_TEST_COLLECTION_ID - Optional 8 character alphanumeric ID for a test collection
#   TRUNK_VARIANT - Variant name for test results (e.g., 'linux', 'pr-123')
#   TRUNK_DISABLE_QUARANTINING - Set to 'true' to disable quarantining
#   TRUNK_ALLOW_EMPTY_TEST_RESULTS - Set to 'true' to allow empty results
#   TRUNK_DRY_RUN - Set to 'true' to save bundle locally instead of uploading
#   TRUNK_USE_UNCLONED_REPO - Set to 'true' for uncloned repo mode
#   TRUNK_LOCAL_UPLOAD_DIR - Directory to save test results locally (disables upload)
#   TRUNK_QUARANTINED_TESTS_DISK_CACHE_TTL_SECS - Time to cache quarantined tests on disk (in seconds)
#   TRUNK_QUARANTINE_QUERY_FAILURE_EXIT - Set to 'true' to abort the RSpec run when quarantine
#     lookup fails (remaining examples are skipped)
#   DISABLE_RSPEC_TRUNK_FLAKY_TESTS - Set to 'true' to completely disable Trunk
#
require 'rspec/core'
require 'rspec/core/formatters/exception_presenter'
require 'time'
require 'rspec_trunk_flaky_tests'

module RSpec
  # Trunk holds everything this gem defines (the native extension defines the
  # module and its classes: TestReport, Status, CIInfo, ...). Nothing is added to
  # the global namespace or to core classes; test/namespace_spec.rb enforces that.
  module Trunk
    ANSI_ESCAPE_PATTERN = %r{(?:\e[@-Z\\-_]|\e\[[0-?]*[ -/]*[@-~])}

    # Knapsack example detector instantiates all test cases in order to determine how to shard them
    # These instantiations should not generate test bundles, so we
    # disable the gem when running under knapsack_pro:rspec_test_example_detector
    KNAPSACK_DETECTOR_COMMANDS = %w[
      knapsack_pro:rspec_test_example_detector
      knapsack_pro:queue:rspec:initialize
    ].freeze

    # Example metadata keys this gem sets, each mapped to the unprefixed name it
    # used to be set under. The old names are deprecated: they are still written
    # (see .write_metadata) so existing readers keep working, but will be removed
    # in a future release.
    DEPRECATED_METADATA_KEYS = {
      trunk_quarantined_exception: :quarantined_exception,
      trunk_attempt_number: :attempt_number,
      trunk_is_description_generated: :is_description_generated
    }.freeze

    class << self
      # The state for this RSpec process, or nil when Trunk is disabled.
      attr_reader :current_run

      def setup
        return if current_run || disabled?

        run = @current_run = Run.new
        RSpec::Core::Example.prepend(ExampleExtension)
        RSpec.configure do |config|
          config.before(:example) do
            skip('Quarantine lookup failed, skipping test run') if run.abort_remaining?
          end
          config.around(:each) { |example| Trunk.run_counting_attempts(example) }
          config.reporter.register_listener AnalyticsListener.new(run), :example_finished, :close
        end
      end

      def disabled?
        knapsack_detector_mode? || ENV['DISABLE_RSPEC_TRUNK_FLAKY_TESTS'] == 'true' ||
          ENV['TRUNK_ORG_URL_SLUG'].nil? || ENV['TRUNK_API_TOKEN'].nil?
      end

      def knapsack_detector_mode?
        KNAPSACK_DETECTOR_COMMANDS.any? { |command| command_line.include?(command) }
      end

      def quarantine_query_failure_exit?
        ENV['TRUNK_QUARANTINE_QUERY_FAILURE_EXIT'] == 'true'
      end

      def command_line
        "#{$PROGRAM_NAME} #{ARGV.join(' ')}"
      end

      def run_counting_attempts(example)
        example.run
        # monitor attempts in the metadata
        attempt_number = example.metadata[:trunk_attempt_number]
        write_metadata(example.metadata, :trunk_attempt_number, attempt_number ? attempt_number + 1 : 0)
      end

      # Sets a trunk_ metadata key, and its deprecated unprefixed name too.
      def write_metadata(metadata, key, value)
        metadata[key] = value
        deprecated_key = DEPRECATED_METADATA_KEYS[key]
        metadata[deprecated_key] = value if deprecated_key
      end

      def escape(str)
        str.dump[1..-2]
      end

      def strip_ansi_codes(text)
        text.to_s.gsub(ANSI_ESCAPE_PATTERN, '')
      end

      # The escaped file path and the dotted classname derived from it.
      def file_and_classname(example)
        file = escape(example.metadata[:file_path])
        [file, file.sub(%r{\.[^/.]+\Z}, '').gsub('/', '.').gsub(/\A\.+|\.+\Z/, '')]
      end

      def parent_name(example)
        name = example.example_group.metadata[:description]
        name.empty? ? 'rspec' : name
      end
    end

    # ANSI colors for the gem's console output, without patching String.
    module Colors
      module_function

      def colorize(text, color_code)
        "\e[#{color_code}m#{text}\e[0m"
      end

      def red(text)
        colorize(text, 31)
      end

      def green(text)
        colorize(text, 32)
      end

      def yellow(text)
        colorize(text, 33)
      end
    end

    # Run is the state shared by every example in this RSpec process: the test
    # report (cached in memory so we can add to it as we go and reduce the number
    # of API calls), and whether quarantining turned out to be unavailable, so
    # that's only announced once.
    class Run
      attr_reader :report

      def initialize(report = TestReport.new('rspec', Trunk.command_line, nil))
        @report = report
        @quarantining_disabled = false
        @lookup_failed = false
      end

      def quarantining_disabled?
        @quarantining_disabled
      end

      def lookup_failed?
        @lookup_failed
      end

      def abort_remaining?
        lookup_failed? && Trunk.quarantine_query_failure_exit?
      end

      # Whether a failure of `example` with `exception` is quarantined. If the
      # quarantine machinery itself blows up, the failure must stand.
      def quarantine?(example, exception)
        check_quarantine(example, exception)
      rescue StandardError => e
        puts Colors.yellow("Quarantine check errored (#{e.class}: #{e.message}), treating test as not quarantined")
        false
      end

      private

      # trunk-ignore(rubocop/Metrics/AbcSize,rubocop/Metrics/MethodLength)
      def check_quarantine(example, exception)
        file, classname = Trunk.file_and_classname(example)
        unless quarantining_disabled?
          puts Colors.yellow("Test failed, checking if it can be quarantined: `#{example.location}`")
        end
        result = report.is_quarantined(example.trunk_id, example.full_description, Trunk.parent_name(example),
                                       classname, file)

        if result.quarantine_lookup_failed
          unless lookup_failed?
            puts Colors.yellow('Failed to check quarantining status, no failures will be quarantined')
            @lookup_failed = true
          end
          if Trunk.quarantine_query_failure_exit?
            puts Colors.red('Quarantine lookup failed, exiting early')
            RSpec.world.wants_to_quit = true
          end
          false
        elsif result.quarantining_disabled_for_repo
          unless quarantining_disabled?
            puts Colors.yellow('Quarantining is disabled for this repo, no failures will be quarantined')
            @quarantining_disabled = true
          end
          false
        elsif result.test_is_quarantined
          # monitor the override in the metadata
          Trunk.write_metadata(example.metadata, :trunk_quarantined_exception, exception)
          puts Colors.green("Test is quarantined, overriding exception: #{exception}")
          true
        else
          puts Colors.red('Test is not quarantined, continuing')
          false
        end
      end
    end

    # Prepended to RSpec::Core::Example. RSpec uses the existence of an exception
    # to determine if the test failed, so #set_exception is where we capture the
    # exception and decide whether to fail the test or not.
    module ExampleExtension
      # trunk-ignore(rubocop/Naming/AccessorMethodName)
      def set_exception(exception)
        return super unless quarantine_candidate?(exception)
        return super unless Trunk.current_run&.quarantine?(self, exception)

        nil
      end

      def trunk_id
        "trunk:#{id}-#{location}" if trunk_description_generated?
      end

      private

      def assign_generated_description
        Trunk.write_metadata(metadata, :trunk_is_description_generated, trunk_description_generated?)
        super
      end

      def trunk_description_generated?
        cached = metadata[:trunk_is_description_generated]
        return cached unless cached.nil?

        description == location_description
      end

      def quarantine_candidate?(exception)
        # Pending stays green, except a fixed pending example is a real failure.
        return false if metadata[:pending] && !pending_example_fixed?(exception)

        !metadata[:retry_attempts]&.positive?
      end

      def pending_example_fixed?(exception)
        defined?(RSpec::Core::Pending::PendingExampleFixedError) &&
          exception.is_a?(RSpec::Core::Pending::PendingExampleFixedError)
      end
    end

    # Formats failures for the report as plain text suitable for storage and the web UI.
    module FailureFormatter
      # A no-op colorizer passed to RSpec's ExceptionPresenter.
      module PlainColorizer
        module_function

        def wrap(text, _code_or_symbol)
          text
        end
      end

      module_function

      # Defer to RSpec's own ExceptionPresenter so the failure_message field matches
      # what users see in their RSpec console output (Failure/Error: <source line>,
      # the exception class and message, and any "Caused by:" chain).
      def message(exception, example)
        return '' unless exception

        presenter = RSpec::Core::Formatters::ExceptionPresenter.new(exception, example)
        Trunk.strip_ansi_codes(presenter.fully_formatted(nil, PlainColorizer))
      rescue StandardError
        legacy_message(exception)
      end

      # trunk-ignore(rubocop/Metrics/MethodLength,rubocop/Metrics/AbcSize)
      def backtrace(exception, example)
        return '' unless exception

        lines = backtrace_lines(exception, example)

        cause = exception.cause
        depth = 0
        while cause && depth < 10
          lines << ''
          lines << "Caused by: #{cause.class}: #{cause.message}"
          lines.concat(backtrace_lines(cause, example))
          cause = cause.cause
          depth += 1
        end

        result = lines.join("\n")
        # The exception presenter may choke on MultipleExceptionError, such as errors in before
        # and after hooks, so we fall back to the legacy formatter
        return legacy_backtrace(exception) if result.strip.empty?

        Trunk.strip_ansi_codes(result)
      rescue StandardError
        legacy_backtrace(exception)
      end

      def backtrace_lines(exception, example)
        presenter = RSpec::Core::Formatters::ExceptionPresenter.new(exception, example)
        Array(presenter.formatted_backtrace)
      rescue StandardError
        Array(exception.backtrace)
      end

      def legacy_message(exception)
        case exception
        when RSpec::Core::MultipleExceptionError
          messages = exception.all_exceptions.map { |e| "#{e.class}: #{e.message}" }
          Trunk.strip_ansi_codes("#{exception.class}: #{messages.join(' | ')}")
        else
          Trunk.strip_ansi_codes(exception.to_s)
        end
      end

      # trunk-ignore(rubocop/Metrics/MethodLength,rubocop/Metrics/AbcSize)
      def legacy_backtrace(exception)
        case exception
        when RSpec::Core::MultipleExceptionError
          Trunk.strip_ansi_codes(exception.all_exceptions.map do |e|
            if e.backtrace && !e.backtrace.empty?
              "#{e.class}: #{e.message}\n#{e.backtrace.join("\n")}"
            else
              "#{e.class}: #{e.message}"
            end
          end.join("\n\n"))
        else
          Trunk.strip_ansi_codes(exception.backtrace&.join("\n") || '')
        end
      end
    end

    # AnalyticsListener is an RSpec reporter listener that records each finished
    # example in the run's test report and submits the report at the end.
    class AnalyticsListener
      MAX_TEXT_FIELD_SIZE = 8_000

      def initialize(run)
        @run = run
      end

      def example_finished(notification)
        add_test_case(notification.example)
      end

      # trunk-ignore(rubocop/Metrics/MethodLength,rubocop/Metrics/AbcSize)
      def close(_notification)
        if @run.quarantining_disabled?
          puts Colors.yellow('Note: Quarantining is disabled for this repo. Test failures were not quarantined.')
        end
        if @run.lookup_failed?
          puts Colors.yellow('Note: Failed to check quarantining status. Test failures were not quarantined.')
        end

        if ENV['TRUNK_LOCAL_UPLOAD_DIR']
          saved = @run.report.try_save(ENV['TRUNK_LOCAL_UPLOAD_DIR'])
          if saved
            puts Colors.green('Local Flaky tests report generated')
          else
            puts Colors.red('Failed to generate local flaky tests report')
          end
        else
          published = @run.report.publish
          if published
            puts Colors.green('Flaky tests report upload complete')
          else
            puts Colors.red('Failed to publish flaky tests report')
          end
        end
      end

      # trunk-ignore(rubocop/Metrics/AbcSize,rubocop/Metrics/MethodLength)
      def add_test_case(example)
        status, exception = status_and_exception(example)
        failure_message = ''
        backtrace = ''
        if exception
          failure_message = FailureFormatter.message(exception, example).strip
          backtrace = FailureFormatter.backtrace(exception, example).strip
        end
        failure_message = failure_message[0...MAX_TEXT_FIELD_SIZE] if failure_message.length > MAX_TEXT_FIELD_SIZE
        backtrace = backtrace[0...MAX_TEXT_FIELD_SIZE] if backtrace.length > MAX_TEXT_FIELD_SIZE
        # TODO: should we use concatenated string or alias when auto-generated description?
        name = example.full_description
        file, classname = Trunk.file_and_classname(example)
        line = example.metadata[:line_number]
        started_at = example.execution_result.started_at.to_i
        finished_at = example.execution_result.finished_at.to_i

        attempt_number = example.metadata[:retry_attempts] || example.metadata[:trunk_attempt_number] || 0
        # set the status to failure, but mark it as quarantined
        is_quarantined = example.metadata[:trunk_quarantined_exception] ? true : false
        @run.report.add_test(example.trunk_id, name, classname, file, Trunk.parent_name(example), line, status,
                             attempt_number, started_at, finished_at, failure_message || '', backtrace || '',
                             is_quarantined)
      end

      # Determine the Trunk status to report for an example, plus the exception (if
      # any) whose message/backtrace should be recorded for it.
      #
      # `pending` examples need care because RSpec expresses their outcome as the
      # inverse of the body's pass/fail. A pending example's contract is "this is
      # expected to fail", so:
      #   - body still fails  -> expectation met     -> RSpec status :pending, build green
      #   - body now passes   -> expectation violated -> RSpec status :failed
      #                          (PendingExampleFixedError), build red
      # We report the outcome RSpec actually decided, which also matches the build
      # result:
      #   - skip / xit (never ran)     -> skipped
      #   - pending, body still failing -> success (expectation met)
      #   - pending, body now passing   -> failure (PendingExampleFixedError)
      # RSpec's own build pass/fail behavior is left untouched (see #set_exception).
      #
      # trunk-ignore(rubocop/Metrics/AbcSize,rubocop/Metrics/CyclomaticComplexity,rubocop/Metrics/MethodLength)
      def status_and_exception(example)
        result = example.execution_result
        quarantined_exception = example.metadata[:trunk_quarantined_exception]

        # A genuinely skipped example (`skip`/`xit`, or `skip` called from a hook or
        # body) never runs, so it has no real pass/fail outcome. RSpec still reports
        # it with status :pending, so detect skips explicitly -- and up front -- via
        # metadata[:skip] before interpreting any pending pass/fail semantics below.
        return [Status.new('skipped'), nil] if example.metadata[:skip]

        case result.status
        when :passed
          # A quarantined failure is recorded as :passed by RSpec (see #set_exception),
          # so report it as a failure but carry the original quarantined exception.
          quarantined_exception ? [Status.new('failure'), quarantined_exception] : [Status.new('success'), nil]
        when :failed
          # Includes a pending example whose body unexpectedly passed: RSpec reports it
          # as :failed with a PendingExampleFixedError (on example.exception) and breaks
          # the build (the pending expectation was violated), so it is a failure here too.
          [Status.new('failure'), example.exception || quarantined_exception]
        when :pending
          # A quarantined fixed-pending is left :pending but pending_fixed? -- still a
          # failure. Otherwise the pending "should fail" expectation was met -> success.
          if result.pending_fixed?
            [Status.new('failure'), quarantined_exception || example.exception]
          else
            [Status.new('success'), nil]
          end
        else
          [Status.new(result.status.to_s), example.exception || quarantined_exception]
        end
      end
    end
  end
end

RSpec::Trunk.setup
