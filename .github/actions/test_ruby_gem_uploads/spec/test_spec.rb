# frozen_string_literal: true

def square(val)
  val * val
end

describe 'simple_test' do
  [1, 2, 3].each do |i|
    it do
      expect(square(i)).to eq(i * i)
    end
  end
end

describe 'random_test' do
  it do
    a = Random.rand(11)
    expect(a).to be <= 10
    expect(a).to eq(a)
  end
end
