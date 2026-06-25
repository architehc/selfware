pub fn add(a: i64, b: i64) -> i64 {
    // Add two numbers
    a + b
}

/// Subtracts the second number from the first.
pub fn subtract(a: i64, b: i64) -> i64 {
    a - b
}

/// Multiplies two numbers.
pub fn multiply(a: i64, b: i64) -> i64 {
    a * b
}

/// Divides the first number by the second, returning None if dividing by zero.
pub fn divide(a: i64, b: i64) -> Option<i64> {
    if b == 0 {
        None
    } else {
        Some(a / b)
    }
}

/// Raises a base to an exponent.
pub fn pow(base: i64, exp: u32) -> i64 {
    base.pow(exp)
}
