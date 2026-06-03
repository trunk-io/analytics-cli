# frozen_string_literal: true

describe 'slow_test' do
  it 'passes after a 30 second delay' do
    sleep 30
    expect(true).to be true
  end
end
