# frozen_string_literal: true

# trunk-ignore(rubocop/Metrics/BlockLength)
describe 'multiple_exception_error_test' do
  context 'with before hook error and test failure' do
    before do
      raise StandardError, 'Error in before hook'
    end

    it 'test that would also fail' do
      expect(true).to eq(false)
    end
  end

  context 'with aggregate_failures creating multiple exceptions' do
    it 'raises multiple exceptions' do
      aggregate_failures do
        expect(1).to eq(2)
        expect(3).to eq(4)
        expect(5).to eq(6)
      end
    end
  end

  context 'with before and after hook failures' do
    before do
      raise StandardError, 'Error in before hook'
    end

    after do
      raise StandardError, 'Error in after hook'
    end

    it 'test execution with hook errors' do
      expect(true).to eq(true)
    end
  end

  context 'nested context with multiple failures' do
    it 'multiple expectations that fail' do
      aggregate_failures 'checking multiple conditions' do
        expect(1).to eq(2)
        expect('a').to eq('b')
        expect([1, 2]).to eq([3, 4])
        raise 'Additional error in test'
      end
    end
  end
end
