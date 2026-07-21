use selfware::evolve::ast::{AstAnalyzer, AstNode};
use selfware::evolve::summary::{compile_structural_summary, compile_summary};

#[test]
fn test_structural_summary_projects_source_without_function_bodies() {
    let source = r#"#[derive(Debug)]
struct Item {
    value: usize,
}

impl<const N: usize> Item<{ N }> {
    /// Returns the stored value.
    fn value(&self) -> usize {
        self.value + N
    }
}
"#;
    let ast = AstAnalyzer::new().parse_source(source).unwrap();

    let summary = compile_summary(&ast, source);

    assert!(summary.contains("#[derive(Debug)]"));
    assert!(summary.contains("struct Item {"));
    assert!(summary.contains("value: usize"));
    assert!(summary.contains("impl<const N: usize> Item<{ N }> {"));
    assert!(summary.contains("/// Returns the stored value."));
    assert!(summary.contains("fn value(&self) -> usize { ... }"));
    assert!(!summary.contains("self.value + N"));
}

#[test]
fn test_structural_summary_skips_invalid_ranges_without_panicking() {
    let ast = AstNode {
        kind: "source_file".to_string(),
        start_byte: 0,
        end_byte: 2,
        start_line: 1,
        start_column: 1,
        end_line: 1,
        end_column: 3,
        label: None,
        named: true,
        has_error: false,
        children: vec![AstNode {
            kind: "function_item".to_string(),
            start_byte: 10,
            end_byte: 20,
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
            label: Some("bad".to_string()),
            named: true,
            has_error: false,
            children: vec![],
        }],
    };

    assert_eq!(compile_summary(&ast, "fn"), "");
}

#[test]
fn test_structural_summary_preserves_every_declaration_shape() {
    let source = r#"struct Pair<T>(T) where T: Copy;

trait Service {
    fn required(&self);
}

extern "C" {
    fn external(value: i32);
}

register_service!(Service);
"#;
    let ast = AstAnalyzer::new().parse_source(source).unwrap();

    let report = compile_structural_summary(&ast, source);

    assert!(report.outline.contains("struct Pair<T>(T) where T: Copy;"));
    assert!(!report.outline.contains("struct Pair<T> { ... }"));
    assert!(report.outline.contains("fn required(&self);"));
    assert!(report.outline.contains("extern \"C\" {"));
    assert!(report.outline.contains("fn external(value: i32);"));
    assert!(report.outline.contains("register_service!(Service);"));
    assert!(report.complete, "{report:#?}");
    assert!(report.omitted_node_kinds.is_empty());
    assert!(report.evidence.iter().all(|item| item.start_line > 0));
}

#[test]
fn test_structural_summary_discloses_parse_errors() {
    let source = "pub fn broken( {\n";
    let ast = AstAnalyzer::new().parse_source(source).unwrap();

    let report = compile_structural_summary(&ast, source);

    assert!(report.parse_has_error);
    assert!(!report.complete);
}

#[test]
fn test_structural_summary_reports_output_and_evidence_limits() {
    let declarations = (0..4097)
        .map(|index| format!("const VALUE_{index}: usize = {index};\n"))
        .collect::<String>();
    let ast = AstAnalyzer::new().parse_source(&declarations).unwrap();
    let evidence_limited = compile_structural_summary(&ast, &declarations);
    assert_eq!(evidence_limited.evidence.len(), 4096);
    assert!(evidence_limited.truncated);
    assert!(!evidence_limited.complete);

    let oversized = format!("const VALUE: &str = \"{}\";", "x".repeat(300_000));
    let ast = AstAnalyzer::new().parse_source(&oversized).unwrap();
    let output_limited = compile_structural_summary(&ast, &oversized);
    assert!(output_limited.truncated);
    assert!(!output_limited.complete);
    assert!(output_limited.outline.len() <= 256 * 1024);
}

#[test]
fn test_structural_summary_reports_nesting_limit() {
    let mut source = String::new();
    for index in 0..130 {
        source.push_str(&format!("mod level_{index} {{\n"));
    }
    source.push_str("const VALUE: usize = 1;\n");
    source.push_str(&"}\n".repeat(130));
    let ast = AstAnalyzer::new().parse_source(&source).unwrap();

    let report = compile_structural_summary(&ast, &source);

    assert!(report.depth_limit_reached);
    assert!(report.truncated);
    assert!(!report.complete);
}
