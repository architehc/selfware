//! Merge-readiness and deterministic multi-hop recommendations.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::diagnostics::{AnalysisKind, AnalysisReport, DiagnosticsEngine};
use super::git::{GitEngine, GitStatusReport};
use super::ontology::ValidationReport;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateState {
    Pass,
    Fail,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateEvidence {
    pub source: String,
    pub detail: String,
    pub path: Option<String>,
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessGate {
    pub id: String,
    pub label: String,
    pub state: GateState,
    pub summary: String,
    pub evidence: Vec<GateEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendationHop {
    pub order: usize,
    pub action: String,
    pub target: String,
    pub verification: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterministicRecommendation {
    pub id: String,
    pub severity: String,
    pub title: String,
    pub rationale: String,
    pub evidence: Vec<GateEvidence>,
    pub evidence_complete: bool,
    pub hops: Vec<RecommendationHop>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessReport {
    pub ready: bool,
    pub status: String,
    pub gates: Vec<ReadinessGate>,
    pub recommendations: Vec<DeterministicRecommendation>,
    pub git: GitStatusReport,
    pub analyses: Vec<AnalysisReport>,
}

#[derive(Debug, Clone)]
pub struct ReadinessEngine {
    diagnostics: DiagnosticsEngine,
    git: GitEngine,
}

impl ReadinessEngine {
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        let root = project_root.as_ref();
        Self {
            diagnostics: DiagnosticsEngine::new(root),
            git: GitEngine::new(root),
        }
    }

    /// Run a single analysis kind with a timeout. On timeout, returns a report
    /// with `success = false` and a descriptive message instead of hanging.
    async fn run_with_timeout(
        &self,
        kind: AnalysisKind,
        timeout: std::time::Duration,
    ) -> AnalysisReport {
        match tokio::time::timeout(timeout, self.diagnostics.run(kind)).await {
            Ok(Ok(report)) => report,
            Ok(Err(error)) => AnalysisReport {
                kind,
                label: kind.label().to_string(),
                command: std::iter::once("cargo".to_string())
                    .chain(kind.args().iter().map(|arg| (*arg).to_string()))
                    .collect(),
                success: false,
                exit_code: None,
                duration_ms: timeout.as_millis() as u64,
                diagnostics: Vec::new(),
                errors: 0,
                warnings: 0,
                stdout_tail: String::new(),
                stderr_tail: format!("analysis failed: {error}"),
                evidence_complete: false,
            },
            Err(_) => AnalysisReport {
                kind,
                label: kind.label().to_string(),
                command: std::iter::once("cargo".to_string())
                    .chain(kind.args().iter().map(|arg| (*arg).to_string()))
                    .collect(),
                success: false,
                exit_code: None,
                duration_ms: timeout.as_millis() as u64,
                diagnostics: Vec::new(),
                errors: 0,
                warnings: 0,
                stdout_tail: String::new(),
                stderr_tail: format!("analysis timed out after {}s", timeout.as_secs()),
                evidence_complete: false,
            },
        }
    }

    pub async fn evaluate(&self, graph: &ValidationReport) -> Result<ReadinessReport> {
        let git = self.git.status()?;

        // Run the three cargo analyses concurrently instead of sequentially so
        // the total wall-clock time is ~max(check, clippy, tests) instead of
        // check + clippy + tests. Each command is individually bounded by a
        // timeout so a stuck build cannot block the HTTP request indefinitely.
        const ANALYSIS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

        let (check, clippy, tests) = tokio::join!(
            self.run_with_timeout(AnalysisKind::Check, ANALYSIS_TIMEOUT),
            self.run_with_timeout(AnalysisKind::Clippy, ANALYSIS_TIMEOUT),
            self.run_with_timeout(AnalysisKind::EvolveTests, ANALYSIS_TIMEOUT),
        );

        let analyses = vec![check, clippy, tests];

        let mut gates = Vec::new();
        for analysis in &analyses {
            gates.push(ReadinessGate {
                id: format!("analysis:{:?}", analysis.kind).to_lowercase(),
                label: analysis.label.clone(),
                state: if analysis.success {
                    GateState::Pass
                } else {
                    GateState::Fail
                },
                summary: format!(
                    "{} error(s), {} warning(s), {} ms",
                    analysis.errors, analysis.warnings, analysis.duration_ms
                ),
                evidence: vec![GateEvidence {
                    source: "command".to_string(),
                    detail: analysis.command.join(" "),
                    path: None,
                    line: None,
                }],
            });
        }
        gates.push(ReadinessGate {
            id: "graph_integrity".to_string(),
            label: "Graph integrity".to_string(),
            state: if graph.valid {
                GateState::Pass
            } else {
                GateState::Fail
            },
            summary: if graph.valid {
                "No blocking structural defects".to_string()
            } else {
                format!(
                    "{} cycle(s), {} dangling edge(s), {} isolated node(s)",
                    graph.cycles.len(),
                    graph.dangling_edges.len(),
                    graph.isolated_nodes.len()
                )
            },
            evidence: vec![GateEvidence {
                source: "graph_validator".to_string(),
                detail: "GET /api/ontology/validate".to_string(),
                path: Some(".selfware/evolve-graph.yaml".to_string()),
                line: None,
            }],
        });
        gates.push(ReadinessGate {
            id: "git_clean".to_string(),
            label: "Reviewable working tree".to_string(),
            state: if git.dirty {
                GateState::Fail
            } else {
                GateState::Pass
            },
            summary: if git.dirty {
                format!("{} uncommitted path(s)", git.files.len())
            } else {
                "Working tree is clean".to_string()
            },
            evidence: vec![GateEvidence {
                source: "git".to_string(),
                detail: format!("HEAD {}", git.head),
                path: None,
                line: None,
            }],
        });
        gates.push(ReadinessGate {
            id: "coverage".to_string(),
            label: "Coverage delta".to_string(),
            state: GateState::Unknown,
            summary: "No fresh coverage run is attached to this snapshot".to_string(),
            evidence: Vec::new(),
        });
        gates.push(ReadinessGate {
            id: "hotpath_profile".to_string(),
            label: "Hot-path profile".to_string(),
            state: GateState::Unknown,
            summary: "No benchmark or profiler artifact is attached to this snapshot".to_string(),
            evidence: Vec::new(),
        });

        let recommendations = deterministic_recommendations(&analyses, &git, graph);
        let ready = gates.iter().all(|gate| gate.state == GateState::Pass);
        let status = if ready {
            "ready"
        } else if gates.iter().any(|gate| gate.state == GateState::Fail) {
            "blocked"
        } else {
            "incomplete"
        }
        .to_string();

        Ok(ReadinessReport {
            ready,
            status,
            gates,
            recommendations,
            git,
            analyses,
        })
    }
}

fn deterministic_recommendations(
    analyses: &[AnalysisReport],
    git: &GitStatusReport,
    graph: &ValidationReport,
) -> Vec<DeterministicRecommendation> {
    let mut recommendations = Vec::new();
    let mut index = 1usize;

    for report in analyses {
        for diagnostic in report.diagnostics.iter().take(40) {
            let primary = diagnostic.spans.iter().find(|span| span.is_primary);
            let target = primary
                .map(|span| format!("{}:{}", span.file, span.line_start))
                .unwrap_or_else(|| "workspace".to_string());
            let dead_code = diagnostic.code.as_deref() == Some("dead_code")
                || diagnostic.message.contains("never used")
                || diagnostic.message.contains("unused");
            let evidence = vec![GateEvidence {
                source: report.command.join(" "),
                detail: diagnostic.message.clone(),
                path: primary.map(|span| span.file.clone()),
                line: primary.map(|span| span.line_start),
            }];
            let hops = if dead_code {
                vec![
                    RecommendationHop {
                        order: 1,
                        action: "Inspect declaration and inbound references".to_string(),
                        target: target.clone(),
                        verification: "Graph and AST references are enumerated".to_string(),
                    },
                    RecommendationHop {
                        order: 2,
                        action: "Run focused tests for the owning component".to_string(),
                        target: target.clone(),
                        verification: "Focused tests pass before any source deletion is staged"
                            .to_string(),
                    },
                    RecommendationHop {
                        order: 3,
                        action: "Stage deletion preview".to_string(),
                        target: target.clone(),
                        verification:
                            "Exact content hash, dependents, diff, and rollback checkpoint exist"
                                .to_string(),
                    },
                ]
            } else {
                vec![
                    RecommendationHop {
                        order: 1,
                        action: "Open grounded diagnostic span".to_string(),
                        target: target.clone(),
                        verification: "Source hash still matches the diagnostic snapshot"
                            .to_string(),
                    },
                    RecommendationHop {
                        order: 2,
                        action: "Apply a scoped edit".to_string(),
                        target: target.clone(),
                        verification: report.command.join(" "),
                    },
                    RecommendationHop {
                        order: 3,
                        action: "Run evolve tests".to_string(),
                        target: "tests/evolve".to_string(),
                        verification: "cargo test --test evolve".to_string(),
                    },
                ]
            };
            recommendations.push(DeterministicRecommendation {
                id: format!("R{index}"),
                severity: diagnostic.level.clone(),
                title: if dead_code {
                    format!("Investigate possible dead code at {target}")
                } else {
                    format!("Resolve {} diagnostic at {target}", diagnostic.level)
                },
                rationale: diagnostic.message.clone(),
                evidence,
                evidence_complete: report.evidence_complete && primary.is_some(),
                hops,
            });
            index += 1;
        }
    }

    if git.dirty {
        recommendations.push(DeterministicRecommendation {
            id: format!("R{index}"),
            severity: "warning".to_string(),
            title: "Separate the working-tree overlay before branch creation".to_string(),
            rationale: format!("Git reports {} changed path(s)", git.files.len()),
            evidence: git
                .files
                .iter()
                .take(40)
                .map(|file| GateEvidence {
                    source: "git status".to_string(),
                    detail: format!("index={}, worktree={}", file.index, file.worktree),
                    path: Some(file.path.clone()),
                    line: None,
                })
                .collect(),
            evidence_complete: git.files.len() <= 40,
            hops: vec![
                RecommendationHop {
                    order: 1,
                    action: "Review exact working-tree diff".to_string(),
                    target: "repository".to_string(),
                    verification: "Every changed path has an owner and intent".to_string(),
                },
                RecommendationHop {
                    order: 2,
                    action: "Commit or preserve unrelated work".to_string(),
                    target: "repository".to_string(),
                    verification: "git status is clean".to_string(),
                },
                RecommendationHop {
                    order: 3,
                    action: "Create exact-head evolve branch".to_string(),
                    target: git.head.clone(),
                    verification: "Branch starts at the approved HEAD".to_string(),
                },
            ],
        });
        index += 1;
    }

    if !graph.valid {
        recommendations.push(DeterministicRecommendation {
            id: format!("R{index}"),
            severity: "error".to_string(),
            title: "Repair graph integrity before mutation actions".to_string(),
            rationale: "The graph validator reports incomplete or contradictory structure"
                .to_string(),
            evidence: vec![GateEvidence {
                source: "graph_validator".to_string(),
                detail: format!(
                    "cycles={}, dangling={}, isolated={}",
                    graph.cycles.len(),
                    graph.dangling_edges.len(),
                    graph.isolated_nodes.len()
                ),
                path: Some(".selfware/evolve-graph.yaml".to_string()),
                line: None,
            }],
            evidence_complete: true,
            hops: vec![
                RecommendationHop {
                    order: 1,
                    action: "Inspect invalid graph evidence".to_string(),
                    target: "ontology".to_string(),
                    verification: "Every reported endpoint maps to a current node version"
                        .to_string(),
                },
                RecommendationHop {
                    order: 2,
                    action: "Rebuild the derived graph".to_string(),
                    target: "workspace".to_string(),
                    verification: "Graph revision changes deterministically".to_string(),
                },
                RecommendationHop {
                    order: 3,
                    action: "Re-run structural validation".to_string(),
                    target: "ontology".to_string(),
                    verification: "No blocking graph defects remain".to_string(),
                },
            ],
        });
    }

    recommendations
}
