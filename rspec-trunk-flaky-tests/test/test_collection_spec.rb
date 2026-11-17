# frozen_string_literal: true

require 'open3'
require 'rbconfig'
require 'tmpdir'
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

  def env(test_collection_id)
    {
      'TRUNK_ORG_URL_SLUG' => 'test-collection-spec',
      'TRUNK_API_TOKEN' => 'trunk_v2_test-collection-spec',
      'TRUNK_TEST_COLLECTION_ID' => test_collection_id,
      'DISABLE_RSPEC_TRUNK_FLAKY_TESTS' => nil
    }
  end

  def load_gem(test_collection_id, extra_env = {})
    Open3.capture3(env(test_collection_id).merge(extra_env), RbConfig.ruby, '-I', lib_dir, '-e', probe)
  end

  [nil, '', ' '].each do |test_collection_id|
    it "refuses to load, and says why, when it is #{test_collection_id.inspect}" do
      stdout, stderr, status = load_gem(test_collection_id)

      expect(status).not_to be_success
      expect(stdout).to be_empty
      expect(stderr).to include('RSpec::Trunk::ConfigurationError')
      expect(stderr).to include('Trunk Flaky Tests: this run names no test collection. Set TRUNK_TEST_COLLECTION_ID')
      expect(stderr).to include('To run without Trunk, set DISABLE_RSPEC_TRUNK_FLAKY_TESTS=true.')
    end
  end

  it 'fails an rspec run before any example runs' do
    Dir.mktmpdir do |dir|
      spec_file = File.join(dir, 'probe_spec.rb')
      File.write(spec_file, <<~'RUBY')
        require 'trunk_spec_helper'
        RSpec.describe('probe') { it('runs') { puts 'EXAMPLE RAN' } }
      RUBY
      rspec = Gem.bin_path('rspec-core', 'rspec')
      stdout, stderr, status = Open3.capture3(env(''), RbConfig.ruby, '-I', lib_dir, rspec, spec_file)

      expect(status).not_to be_success
      expect(stdout).not_to include('EXAMPLE RAN')
      expect(stdout + stderr).to include('this run names no test collection')
    end
  end

  it 'stays off quietly when Trunk is turned off' do
    stdout, stderr, status = load_gem('', 'DISABLE_RSPEC_TRUNK_FLAKY_TESTS' => 'true')

    expect(status).to be_success, stderr
    expect(stdout.lines.last.strip).to eq('disabled')
    expect(stderr).not_to include('Trunk Flaky Tests')
  end

  it 'enables Trunk when it is set' do
    stdout, stderr, status = load_gem('aB3xY9kQ')

    expect(status).to be_success, stderr
    expect(stdout.lines.last.strip).to eq('enabled')
    expect(stderr).not_to include('Trunk Flaky Tests')
  end
end
