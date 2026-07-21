//! Gatekeeper: compile/test and architecture gates for evolution actions.
//!
//! Gates must pass before the engine recommends or applies an action.

use anyhow::Result;

#[derive(Debug)]
pub struct GateResult {
    pub passed: bool,
    pub errors: Vec<String>,
}

pub struct Gatekeeper {}

impl Default for Gatekeeper {
    fn default() -> Self {
        Self::new()
    }
}

impl Gatekeeper {
    pub fn new() -> Self {
        Self {}
    }

    pub fn check_code_gates(&self) -> Result<GateResult> {
        // MVP: run cargo check and parse result
        let output = std::process::Command::new("cargo")
            .args(["check", "--lib"])
            .output()?;
        Ok(GateResult {
            passed: output.status.success(),
            errors: if output.status.success() {
                vec![]
            } else {
                vec![String::from_utf8_lossy(&output.stderr).to_string()]
            },
        })
    }

    pub fn check_architecture_gates(&self, graph: &super::Graph) -> Result<GateResult> {
        let report = super::validate_graph(graph);
        let mut errors = Vec::new();
        if !report.duplicate_ids.is_empty() {
            errors.push(format!(
                "duplicate node ids: {}",
                report.duplicate_ids.join(", ")
            ));
        }
        if !report.cycles.is_empty() {
            errors.push(format!("{} ontology cycle(s)", report.cycles.len()));
        }
        if !report.dangling_edges.is_empty() {
            errors.push(format!("{} dangling edge(s)", report.dangling_edges.len()));
        }
        if !report.isolated_nodes.is_empty() {
            errors.push(format!("{} isolated node(s)", report.isolated_nodes.len()));
        }
        Ok(GateResult {
            passed: report.valid,
            errors,
        })
    }
}
