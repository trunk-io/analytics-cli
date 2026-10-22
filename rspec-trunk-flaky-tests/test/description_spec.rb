# frozen_string_literal: true

require 'rspec_trunk_flaky_tests'
require_relative '../spec/spec_helper'
require 'rspec/core/sandbox'

# Test names are built from description_args so that objects described by their
# memory address get the same name on every run.
#
# trunk-ignore(rubocop/Metrics/BlockLength)
RSpec.describe 'test names' do
  # The examples of the group defined (and run) by the block, in a sandbox.
  def examples_of(&block)
    examples = nil
    RSpec::Core::Sandbox.sandboxed do |_config|
      group = block.call
      group.run(RSpec::Core::NullReporter)
      examples = group.descendants.flat_map(&:examples)
    end
    examples
  end

  def names_of(&block)
    examples_of(&block).map { |example| RSpec::Trunk.full_description(example) }
  end

  it "matches RSpec's full_description when no arg has an address" do
    examples = examples_of do
      RSpec.describe(Integer) do
        describe('#run', :tag) do
          context('with overtime') { it('pays 1.5x') { expect(1).to eq(1) } }
        end
        describe('.build') { it('builds') { expect(1).to eq(1) } }
        describe(:symbol, 'and a string') { it('works') { expect(1).to eq(1) } }
        it { expect(2).to eq(2) }
      end
    end

    expect(examples.map { |e| RSpec::Trunk.full_description(e) }).to eq(examples.map(&:full_description))
    expect(examples.map { |e| RSpec::Trunk.full_description(e) }).to include(
      'Integer#run with overtime pays 1.5x',
      'Integer.build builds',
      'Integer is expected to eq 2'
    )
  end

  it 'names an anonymous class by its superclass, the same on every run' do
    define = lambda do
      names_of do
        RSpec.describe('Employee') do
          describe(Class.new(StandardError)) { it('returns the bars') { expect(1).to eq(1) } }
        end
      end
    end

    expect(define.call).to eq(['Employee #<Class < StandardError> returns the bars'])
    expect(define.call).to eq(define.call)
  end

  it 'names an object without its own #to_s by its class' do
    batch_class = Class.new { def self.name = 'Sidekiq::Batch' }
    names = names_of do
      RSpec.describe(batch_class.new) { it('enqueues jobs') { expect(1).to eq(1) } }
    end

    expect(names).to eq(['#<Sidekiq::Batch> enqueues jobs'])
  end

  it 'keeps an object with its own #to_s' do
    tax = Class.new { def to_s = 'SC SUTA' }
    names = names_of { RSpec.describe(tax.new) { it('is exempt') { expect(1).to eq(1) } } }

    expect(names).to eq(['SC SUTA is exempt'])
  end

  it 'checks #to_s even when the object redefines #method' do
    request = Class.new do
      def self.name = 'Request'
      def method = 'GET'
    end
    names = names_of { RSpec.describe(request.new) { it('routes') { expect(1).to eq(1) } } }

    expect(names).to eq(['#<Request> routes'])
  end

  it 'leaves strings alone, even with an address interpolated into them' do
    batch = Object.new
    names = names_of do
      RSpec.describe("parent batch is #{batch}") { it('enqueues jobs') { expect(1).to eq(1) } }
    end

    expect(names).to eq(["parent batch is #{batch} enqueues jobs"])
  end

  it 'builds the parent name the same way' do
    examples = examples_of do
      RSpec.describe('Worker') { describe(Object.new) { it('works') { expect(1).to eq(1) } } }
    end

    expect(RSpec::Trunk.parent_name(examples.first)).to eq('#<Object>')
  end
end
