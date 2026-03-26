#include <gtest/gtest.h>
#include "calculator.hpp"

TEST(CalculatorTest, Add) {
    Calculator calc;
    EXPECT_EQ(calc.add(2, 3), 5);
    EXPECT_EQ(calc.add(-1, 1), 0);
    EXPECT_EQ(calc.add(0, 0), 0);
}

TEST(CalculatorTest, Subtract) {
    Calculator calc;
    EXPECT_EQ(calc.subtract(5, 3), 2);
    EXPECT_EQ(calc.subtract(0, 5), -5);
    EXPECT_EQ(calc.subtract(10, 10), 0);
}

TEST(CalculatorTest, Multiply) {
    Calculator calc;
    EXPECT_EQ(calc.multiply(3, 4), 12);
    EXPECT_EQ(calc.multiply(-2, 3), -6);
    EXPECT_EQ(calc.multiply(0, 100), 0);
}

TEST(CalculatorTest, Divide) {
    Calculator calc;
    EXPECT_DOUBLE_EQ(calc.divide(10, 2), 5.0);
    EXPECT_DOUBLE_EQ(calc.divide(7, 2), 3.5);
    EXPECT_DOUBLE_EQ(calc.divide(10, 0), 0.0);
}
