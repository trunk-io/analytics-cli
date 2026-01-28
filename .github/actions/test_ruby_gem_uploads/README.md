# Running Ruby Tests Locally

If you want to locally run the ruby smoke tests that are run in CI, run:

From `cd rspec-trunk-flaky-tests`:

1. `cd rspec-trunk-flaky-tests`
2. `bundle install`
3. `bundle exec rake build`
4. `cd ../.github/actions/test_ruby_gem_uploads`
5. Add `gem 'rspec_trunk_flaky_tests', :path => '../../../rspec-trunk-flaky-tests'` to [Gemfile](./Gemfile)
6. Run `bundle exec rspec spec/variant_quarantine_spec.rb --format documentation`

After `cd .github/actions/test_ruby_gem_uploads`

See more in the [rspec README.md](../../../rspec-trunk-flaky-tests/README.md).

## Knapsack Pro

Repeat steps 1-5 above, and run:

1. `KNAPSACK_PRO_CI_NODE_BUILD_ID=$(openssl rand -base64 32)`
2. `KNAPSACK_PRO_TEST_DIR=spec KNAPSACK_PRO_TEST_FILE_PATTERN="**/*.rb" KNAPSACK_PRO_PROJECT_DIR=. KNAPSACK_PRO_REPOSITORY_ADAPTER=git KNAPSACK_PRO_LOG_LEVEL=debug KNAPSACK_PRO_TEST_SUITE_TOKEN_RSPEC="<api-key>" KNAPSACK_PRO_CI_NODE_TOTAL=1 KNAPSACK_PRO_CI_NODE_INDEX=0 KNAPSACK_PRO_FIXED_QUEUE_SPLIT=false bundle exec rake "knapsack_pro:queue:rspec"`

### Reference

- [More information about Queue Mode](https://docs.knapsackpro.com/ruby/queue-mode/)
