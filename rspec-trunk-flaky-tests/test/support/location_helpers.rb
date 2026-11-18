# frozen_string_literal: true

# Defines examples from a file other than the spec, as shared examples and rswag's run_test! do.
module LocationHelpers
  SHARED_EXAMPLES = proc { it('is shared') { expect(1).to eq(1) } }

  def run_test!
    it('returns a 200 response') { expect(1).to eq(1) }
  end
end
