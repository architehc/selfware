pub fn add(a: i64, b: i64) -> i64 {
    a + b
}

pub fn subtract(a: i64, b: i64) -> i64 {
    a - b
}

pub fn multiply(_a: i64, _b: i64) -> i64 {
    // BUG: intentionally wrong implementation
    0
}

pub fn divide(a: i64, b: i64) -> Option<i64> {
    // BUG: division by zero currently panics instead of returning None
    Some(a / b)
}

pub fn pow(base: i64, exp: u32) -> i64 {
    // BUG: this only handles exp==0 correctly
    if exp == 0 {
        1
    } else {
        base
    }
}
