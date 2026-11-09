# frozen_string_literal: true

require 'open3'
require 'rbconfig'
require_relative '../spec/spec_helper'

# trunk-ignore(rubocop/Metrics/BlockLength)
RSpec.describe 'TRUNK_TEST_COLLECTION_ID' do
  let(:probe) do
    <<~'RUBY'
      require 'rspec_trunk_flaky_tests'
      require 'trunk_spec_helper'
      puts RSpec::Trunk.current_run ? 'enabled' : 'disabled'
    RUBY
  end

  let(:lib_dir) { File.dirname($LOAD_PATH.resolve_feature_path('trunk_spec_helper').last) }

  def load_gem(test_collection_id)
    env = {
      'TRUNK_ORG_URL_SLUG' => 'test-collection-spec',
      'TRUNK_API_TOKEN' => 'test-collection-spec',
      'TRUNK_TEST_COLLECTION_ID' => test_collection_id,
      'DISABLE_RSPEC_TRUNK_FLAKY_TESTS' => nil
    }
    Open3.capture3(env, RbConfig.ruby, '-I', lib_dir, '-e', probe)
  end

  [nil, '', ' '].each do |test_collection_id|
    it "disables Trunk and says why when it is #{test_collection_id.inspect}" do
      stdout, stderr, status = load_gem(test_collection_id)

      expect(status).to be_success, stderr
      expect(stdout.lines.last.strip).to eq('disabled')
      expect(stderr).to include('Trunk Flaky Tests is disabled: this run names no test collection. ' \
                                'Set TRUNK_TEST_COLLECTION_ID')
    end
  end

  it 'enables Trunk when it is set' do
    stdout, stderr, status = load_gem('aB3xY9kQ')

    expect(status).to be_success, stderr
    expect(stdout.lines.last.strip).to eq('enabled')
    expect(stderr).not_to include('Trunk Flaky Tests is disabled')
  end
end
