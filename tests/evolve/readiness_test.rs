use selfware::evolve::diagnostics::{AnalysisKind, AnalysisReport};
use selfware::evolve::git::{GitFileState, GitStatusReport};
use selfware::evolve::readiness::{
    DeterministicRecommendation, GateEvidence, GateState, ReadinessGate, ReadinessReport,
    RecommendationHop,
};

#[test]
fn test_readiness_report_preserves_gate_evidence_and_multi_hop_actions() {
    let evidence = GateEvidence {
        source: "cargo check --all-targets".to_string(),
        detail: "unused function".to_string(),
        path: Some("src/dead.rs".to_string()),
        line: Some(14),
    };
    let report = ReadinessReport {
        ready: false,
        status: "blocked".to_string(),
        gates: vec![
            ReadinessGate {
                id: "compile".to_string(),
                label: "Cargo check".to_string(),
                state: GateState::Fail,
                summary: "1 error".to_string(),
                evidence: vec![evidence.clone()],
            },
            ReadinessGate {
                id: "coverage".to_string(),
                label: "Coverage delta".to_string(),
                state: GateState::Unknown,
                summary: "No fresh coverage run".to_string(),
                evidence: Vec::new(),
            },
        ],
        recommendations: vec![DeterministicRecommendation {
            id: "R1".to_string(),
            severity: "warning".to_string(),
            title: "Investigate possible dead code".to_string(),
            rationale: "Compiler evidence marks it unused".to_string(),
            evidence: vec![evidence],
            evidence_complete: true,
            hops: vec![
                RecommendationHop {
                    order: 1,
                    action: "Inspect declaration and references".to_string(),
                    target: "src/dead.rs:14".to_string(),
                    verification: "References are enumerated".to_string(),
                },
                RecommendationHop {
                    order: 2,
                    action: "Stage deletion preview".to_string(),
                    target: "src/dead.rs".to_string(),
                    verification: "Preview remains non-executable".to_string(),
                },
            ],
        }],
        git: GitStatusReport {
            branch: Some("self-evolve-feature".to_string()),
            head: "abc123".to_string(),
            dirty: true,
            files: vec![GitFileState {
                path: "src/dead.rs".to_string(),
                index: "clean".to_string(),
                worktree: "modified".to_string(),
            }],
        },
        analyses: vec![AnalysisReport {
            kind: AnalysisKind::Check,
            label: "Cargo check".to_string(),
            command: vec!["cargo".to_string(), "check".to_string()],
            success: false,
            exit_code: Some(101),
            duration_ms: 20,
            diagnostics: Vec::new(),
            errors: 1,
            warnings: 0,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            evidence_complete: true,
        }],
    };

    let value = serde_json::to_value(&report).unwrap();

    assert_eq!(value["ready"], false);
    assert_eq!(value["status"], "blocked");
    assert_eq!(value["gates"][0]["state"], "fail");
    assert_eq!(value["gates"][1]["state"], "unknown");
    assert_eq!(value["gates"][0]["evidence"][0]["path"], "src/dead.rs");
    assert_eq!(value["recommendations"][0]["hops"][1]["order"], 2);
    assert_eq!(value["recommendations"][0]["evidence_complete"], true);
    assert_eq!(value["git"]["files"][0]["worktree"], "modified");
    assert_eq!(value["analyses"][0]["kind"], "cargo_check");

    let decoded: ReadinessReport = serde_json::from_value(value).unwrap();
    assert!(!decoded.ready);
    assert_eq!(decoded.gates[0].state, GateState::Fail);
    assert_eq!(decoded.gates[1].state, GateState::Unknown);
    assert_eq!(decoded.recommendations[0].hops.len(), 2);
}
