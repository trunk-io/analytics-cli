# frozen_string_literal: true

require 'rspec_trunk_flaky_tests'
require_relative '../spec/spec_helper'
require_relative 'support/location_helpers'
require 'rails/version'
require 'rswag/specs'
require 'rspec/core/sandbox'

# trunk-ignore(rubocop/Metrics/BlockLength)
RSpec.describe 'reported test location' do
  spec_file = './test/location_spec.rb'
  helpers_file = './test/support/location_helpers.rb'
  rswag_file = Rswag::Specs::ExampleGroupHelpers.instance_method(:run_test!).source_location.first
  lines = {}

  # trunk-ignore(rubocop/Metrics/AbcSize,rubocop/Metrics/MethodLength)
  define_method(:locations) do
    examples = nil
    RSpec::Core::Sandbox.sandboxed do |_config|
      group = RSpec.describe('API') do
        extend Rswag::Specs::ExampleGroupHelpers
        shared_examples('shared', &LocationHelpers::SHARED_EXAMPLES)
        lines[:own] = __LINE__ + 1
        it('is its own') { expect(1).to eq(1) }
        lines[:it_behaves_like] = __LINE__ + 1
        it_behaves_like 'shared'
        context 'included' do
          lines[:include_examples] = __LINE__ + 1
          include_examples 'shared'
        end
        path('/users') do
          get('lists users') do
            lines[:response] = __LINE__ + 1
            response('200', 'ok') { run_test! }
          end
        end
      end
      group.run(RSpec::Core::NullReporter)
      examples = group.descendants.flat_map(&:examples)
    end
    examples.to_h { |example| [example.full_description, RSpec::Trunk.location(example)] }
  end

  around do |example|
    previous = ENV.fetch('TRUNK_REPORT_SPEC_LOCATION', nil)
    ENV['TRUNK_REPORT_SPEC_LOCATION'] = report_spec_location
    example.run
  ensure
    ENV['TRUNK_REPORT_SPEC_LOCATION'] = previous
  end

  context 'with TRUNK_REPORT_SPEC_LOCATION' do
    let(:report_spec_location) { 'true' }

    it 'reports each example at the spec line that brought it in' do
      expect(locations).to eq(
        'API is its own' => [spec_file, lines[:own]],
        'API behaves like shared is shared' => [spec_file, lines[:it_behaves_like]],
        'API included is shared' => [spec_file, lines[:include_examples]],
        'API /users get ok returns a 200 response' => [spec_file, lines[:response]]
      )
    end

    it 'derives the classname from the spec file' do
      example = nil
      RSpec::Core::Sandbox.sandboxed do |_config|
        group = RSpec.describe('API') do
          extend Rswag::Specs::ExampleGroupHelpers
          path('/users') { get('lists users') { response('200', 'ok') { run_test! } } }
        end
        example = group.descendants.flat_map(&:examples).first
      end
      expect(RSpec::Trunk.file_and_classname(example)).to eq([spec_file, 'test.location_spec'])
    end
  end

  context 'without it' do
    let(:report_spec_location) { nil }

    it 'reports each example where it is defined' do
      expect(locations.values.map(&:first)).to eq([spec_file, helpers_file, helpers_file, rswag_file])
      expect(locations['API is its own']).to eq([spec_file, lines[:own]])
    end
  end
end
