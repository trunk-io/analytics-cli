# frozen_string_literal: true

require_relative '../lib/rspec_trunk_flaky_tests'
require_relative '../spec/spec_helper'

# trunk-ignore(rubocop/Metrics/BlockLength)
RSpec.describe 'RSpec Expectations' do
  it do
    now = Time.now.to_i
    expect(now).to eq(now)
  end

  it 'verified 42 == 42' do
    number = 42
    expect(number).to eq(42)
  end

  it do
    number = rand(1..100)
    expect(number).to eq(number)
  end

  it do
    number = rand(1..100)
    expect(number).not_to eq(number + 1)
  end

  it do
    number = rand(1..100)
    expect(number).to be > number - 1
  end

  it do
    number = rand(1..100)
    expect(number).to be < number + 1
  end

  it do
    array = [rand(1..100), rand(1..100), rand(1..100)]
    array.should include(array[0])
  end

  it do
    array = [rand(1..100), rand(1..100), rand(1..100)]
    array.should_not include(101)
  end
end
