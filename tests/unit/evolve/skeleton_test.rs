//! Unit tests for symbol-level source span extraction (`evolve::skeleton`).

use super::*;

#[test]
fn finds_function_span_including_body_and_closing_brace() {
    let src = "fn helper() {\n    let x = 1;\n    println!(\"{x}\");\n}\n\nfn other() {}\n";
    let (start, end) = extract_symbol_source(src, "helper").unwrap();
    assert_eq!(start, 1);
    assert_eq!(end, 4);
    let span: String = src
        .lines()
        .skip(start - 1)
        .take(end - start + 1)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(span.contains("println!"), "body must be inside the span");
    assert!(span.ends_with('}'), "span must end at the closing brace");
    assert!(!span.contains("other"));
}

#[test]
fn returns_none_for_unknown_symbol() {
    let src = "fn real() {}\npub struct S;\n";
    assert!(extract_symbol_source(src, "missing").is_none());
}

#[test]
fn one_line_const_spans_its_own_line() {
    let src = "pub const MAX: usize = 4;\nfn f() {}\n";
    assert_eq!(extract_symbol_source(src, "MAX"), Some((1, 1)));
}

#[test]
fn struct_block_and_braces_in_strings_dont_confuse_matching() {
    let src = "pub struct Cfg {\n    pub name: String,\n}\nfn after() {}\n";
    assert_eq!(extract_symbol_source(src, "Cfg"), Some((1, 3)));

    let src = "fn render() {\n    let s = \"}\";\n    println!(\"{s}\");\n}\nfn after() {}\n";
    assert_eq!(extract_symbol_source(src, "render"), Some((1, 4)));
}

#[test]
fn method_inside_impl_is_found_by_its_own_name() {
    let src = "struct A;\nimpl A {\n    fn run(&self) {\n        todo!()\n    }\n}\n";
    assert_eq!(extract_symbol_source(src, "run"), Some((3, 5)));
}
