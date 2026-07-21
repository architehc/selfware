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

#[test]
fn test_cfg_test_ranges_finds_top_level_test_module() {
    let source = r#"fn production() {}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {}
}

fn after() {}
"#;

    let ranges = AstAnalyzer::new().cfg_test_ranges(source).unwrap();

    assert_eq!(ranges, vec![(3, 7)]);
}

#[test]
fn test_cfg_test_ranges_recurses_into_modules_and_impls() {
    let source = r#"mod nested {
    struct Subject;

    impl Subject {
        fn production(&self) {}

        #[cfg(test)]
        fn test_helper(&self) {
            assert!(true);
        }
    }

    #[cfg(test)]
    fn module_test_helper() {}
}
"#;

    let ranges = AstAnalyzer::new().cfg_test_ranges(source).unwrap();

    assert_eq!(ranges, vec![(7, 10), (13, 14)]);
}

#[test]
fn test_cfg_test_ranges_keeps_attached_attributes_and_comments() {
    let source = r#"fn before() {}

#[allow(dead_code)]
// The test configuration applies to the next item.
#[cfg(
    test
)]
/* Keep this explanation with the helper. */
#[inline]
fn helper() {
    assert!(true);
}

fn after() {}
"#;

    let ranges = AstAnalyzer::new().cfg_test_ranges(source).unwrap();

    assert_eq!(ranges, vec![(3, 12)]);
}

#[test]
fn test_cfg_test_ranges_accepts_only_predicates_that_require_test() {
    let source = r#"#[cfg(any(test, feature = "slow"))]
fn any_predicate() {}

#[cfg(all(test, feature = "slow"))]
fn all_predicate() {}

#[cfg(all(any(test, feature = "slow"), unix))]
fn nested_non_exclusive_predicate() {}

#[cfg_attr(test, allow(dead_code))]
fn conditional_attribute() {}

#[cfg(feature = "slow")]
fn feature_only() {}
"#;

    let ranges = AstAnalyzer::new().cfg_test_ranges(source).unwrap();

    assert_eq!(ranges, vec![(4, 5)]);
}

#[test]
fn test_cfg_test_ranges_are_disjoint_when_outer_item_is_test_only() {
    let source = r#"#[cfg(test)]
mod tests {
    #[cfg(test)]
    fn nested() {}
}
"#;

    let ranges = AstAnalyzer::new().cfg_test_ranges(source).unwrap();

    assert_eq!(ranges, vec![(1, 5)]);
}

#[test]
fn test_cfg_test_body_ranges_do_not_count_external_test_module_links() {
    let source = r#"#[cfg(test)]
#[path = "subject_test.rs"]
mod tests;

#[cfg(test)]
fn inline_helper() {}
"#;

    let analyzer = AstAnalyzer::new();
    assert_eq!(
        analyzer.cfg_test_ranges(source).unwrap(),
        vec![(1, 3), (5, 6)]
    );
    assert_eq!(analyzer.cfg_test_body_ranges(source).unwrap(), vec![(5, 6)]);
}
