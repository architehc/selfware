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

#[test]
fn multi_line_fn_signature_is_captured_in_full() {
    let src = "pub fn foo(\n    x: usize,\n    y: usize,\n) -> usize {\n    x + y\n}\n";
    let skel = extract_rust_skeleton(Path::new("t.rs"), src);
    let sig = skel
        .items
        .iter()
        .find_map(|item| match item {
            SkeletonItem::Function {
                name, signature, ..
            } if name == "foo" => Some(signature),
            _ => None,
        })
        .expect("foo must be captured");
    assert!(
        sig.contains("x: usize") && sig.contains("y: usize") && sig.contains("-> usize"),
        "multi-line signature must include continuation lines, got: {sig}"
    );
    assert!(
        !sig.contains("x + y"),
        "body must not leak into the signature"
    );
}

#[test]
fn multi_line_signature_is_capped_at_eight_lines() {
    let src = "fn wide(\n    a: usize,\n    b: usize,\n    c: usize,\n    d: usize,\n    e: usize,\n    f: usize,\n    g: usize,\n    h: usize,\n    i: usize,\n) {}\n";
    let skel = extract_rust_skeleton(Path::new("t.rs"), src);
    let sig = skel
        .items
        .iter()
        .find_map(|item| match item {
            SkeletonItem::Function {
                name, signature, ..
            } if name == "wide" => Some(signature),
            _ => None,
        })
        .expect("wide must be captured");
    // 8-line cap: the declaration line plus at most 7 continuation lines —
    // the unbalanced tail must not swallow the rest of the file.
    assert!(
        !sig.contains("i: usize"),
        "signature capture must stop at the 8-line cap, got: {sig}"
    );
}

#[test]
fn scoped_visibility_prefixes_are_recognized() {
    let src = "pub(in crate::inner) fn scoped_fn() {}\n\
               pub(crate) struct ScopedStruct;\n\
               pub(in crate::inner) enum ScopedEnum { A }\n\
               pub(super) trait ScopedTrait {}\n\
               pub(crate) const SCOPED_CONST: usize = 1;\n\
               pub(crate) use std::fmt;\n\
               pub(in crate::inner) mod scoped_mod;\n";
    let skel = extract_rust_skeleton(Path::new("t.rs"), src);
    let has = |pred: fn(&SkeletonItem) -> bool| skel.items.iter().any(pred);
    assert!(has(
        |i| matches!(i, SkeletonItem::Function { name, .. } if name == "scoped_fn")
    ));
    assert!(has(
        |i| matches!(i, SkeletonItem::Struct { name, .. } if name == "ScopedStruct")
    ));
    assert!(has(
        |i| matches!(i, SkeletonItem::Enum { name, .. } if name == "ScopedEnum")
    ));
    assert!(has(
        |i| matches!(i, SkeletonItem::Trait { name, .. } if name == "ScopedTrait")
    ));
    assert!(has(
        |i| matches!(i, SkeletonItem::Const { name, .. } if name == "SCOPED_CONST")
    ));
    assert!(has(
        |i| matches!(i, SkeletonItem::Use { path, .. } if path.starts_with("pub(crate) use"))
    ));
    assert!(has(
        |i| matches!(i, SkeletonItem::Module { name, .. } if name == "scoped_mod")
    ));
}

#[test]
fn plain_visibility_prefixes_still_work() {
    let src = "pub fn a() {}\npub(crate) fn b() {}\npub(super) async fn c() {}\nfn d() {}\n";
    let skel = extract_rust_skeleton(Path::new("t.rs"), src);
    let names: Vec<&str> = skel
        .items
        .iter()
        .filter_map(|i| match i {
            SkeletonItem::Function { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(names, ["a", "b", "c", "d"]);
}
