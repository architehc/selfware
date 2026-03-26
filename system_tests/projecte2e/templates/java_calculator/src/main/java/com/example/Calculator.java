package com.example;

/**
 * A simple calculator class.
 */
public class Calculator {

    /**
     * Adds two numbers.
     * @param a First number
     * @param b Second number
     * @return Sum of a and b
     */
    public int add(int a, int b) {
        return a + b;
    }

    /**
     * Subtracts b from a.
     * @param a First number
     * @param b Second number
     * @return Difference (a - b)
     */
    public int subtract(int a, int b) {
        return a - b;
    }

    /**
     * Multiplies two numbers.
     * @param a First number
     * @param b Second number
     * @return Product of a and b
     */
    public int multiply(int a, int b) {
        return a * b;
    }

    /**
     * Divides a by b.
     * @param a Dividend
     * @param b Divisor
     * @return Quotient (a / b)
     * @throws ArithmeticException if b is zero
     */
    public double divide(int a, int b) {
        if (b == 0) {
            throw new ArithmeticException("Division by zero");
        }
        return (double) a / b;
    }
}