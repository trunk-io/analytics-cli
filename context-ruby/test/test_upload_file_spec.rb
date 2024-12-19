# frozen_string_literal: true

require_relative '../lib/context_ruby'
require_relative '../spec/spec_helper'

describe do
  # intentionally no description here
  it do
    # generate a random number to be injected into the description
    now = Time.now.to_i
    expect(now).to eq(now)
  end
end
