# frozen_string_literal: true

# A fixed-pending example (body now passes -> PendingExampleFixedError) reds the
# build unless quarantined: it fails without a variant and passes (quarantined)
# when run with the smoke-test-variant variant.
describe 'pending_quarantine_test' do
  it 'should be quarantined when run with variant' do
    pending('expected to still fail, but the body now passes')
    expect(2 + 2).to eq(4)
  end
end
