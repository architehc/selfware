require 'minitest/autorun'
require_relative '../lib/calculator'

# Tests for Calculator class
class TestCalculator < Minitest::Test
  def setup
    @calc = Calculator.new
  end

  def test_add
    assert_equal 5, @calc.add(2, 3)
    assert_equal 0, @calc.add(-1, 1)
    assert_equal 0, @calc.add(0, 0)
  end

  def test_subtract
    assert_equal 2, @calc.subtract(5, 3)
    assert_equal(-5, @calc.subtract(0, 5))
    assert_equal 0, @calc.subtract(10, 10)
  end

  def test_multiply
    assert_equal 12, @calc.multiply(3, 4)
    assert_equal(-6, @calc.multiply(-2, 3))
    assert_equal 0, @calc.multiply(0, 100)
  end

  def test_divide
    assert_equal 5.0, @calc.divide(10, 2)
    assert_equal 2.5, @calc.divide(5, 2)
    assert_nil @calc.divide(10, 0)
  end
end
