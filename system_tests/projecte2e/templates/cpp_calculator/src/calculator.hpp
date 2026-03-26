#pragma once

/**
 * @brief A simple calculator class
 */
class Calculator {
public:
    /**
     * @brief Add two numbers
     * @param a First number
     * @param b Second number
     * @return Sum of a and b
     */
    int add(int a, int b);

    /**
     * @brief Subtract b from a
     * @param a First number
     * @param b Second number
     * @return Difference (a - b)
     */
    int subtract(int a, int b);

    /**
     * @brief Multiply two numbers
     * @param a First number
     * @param b Second number
     * @return Product of a and b
     */
    int multiply(int a, int b);

    /**
     * @brief Divide a by b
     * @param a Dividend
     * @param b Divisor
     * @return Quotient, or 0 if b is 0
     */
    double divide(int a, int b);
};
