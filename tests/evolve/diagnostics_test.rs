use selfware::evolve::diagnostics::{
    AnalysisKind, AnalysisReport, CompilerDiagnostic, DiagnosticSpan,
};

#[test]
fn test_analysis_kind_accepts_only_the_fixed_command_vocabulary() {
    assert_eq!(
        serde_json::from_str::<AnalysisKind>(r#""cargo_check""#).unwrap(),
        AnalysisKind::Check
    );
    assert_eq!(
        serde_json::from_str::<AnalysisKind>(r#""check""#).unwrap(),
        AnalysisKind::Check
    );
    assert_eq!(
        serde_json::from_str::<AnalysisKind>(r#""clippy""#).unwrap(),
        AnalysisKind::Clippy
    );
    assert_eq!(
        serde_json::from_str::<AnalysisKind>(r#""evolve_tests""#).unwrap(),
        AnalysisKind::EvolveTests
    );
    assert!(serde_json::from_str::<AnalysisKind>(r#""sh -c rm""#).is_err());
    assert_eq!(
        serde_json::to_string(&AnalysisKind::Check).unwrap(),
        r#""cargo_check""#
    );
    assert_eq!(AnalysisKind::Check.label(), "Cargo check");
    assert_eq!(AnalysisKind::Clippy.label(), "Clippy");
    assert_eq!(AnalysisKind::EvolveTests.label(), "Evolve tests");
}

#[test]
fn test_analysis_report_serializes_grounded_compiler_spans() {
    let report = AnalysisReport {
        kind: AnalysisKind::Check,
        label: "Cargo check".to_string(),
        command: vec![
            "cargo".to_string(),
            "check".to_string(),
            "--all-targets".to_string(),
            "--message-format=json".to_string(),
        ],
        success: false,
        exit_code: Some(101),
        duration_ms: 42,
        diagnostics: vec![CompilerDiagnostic {
            level: "error".to_string(),
            code: Some("E0308".to_string()),
            message: "mismatched types".to_string(),
            rendered: Some("error[E0308]: mismatched types".to_string()),
            spans: vec![DiagnosticSpan {
                file: "src/lib.rs".to_string(),
                line_start: 12,
                line_end: 12,
                column_start: 5,
                column_end: 9,
                is_primary: true,
                label: Some("expected usize".to_string()),
            }],
        }],
        errors: 1,
        warnings: 0,
        stdout_tail: "compiler JSON".to_string(),
        stderr_tail: String::new(),
        evidence_complete: true,
    };

    let value = serde_json::to_value(&report).unwrap();

    assert_eq!(value["kind"], "cargo_check");
    assert_eq!(value["command"][0], "cargo");
    assert_eq!(value["diagnostics"][0]["code"], "E0308");
    assert_eq!(value["diagnostics"][0]["spans"][0]["file"], "src/lib.rs");
    assert_eq!(value["diagnostics"][0]["spans"][0]["line_start"], 12);
    assert_eq!(value["diagnostics"][0]["spans"][0]["is_primary"], true);

    let decoded: AnalysisReport = serde_json::from_value(value).unwrap();
    assert_eq!(decoded.kind, AnalysisKind::Check);
    assert_eq!(decoded.errors, 1);
    assert_eq!(decoded.diagnostics[0].spans[0].column_end, 9);
    assert!(decoded.evidence_complete);
}
