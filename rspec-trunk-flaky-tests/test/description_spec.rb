# frozen_string_literal: true

require 'rspec_trunk_flaky_tests'
require_relative '../spec/spec_helper'
require 'rspec/core/sandbox'
require 'timeout'

# Defines .name for something else, as some apps do.
class DescriptionSpecBuilder
  class << self
    private

    def name(_key, _value) = raise('not a class name')
  end
end

DescriptionSpecLabeled = Class.new { def self.name = 'Payroll Builder' }
DescriptionSpecUnnamed = Class.new { def self.name = '' }

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

  it 'names a class whose .name is private and takes arguments as RSpec does' do
    names = names_of do
      RSpec.describe('Factory') do
        describe(DescriptionSpecBuilder) { it('builds') { expect(1).to eq(1) } }
        describe(DescriptionSpecBuilder.new) { it('is built') { expect(1).to eq(1) } }
        describe(Class.new(DescriptionSpecBuilder)) { it('is a subclass') { expect(1).to eq(1) } }
      end
    end

    expect(names).to eq([
                          'Factory DescriptionSpecBuilder builds',
                          'Factory #<DescriptionSpecBuilder> is built',
                          'Factory #<Class < DescriptionSpecBuilder> is a subclass'
                        ])
  end

  it 'names a class whose .name returns something other than a String by its anonymous name' do
    tagged = Class.new(StandardError) { def self.name = :tagged }
    names = names_of { RSpec.describe(tagged) { it('works') { expect(1).to eq(1) } } }

    expect(names).to eq(['#<Class < StandardError> works'])
  end

  it 'names a class whose .name is abstract' do
    abstract = Class.new { def self.name = raise(NotImplementedError) }
    names = names_of { RSpec.describe(abstract) { it('works') { expect(1).to eq(1) } } }

    expect(names).to eq(['#<Class < Object> works'])
  end

  it 'does not ask a class whether it is a Module' do
    typed = Class.new do
      def self.name = 'Typed'
      def self.is_a?(*) = raise('not a type check')
    end
    names = names_of { RSpec.describe('Lookup') { describe(typed) { it('works') { expect(1).to eq(1) } } } }

    expect(names).to eq(['Lookup Typed works'])
  end

  it "uses core's #class and .superclass when they are overridden" do
    proxy = Class.new do
      def self.name = 'Proxy'
      def class = raise('not a class')
    end
    looped = Class.new(StandardError)
    looped.define_singleton_method(:superclass) { looped }
    names = names_of do
      RSpec.describe('Lookup') do
        describe(proxy.new) { it('forwards') { expect(1).to eq(1) } }
        describe(looped) { it('terminates') { expect(1).to eq(1) } }
      end
    end

    expect(names).to eq(['Lookup #<Proxy> forwards', 'Lookup #<Class < StandardError> terminates'])
  end

  it 'names a class as RSpec does when that has no address, whatever its own .name says' do
    priced = Class.new { def self.to_s = 'Money (USD)' }
    lying = Class.new do
      def self.to_s = 'Lying'
      def self.is_a?(type) = type == Module ? false : super
    end
    examples = examples_of do
      RSpec.describe('Payroll') do
        describe(priced, '#run') { it('runs') { expect(1).to eq(1) } }
        describe(lying, '#run') { it('runs') { expect(1).to eq(1) } }
        describe(DescriptionSpecLabeled) { it('runs') { expect(1).to eq(1) } }
        describe(DescriptionSpecUnnamed) { it('runs') { expect(1).to eq(1) } }
        describe(Integer.singleton_class) { it('runs') { expect(1).to eq(1) } }
      end
    end

    expect(examples.map { |e| RSpec::Trunk.full_description(e) }).to eq(examples.map(&:full_description))
    expect(examples.map(&:full_description)).to eq([
                                                     'Payroll Money (USD)#run runs',
                                                     'Payroll Lying #run runs',
                                                     'Payroll DescriptionSpecLabeled runs',
                                                     'Payroll DescriptionSpecUnnamed runs',
                                                     'Payroll #<Class:Integer> runs'
                                                   ])
  end

  # Kinds of describe arg, including ones whose own .name, .to_s, .is_a?, #class or
  # .superclass behave unusually.
  let(:describe_args) do
    priced = Class.new { def self.to_s = 'Money (USD)' }
    lying = Class.new do
      def self.to_s = 'Lying'
      def self.is_a?(type) = type == Module ? false : super
    end
    looped = Class.new(StandardError)
    looped.define_singleton_method(:superclass) { looped }
    abstract = Class.new
    abstract.define_singleton_method(:name) { raise NotImplementedError }
    [
      [Integer], [Comparable], [Integer, '#to_s'], [Integer, '.sqrt'], [Integer, '::Inner'], [Comparable, 'with x'],
      %w[a b], [:sym], [42], [1..2], [[1, 2]], ['', Integer], [Integer.singleton_class],
      [DescriptionSpecBuilder], [DescriptionSpecBuilder, '#build'], [DescriptionSpecLabeled], [DescriptionSpecUnnamed],
      [priced], [priced, '#run'], [lying, '#run'], [looped], [abstract],
      [Class.new(StandardError) { def self.name = :tagged }], [Object.new], [Object.new.singleton_class], [Module.new],
      [Class.new { def class = raise('not a class') }.new]
    ]
  end

  # RSpec's full_description and ours for the args at the top level, nested in a group, and
  # with a group nested under them.
  def names_with_rspec(args)
    shapes = [
      -> { RSpec.describe(*args) { it('runs') {} } },
      -> { RSpec.describe('Outer') { describe(*args) { it('runs') {} } } },
      -> { RSpec.describe(*args) { describe('#child') { it('runs') {} } } }
    ]
    shapes.flat_map { |shape| examples_of(&shape) }.map { |e| [e.full_description, trunk_name(e)] }
  end

  def trunk_name(example)
    Timeout.timeout(5) { RSpec::Trunk.full_description(example) }
  end

  it "names every kind of arg as RSpec does when RSpec's name has no address" do
    aggregate_failures do
      describe_args.each do |args|
        names_with_rspec(args).each do |rspec, ours|
          next if rspec.match?(/0x\h+/)

          expect(ours).to eq(rspec), "#{args.inspect}: expected #{rspec.inspect}, got #{ours.inspect}"
        end
      end
    end
  end

  it 'names every kind of arg without an address, and without raising or hanging' do
    aggregate_failures do
      describe_args.each do |args|
        names_with_rspec(args).map(&:last).each { |ours| expect(ours).not_to match(/0x\h+/), args.inspect }
      end
    end
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
