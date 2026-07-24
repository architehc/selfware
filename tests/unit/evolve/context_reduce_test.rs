use super::{dedup_context, reduce_source, strip_cfg_test_blocks, strip_comments};

#[test]
fn strips_line_comments_keeps_code() {
    let src = "let x = 1; // set x\nlet y = 2;\n";
    let out = strip_comments(src);
    assert!(out.contains("let x = 1;"));
    assert!(out.contains("let y = 2;"));
    assert!(!out.contains("set x"));
}

#[test]
fn strips_doc_and_block_comments() {
    let src = "/// docs\n/* block\n spanning */\npub fn f() {}\n";
    let out = strip_comments(src);
    assert!(out.contains("pub fn f() {}"));
    assert!(!out.contains("docs"));
    assert!(!out.contains("block"));
    assert!(!out.contains("spanning"));
}

#[test]
fn preserves_slashes_inside_string_literals() {
    let src = r#"let url = "http://x//y"; // real comment"#;
    let out = strip_comments(src);
    assert!(out.contains(r#""http://x//y""#), "string content must survive: {out}");
    assert!(!out.contains("real comment"));
}

#[test]
fn removes_blank_lines_left_behind() {
    let src = "// only a comment\nlet a = 1;\n// another\n";
    let out = strip_comments(src);
    assert_eq!(out, "let a = 1;\n");
}

#[test]
fn strips_inline_cfg_test_module() {
    let src = "pub fn add(a: i32) -> i32 { a + 1 }\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() { assert_eq!(add(1), 2); }\n}\npub fn done() {}\n";
    let out = strip_cfg_test_blocks(src);
    assert!(out.contains("pub fn add"));
    assert!(out.contains("pub fn done"));
    assert!(!out.contains("mod tests"), "test module removed: {out}");
    assert!(!out.contains("assert_eq"), "test body removed: {out}");
}

#[test]
fn reduce_source_drops_comments_and_tests() {
    let src = "/// doc\npub fn keep() {}\n#[cfg(test)]\nmod tests {\n    fn helper() {}\n}\n";
    let out = reduce_source(src);
    assert!(out.contains("pub fn keep"));
    assert!(!out.contains("doc"));
    assert!(!out.contains("tests"));
    // The reduced output must be strictly smaller.
    assert!(out.len() < src.len());
}

#[test]
fn dedup_elides_identical_body_keeps_canonical() {
    // Body must be comfortably larger than the stub (2x margin) to be elided.
    let body = "\n    let mut total = 0;\n    let mut count = 0;\n    for x in items {\n        if *x > 0 {\n            total += x;\n            count += 1;\n        } else {\n            total -= x;\n        }\n    }\n    if count > 0 { total / count } else { total }\n";
    let a = format!("pub fn sum_a(items: &[i32]) -> i32 {{{body}}}\n");
    let b = format!("pub fn sum_b(items: &[i32]) -> i32 {{{body}}}\n");
    let (out, elided) = dedup_context(&[("a.rs".into(), a.clone()), ("b.rs".into(), b)]);
    assert_eq!(elided, 1, "one duplicate body should be elided");
    // Canonical (first) file keeps the real body.
    assert!(out[0].1.contains("for x in items"));
    // Second file's body is replaced with a stub referencing the canonical.
    assert!(out[1].1.contains("deduped: identical to a.rs::sum_a"), "got: {}", out[1].1);
    assert!(!out[1].1.contains("for x in items"));
}

#[test]
fn dedup_skips_marginal_bodies_where_stub_would_not_shrink() {
    // Identical 3-line bodies that are smaller than 2x the stub stay inline.
    let body = "\n    let a = 1;\n    let b = 2;\n    a + b\n";
    let a = format!("pub fn f() -> i32 {{{body}}}\n");
    let b = format!("pub fn g() -> i32 {{{body}}}\n");
    let (out, elided) = dedup_context(&[("a.rs".into(), a), ("b.rs".into(), b)]);
    assert_eq!(elided, 0, "marginal elision would be net-negative in tokens");
    assert!(out[1].1.contains("a + b"));
}

#[test]
fn dedup_leaves_distinct_and_trivial_bodies_alone() {
    let a = "pub fn f() -> i32 {\n    let a = 1;\n    let b = 2;\n    a + b\n}\n";
    let b = "pub fn g() -> i32 {\n    let a = 1;\n    let b = 3;\n    a * b\n}\n";
    let (out, elided) = dedup_context(&[("a.rs".into(), a.into()), ("b.rs".into(), b.into())]);
    assert_eq!(elided, 0, "distinct bodies must not be deduped");
    assert!(out[1].1.contains("a * b"));

    // Trivial (short) identical bodies are left inline, not stubbed.
    let t = "pub fn one() -> i32 { 1 }\n";
    let (_out, n) = dedup_context(&[("a.rs".into(), t.into()), ("b.rs".into(), t.into())]);
    assert_eq!(n, 0);
}

#[test]
fn dedup_survives_multibyte_chars() {
    // Regression: byte-window keyword scan must not panic when the window lands
    // inside a multi-byte char (em-dash in a string literal).
    let src = "pub fn msg() -> &'static str {\n    \"reasoning — action — verify\"\n}\npub fn other() -> i32 {\n    let x = 1;\n    let y = 2;\n    x + y\n}\n";
    let (out, _) = dedup_context(&[("a.rs".into(), src.into())]);
    assert!(out[0].1.contains("reasoning — action — verify"));
}

#[test]
fn strip_cfg_test_handles_brace_in_string() {
    let src = "#[cfg(test)]\nmod tests {\n    let s = \"}\";\n    fn t() {}\n}\npub fn after() {}\n";
    let out = strip_cfg_test_blocks(src);
    // The `}` inside the string must not prematurely close the block, so `after`
    // survives and the test module is fully removed.
    assert!(out.contains("pub fn after"));
    assert!(!out.contains("mod tests"));
}
