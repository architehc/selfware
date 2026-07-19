use selfware::evolve::ast::AstAnalyzer;

#[test]
fn test_ast_analyzer_parses_rust_file() {
    let analyzer = AstAnalyzer::new();
    let ast = analyzer.parse_file("src/lib.rs").unwrap();
    assert_eq!(ast.kind, "source_file");
    assert_eq!(ast.start_byte, 0);
    assert!(ast.end_byte > 0);
    assert!(!ast.children.is_empty());
}

#[test]
fn test_ast_analyzer_finds_function_items() {
    let analyzer = AstAnalyzer::new();
    let ast = analyzer.parse_file("src/evolve/mod.rs").unwrap();
    let has_fn = ast.children.iter().any(|c| c.kind == "function_item");
    assert!(has_fn);
}

#[test]
fn test_ast_analyzer_missing_file_errors() {
    let analyzer = AstAnalyzer::new();
    assert!(analyzer.parse_file("src/does_not_exist.rs").is_err());
}
