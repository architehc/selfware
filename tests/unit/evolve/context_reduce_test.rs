use super::{reduce_source, strip_cfg_test_blocks, strip_comments};

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
fn strip_cfg_test_handles_brace_in_string() {
    let src = "#[cfg(test)]\nmod tests {\n    let s = \"}\";\n    fn t() {}\n}\npub fn after() {}\n";
    let out = strip_cfg_test_blocks(src);
    // The `}` inside the string must not prematurely close the block, so `after`
    // survives and the test module is fully removed.
    assert!(out.contains("pub fn after"));
    assert!(!out.contains("mod tests"));
}
