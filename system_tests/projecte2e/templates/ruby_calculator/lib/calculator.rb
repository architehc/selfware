# A simple calculator class
class Calculator
  # Add two numbers
  # @param a [Integer] First number
  # @param b [Integer] Second number
  # @return [Integer] Sum
  def add(a, b)
    a + b
  end

  # Subtract b from a
  # @param a [Integer] First number
  # @param b [Integer] Second number
  # @return [Integer] Difference
  def subtract(a, b)
    a - b
  end

  # Multiply two numbers
  # @param a [Integer] First number
  # @param b [Integer] Second number
  # @return [Integer] Product
  def multiply(a, b)
    a * b
  end

  # Divide a by b
  # @param a [Integer] Dividend
  # @param b [Integer] Divisor
  # @return [Float, nil] Quotient or nil if b is 0
  def divide(a, b)
    return nil if b == 0
    a.to_f / b
  end
end
