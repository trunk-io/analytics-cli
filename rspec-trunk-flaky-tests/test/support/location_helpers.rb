# frozen_string_literal: true

# Shared examples defined in a file other than the spec that includes them.
module LocationHelpers
  SHARED_EXAMPLES = proc { it('is shared') { expect(1).to eq(1) } }
end
