# frozen_string_literal: true

describe 'variant_quarantine_test_alt' do
  it 'should be quarantined when run with variant' do
    # This test should fail when run without a variant (not quarantined)
    # and be quarantined (not fail) when run with a variant
    expect(2 + 2).to eq(5)
  end
end
