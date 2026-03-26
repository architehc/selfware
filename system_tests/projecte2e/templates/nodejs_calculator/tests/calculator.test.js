import { describe, it, expect } from 'vitest';
import { add, subtract, multiply, divide } from '../src/calculator.js';

describe('calculator', () => {
    it('adds numbers', () => {
        expect(add(2, 3)).toBe(5);
        expect(add(-1, 1)).toBe(0);
    });

    it('subtracts numbers', () => {
        expect(subtract(5, 3)).toBe(2);
        expect(subtract(0, 5)).toBe(-5);
    });

    it('multiplies numbers', () => {
        expect(multiply(3, 4)).toBe(12);
        expect(multiply(-2, 3)).toBe(-6);
    });

    it('divides numbers', () => {
        expect(divide(10, 2)).toBe(5);
        expect(divide(10, 0)).toBeNull();
    });
});
