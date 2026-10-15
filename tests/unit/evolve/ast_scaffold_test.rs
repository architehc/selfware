use super::*;

#[test]
fn test_validate_rust_syntax_accepts_valid_rust() {
    let valid_code = r#"
        pub struct Worker {
            pub id: usize,
            pub name: String,
        }

        impl Worker {
            pub fn new(id: usize, name: impl Into<String>) -> Self {
                Self { id, name: name.into() }
            }
        }
    "#;

    assert!(validate_rust_syntax(valid_code).is_ok());
}

#[test]
fn test_validate_rust_syntax_rejects_broken_syntax_with_coordinates() {
    let broken_code = r#"
        pub struct Worker {
            pub id: usize,
            pub name: String
        // Missing closing brace!
        fn broken() {}
    "#;

    let res = validate_rust_syntax(broken_code);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(err.line.is_some_and(|l| l > 0));
    assert!(err.column.is_some_and(|c| c > 0));
    assert!(err.to_string().contains("at line"));
    assert!(!err.message.is_empty());
}
