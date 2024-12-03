require_relative '../lib/context_ruby'

describe 'context_ruby' do
  it 'should have access to bundle repo' do
    env_vars = {
        "GITHUB_ACTIONS"=>"true",
        "GITHUB_REF"=>"abc",
        "GITHUB_ACTOR"=>"Spikey",
        "GITHUB_REPOSITORY"=>"analytics-cli",
        "GITHUB_RUN_ID"=>"12345",
        "GITHUB_WORKFLOW"=>"test-workflow",
        "GITHUB_JOB"=>"test-job",
    }
    parsed = env_parse(env_vars)
  end
end
