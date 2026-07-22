use super::strip_comments;

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
