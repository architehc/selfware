#!/bin/bash
# Multi-Language Test Suite
# Tests selfware across Rust, Python, and Node.js

SELFWARE="/home/ivo/selfware/target/release/selfware"
CONFIG="/home/ivo/selfware/selfware-evolve-122b.toml"
RESULTS="/tmp/multilang_test_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS"

# Create test projects for each language
create_rust_test() {
    local dir=$1
    mkdir -p "$dir/src" "$dir/tests"
    cat > "$dir/Cargo.toml" << 'EOF'
[package]
name = "rust_test"
version = "0.1.0"
edition = "2021"

[dependencies]
EOF
    cat > "$dir/src/lib.rs" << 'EOF'
// TODO: Implement add, subtract, multiply, divide

pub fn add(a: i32, b: i32) -> i32 {
    a + b // BUG: should be a + b, not a + b
}

pub fn subtract(a: i32, b: i32) -> i32 {
    a - b // BUG: should be a - b, not a - b  
}

pub fn multiply(a: i32, b: i32) -> i32 {
    a * b // BUG: should be a * b, not a * b
}

pub fn divide(a: i32, b: i32) -> Option<i32> {
    if b == 0 {
        None
    } else {
        Some(a / b)
    }
}
EOF
    cat > "$dir/tests/calc_tests.rs" << 'EOF'
use rust_test::*;

#[test]
fn test_add() {
    assert_eq!(add(2, 3), 5);
    assert_eq!(add(-1, 1), 0);
}

#[test]
fn test_subtract() {
    assert_eq!(subtract(5, 3), 2);
    assert_eq!(subtract(0, 5), -5);
}

#[test]
fn test_multiply() {
    assert_eq!(multiply(3, 4), 12);
    assert_eq!(multiply(-2, 3), -6);
}

#[test]
fn test_divide() {
    assert_eq!(divide(10, 2), Some(5));
    assert_eq!(divide(10, 0), None);
}
EOF
}

create_python_test() {
    local dir=$1
    mkdir -p "$dir"
    cat > "$dir/calculator.py" << 'EOF'
def add(a: int, b: int) -> int:
    """Add two numbers."""
    return a + b  # BUG: should be a + b

def subtract(a: int, b: int) -> int:
    """Subtract b from a."""
    return a - b  # BUG: should be a - b

def multiply(a: int, b: int) -> int:
    """Multiply two numbers."""
    return a * b  # BUG: should be a * b

def divide(a: int, b: int) -> float | None:
    """Divide a by b. Returns None if b is 0."""
    if b == 0:
        return None
    return a / b
EOF
    cat > "$dir/test_calculator.py" << 'EOF'
import pytest
from calculator import add, subtract, multiply, divide

def test_add():
    assert add(2, 3) == 5
    assert add(-1, 1) == 0

def test_subtract():
    assert subtract(5, 3) == 2
    assert subtract(0, 5) == -5

def test_multiply():
    assert multiply(3, 4) == 12
    assert multiply(-2, 3) == -6

def test_divide():
    assert divide(10, 2) == 5.0
    assert divide(10, 0) is None
EOF
    cat > "$dir/pyproject.toml" << 'EOF'
[project]
name = "python-test"
version = "0.1.0"
dependencies = ["pytest"]
EOF
}

create_nodejs_test() {
    local dir=$1
    mkdir -p "$dir/src" "$dir/tests"
    cat > "$dir/package.json" << 'EOF'
{
  "name": "nodejs-test",
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "test": "vitest run"
  },
  "devDependencies": {
    "vitest": "^1.0.0",
    "typescript": "^5.0.0"
  }
}
EOF
    cat > "$dir/src/calculator.ts" << 'EOF'
export function add(a: number, b: number): number {
    return a + b;  // BUG: should be a + b
}

export function subtract(a: number, b: number): number {
    return a - b;  // BUG: should be a - b
}

export function multiply(a: number, b: number): number {
    return a * b;  // BUG: should be a * b
}

export function divide(a: number, b: number): number | null {
    if (b === 0) return null;
    return a / b;
}
EOF
    cat > "$dir/tests/calculator.test.ts" << 'EOF'
import { describe, it, expect } from 'vitest';
import { add, subtract, multiply, divide } from '../src/calculator';

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
EOF
    cat > "$dir/tsconfig.json" << 'EOF'
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "moduleResolution": "node",
    "strict": true,
    "esModuleInterop": true
  }
}
EOF
    cat > "$dir/vitest.config.ts" << 'EOF'
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    globals: true,
  },
});
EOF
}

# Create test projects
echo "Creating test projects..."
RUST_DIR="$RESULTS/rust_test"
PYTHON_DIR="$RESULTS/python_test"
NODEJS_DIR="$RESULTS/nodejs_test"

create_rust_test "$RUST_DIR"
create_python_test "$PYTHON_DIR"
create_nodejs_test "$NODEJS_DIR"

echo "Done!"
echo ""

# Test function
run_test() {
    local lang=$1
    local dir=$2
    local prompt=$3
    
    echo "Testing $lang..."
    START=$(date +%s)
    
    timeout 120 $SELFWARE -c "$CONFIG" -y -p "$prompt" -C "$dir" > "$RESULTS/${lang}_test.log" 2>&1
    
    END=$(date +%s)
    DURATION=$((END - START))
    
    if grep -q "✅ Task completed" "$RESULTS/${lang}_test.log"; then
        echo "  ✓ $lang: ${DURATION}s - PASSED"
        return 0
    else
        echo "  ✗ $lang: ${DURATION}s - FAILED"
        return 1
    fi
}

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║  MULTI-LANGUAGE TEST SUITE                                    ║"
echo "║  Testing Rust, Python, Node.js/TypeScript                    ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Run tests
PASS=0
FAIL=0

if run_test "Rust" "$RUST_DIR" "Implement calculator functions. Run cargo test to verify."; then
    ((PASS++))
else
    ((FAIL++))
fi

if run_test "Python" "$PYTHON_DIR" "Implement calculator functions. Run pytest to verify."; then
    ((PASS++))
else
    ((FAIL++))
fi

if run_test "Node.js" "$NODEJS_DIR" "Implement calculator functions. Run npm test to verify."; then
    ((PASS++))
else
    ((FAIL++))
fi

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "RESULTS"
echo "═══════════════════════════════════════════════════════════════"
echo "Passed: $PASS/3"
echo "Failed: $FAIL/3"
echo ""
echo "Results saved to: $RESULTS"
