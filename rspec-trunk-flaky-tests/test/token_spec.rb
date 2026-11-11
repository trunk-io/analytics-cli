# frozen_string_literal: true

require 'open3'
require 'rbconfig'
require_relative '../spec/spec_helper'

# trunk-ignore(rubocop/Metrics/BlockLength)
RSpec.describe 'TRUNK_API_TOKEN' do
  let(:probe) do
    <<~'RUBY'
      require 'rspec_trunk_flaky_tests'
      require 'trunk_spec_helper'
      puts RSpec::Trunk.current_run ? 'enabled' : 'disabled'
    RUBY
  end

  let(:lib_dir) { File.dirname($LOAD_PATH.resolve_feature_path('trunk_spec_helper').last) }

  def load_gem(token)
    env = {
      'TRUNK_ORG_URL_SLUG' => 'token-spec',
      'TRUNK_API_TOKEN' => token,
      'TRUNK_TEST_COLLECTION_ID' => 'aB3xY9kQ',
      'DISABLE_RSPEC_TRUNK_FLAKY_TESTS' => nil
    }
    Open3.capture3(env, RbConfig.ruby, '-I', lib_dir, '-e', probe)
  end

  ['0123456789abcdef0123456789abcdef', 'a.b', ''].each do |token|
    it "disables Trunk and says why for an organization token like #{token.inspect}" do
      stdout, stderr, status = load_gem(token)

      expect(status).to be_success, stderr
      expect(stdout.lines.last.strip).to eq('disabled')
      expect(stderr).to include('Trunk Flaky Tests is disabled: This Trunk token is not a v2 API key.')
      expect(stderr).to include('Settings → Developer → API Keys')
      expect(stderr).to include('Upload test results')
      expect(stderr).to include('pin analytics-cli 0.17.x')
    end
  end

  ['trunk_v2_AbC123', 'eyJhbGciOiJSUzI1NiJ9.eyJleHAiOjF9.c2ln'].each do |token|
    it "enables Trunk for #{token.inspect}" do
      stdout, stderr, status = load_gem(token)

      expect(status).to be_success, stderr
      expect(stdout.lines.last.strip).to eq('enabled')
      expect(stderr).not_to include('Trunk Flaky Tests is disabled')
    end
  end
end
