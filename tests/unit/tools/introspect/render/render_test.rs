use super::*;

#[test]
fn test_render_tree() {
    let renderer = OutputRenderer::new("tree");

    let files = vec![FileInfo {
        path: "src/main.rs".to_string(),
        depth: "signatures".to_string(),
        tokens: 500,
        symbols: vec!["main".to_string(), "helper".to_string()],
        symbols_omitted: 0,
        rendered_lines: vec!["pub fn main()".to_string(), "pub fn helper()".to_string()],
    }];

    let result = renderer.render_tree(&files).unwrap();
    assert!(result.contains("main.rs"));
    assert!(result.contains("signatures"));
    // The entry lists each symbol's rendered signature line.
    assert!(result.contains("◆ pub fn helper()"));
}

#[test]
fn test_render_flat() {
    let renderer = OutputRenderer::new("flat");

    let files = vec![FileInfo {
        path: "src/lib.rs".to_string(),
        depth: "full".to_string(),
        tokens: 1000,
        symbols: vec!["foo".to_string()],
        symbols_omitted: 0,
        rendered_lines: vec!["fn foo()".to_string()],
    }];

    let result = renderer.render_flat(&files).unwrap();
    assert!(result.contains("lib.rs"));
    assert!(result.contains("1000 tokens"));
}

#[test]
fn test_truncate_output() {
    let long_text = "a".repeat(10000);
    let truncated = truncate_output(&long_text, 100); // ~400 chars

    assert!(truncated.len() < long_text.len());
    assert!(truncated.contains("truncated"));
}

/// Every file entry in the rendered tree is exactly its `file_block` — the
/// text a per-file token count is measured on — including the truncation
/// marker for a budget-cut file.
#[test]
fn test_render_tree_is_built_from_file_blocks() {
    let renderer = OutputRenderer::new("tree");
    let files = vec![
        FileInfo {
            path: "src/a.rs".to_string(),
            depth: "signatures".to_string(),
            tokens: 0,
            symbols: vec!["a".to_string()],
            symbols_omitted: 3,
            rendered_lines: vec!["pub fn a()".to_string()],
        },
        FileInfo {
            path: "src/b.rs".to_string(),
            depth: "signatures".to_string(),
            tokens: 0,
            symbols: vec!["b".to_string()],
            symbols_omitted: 0,
            rendered_lines: vec!["pub fn b()".to_string()],
        },
    ];
    let out = renderer.render(&files).unwrap();
    let flags = renderer.last_in_group_flags(&files);
    assert_eq!(flags, vec![false, true]);
    for (i, f) in files.iter().enumerate() {
        assert!(out.contains(&renderer.file_block(f, i, flags[i])));
    }
    assert!(out.contains("… 3 more symbols omitted (token budget)"));
}
