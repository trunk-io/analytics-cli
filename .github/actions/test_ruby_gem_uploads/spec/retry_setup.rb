require 'rspec/retryable'

class SimpleRetryHandler
  MAX_RETRIES = 2

  def initialize
    # Initialization code here
    @retries = Hash.new(0)
  end

  def call(payload)
    # use payload to set retry or not based on RSpec example state

    if @retries[payload.example.id] < MAX_RETRIES
      if payload.state == :failed
        @retries[payload.example.id] += 1
        puts "Retrying #{payload.example.id} (attempt #{@retries[payload.example.id]}/#{MAX_RETRIES})"
        payload.retry = true
      else
        yield
      end
    else
      puts "Not retrying #{payload.example.id} #{@retries[payload.example.id]} times"
      # Pass down to next handler
      yield
    end
  end
end

RSpec::Retryable.bind

RSpec::Retryable.handlers.register(SimpleRetryHandler)