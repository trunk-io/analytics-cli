# frozen_string_literal: true

require_relative '../lib/context_ruby'
require_relative '../spec/spec_helper'

# trunk-ignore(rubocop/Metrics/BlockLength)
describe do
  it 'should be able to env_parse' do
    env_vars = {
      'GITHUB_ACTIONS' => 'true',
      'GITHUB_REF' => 'abc',
      'GITHUB_ACTOR' => 'Spikey',
      'GITHUB_REPOSITORY' => 'analytics-cli',
      'GITHUB_RUN_ID' => '12345',
      'GITHUB_WORKFLOW' => 'test-workflow',
      'GITHUB_JOB' => 'test-job'
    }
    parsed = env_parse(env_vars)
    expect(parsed.platform.to_s).to eq('GITHUB_ACTIONS')
    expect(parsed.job_url).to eq('https://github.com/analytics-cli/actions/runs/12345')
    expect(parsed.branch).to eq('abc')
    expect(parsed.actor).to eq('Spikey')
    expect(parsed.workflow).to eq('test-workflow')
    expect(parsed.job).to eq('test-job')
  end

  it 'should be able to make a new CIInfo' do
    ci = CIInfo.new(1)
    expect(ci.platform.to_s).to eq('BUILD_ID')
    expect(ci.job_url).to eq(nil)
    expect(ci.branch).to eq(nil)
    expect(ci.actor).to eq(nil)
    expect(ci.workflow).to eq(nil)
    expect(ci.job).to eq(nil)
  end

  it 'should error on invalid CIInfo' do
    expect { CIInfo.new(100) }.to raise_error(TypeError)
  end
end
