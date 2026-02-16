use easy_calculator::{add, divide, multiply, pow, subtract};

#[test]
fn basic_arithmetic_works() {
    assert_eq!(add(2, 3), 5);
    assert_eq!(subtract(10, 4), 6);
    assert_eq!(multiply(7, 6), 42);
}

#[test]
fn divide_handles_zero_safely() {
    assert_eq!(divide(9, 3), Some(3));
    assert_eq!(divide(5, 0), None);
}

#[test]
fn pow_handles_common_exponents() {
    assert_eq!(pow(2, 0), 1);
    assert_eq!(pow(2, 1), 2);
    assert_eq!(pow(2, 10), 1024);
    assert_eq!(pow(-3, 3), -27);
}
