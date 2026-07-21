use super::*;

#[test]
fn test_render_tree() {
    let renderer = OutputRenderer::new("tree");

    let files = vec![FileInfo {
        path: "src/main.rs".to_string(),
        depth: "signatures".to_string(),
        tokens: 500,
        symbols: vec!["main".to_string(), "helper".to_string()],
    }];

    let result = renderer.render_tree(&files, &[]).unwrap();
    assert!(result.contains("main.rs"));
    assert!(result.contains("signatures"));
}

#[test]
fn test_render_flat() {
    let renderer = OutputRenderer::new("flat");

    let files = vec![FileInfo {
        path: "src/lib.rs".to_string(),
        depth: "full".to_string(),
        tokens: 1000,
        symbols: vec!["foo".to_string()],
    }];

    let result = renderer.render_flat(&files, &[]).unwrap();
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
