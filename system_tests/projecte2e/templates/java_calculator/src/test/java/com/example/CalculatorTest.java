package com.example;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;
import java.util.ArithmeticException;

/**
 * Tests for Calculator class.
 */
public class CalculatorTest {

    private final Calculator calc = new Calculator();

    @Test
    void testAdd() {
        assertEquals(5, calc.add(2, 3));
        assertEquals(0, calc.add(-1, 1));
        assertEquals(0, calc.add(0, 0));
    }

    @Test
    void testSubtract() {
        assertEquals(2, calc.subtract(5, 3));
        assertEquals(-5, calc.subtract(0, 5));
        assertEquals(0, calc.subtract(10, 10));
    }

    @Test
    void testMultiply() {
        assertEquals(12, calc.multiply(3, 4));
        assertEquals(-6, calc.multiply(-2, 3));
        assertEquals(0, calc.multiply(0, 100));
    }

    @Test
    void testDivide() {
        assertEquals(5.0, calc.divide(10, 2));
        assertEquals(2.5, calc.divide(5, 2));
        assertThrows(ArithmeticException.class, () -> calc.divide(10, 0));
    }
}
