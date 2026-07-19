use selfware::evolve::quality::{
    collect_warnings_from, cyclomatic_complexity, dead_code_ratio, QualityAnalyzer,
};
use selfware::evolve::Node;

#[test]
fn test_quality_analyzer_populates_coverage() {
    let mut node = Node::code("agent", "src/agent");
    let analyzer = QualityAnalyzer::new();
    analyzer.analyze_node(&mut node).unwrap();
    assert!(node.coverage.is_some() || node.warning_count.is_some());
}

#[test]
fn test_quality_analyzer_computes_complexity_and_dead_code() {
    let mut node = Node::code("evolve", "src/evolve");
    let analyzer = QualityAnalyzer::new();
    analyzer.analyze_node(&mut node).unwrap();
    assert!(node.complexity.unwrap() > 0.0);
    assert!(node.dead_code_ratio.unwrap() >= 0.0);
    assert!(node.dead_code_ratio.unwrap() <= 1.0);
}

#[test]
fn test_graceful_degradation_when_cargo_unavailable() {
    // Use a command name that cannot exist instead of mutating the
    // process-wide PATH (which races other cargo-spawning tests).
    assert!(collect_warnings_from("selfware-nonexistent-cargo-binary").is_none());

    let mut node = Node::code("evolve", "src/evolve");
    let analyzer = QualityAnalyzer::with_cargo_cmd("selfware-nonexistent-cargo-binary");
    // Analysis must not fail; warning metrics degrade to None.
    analyzer.analyze_node(&mut node).unwrap();
    assert!(node.warning_count.is_none());
    assert!(node.complexity.is_some());
}

#[test]
fn test_complexity_heuristic_counts_decision_points() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.rs"),
        "fn f(x: bool) {\n if x && true { } else { }\n}\n",
    )
    .unwrap();
    let score = cyclomatic_complexity(dir.path()).unwrap();
    // 1 base + 1 `if` + 1 `&&`
    assert_eq!(score, 3.0);
}

#[test]
fn test_dead_code_ratio_heuristic() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.rs"),
        "#[allow(dead_code)]\nfn a() {}\nfn b() {}\n",
    )
    .unwrap();
    let ratio = dead_code_ratio(dir.path()).unwrap();
    assert_eq!(ratio, 0.5);
}
