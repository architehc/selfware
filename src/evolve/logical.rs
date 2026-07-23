//! The logical layer — the top of the comprehension ladder.
//!
//! Below this sit clusters (10), modules (42), and files (hundreds). This layer
//! names what the system *does* as ~9 capabilities, each with a one-line purpose
//! and the invariants it must preserve. Structure (which modules, which
//! dependencies) is derived from the graph; purpose and invariants are
//! hand-authored here (the defaults) and overridable via a versioned
//! `.selfware/logical-model.yaml`, so the model stays accurate as code moves but
//! the human-meaning persists.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::evolve::clusters::{cluster_of, component_of};
use crate::evolve::{EdgeType, Graph, NodeLayer};

/// One capability: a unit of what the system does, at human scale.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capability {
    pub id: String,
    pub name: String,
    /// One-line description of the responsibility.
    pub purpose: String,
    /// What must hold true — the contract a change here must not break.
    pub invariants: Vec<String>,
    /// Architectural clusters this capability spans.
    pub clusters: Vec<String>,
    /// Top-level modules that implement it (derived from the graph).
    #[serde(default)]
    pub modules: Vec<String>,
    /// Other capabilities it depends on (derived from inter-cluster edges).
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Aggregate production token size (derived).
    #[serde(default)]
    pub tokens: usize,
}

/// The whole logical model: capabilities plus their dependency edges.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LogicalModel {
    pub capabilities: Vec<Capability>,
    pub edges: Vec<LogicalEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogicalEdge {
    pub from: String,
    pub to: String,
    pub weight: usize,
}

/// Hand-authored seed: purpose + invariants + owning clusters per capability.
/// Structure (modules, deps, tokens) is layered on from the live graph.
struct Seed {
    id: &'static str,
    name: &'static str,
    purpose: &'static str,
    invariants: &'static [&'static str],
    clusters: &'static [&'static str],
}

const SEEDS: &[Seed] = &[
    Seed {
        id: "task_loop",
        name: "Task Loop",
        purpose: "Drive a task from prompt to completion via a budget-guarded plan → act → verify → learn state machine.",
        invariants: &[
            "Every iteration runs budget and cancellation checks before doing work.",
            "State advances only through the AgentState machine (Planning → Executing → Completed/Failed).",
            "Tool results are appended to history before the next planning step.",
        ],
        clusters: &["Loop Core"],
    },
    Seed {
        id: "reasoning",
        name: "Reasoning",
        purpose: "Turn agent state into model calls and parse tool calls back out of the responses.",
        invariants: &[
            "All model I/O flows through the api client.",
            "Token budgets are enforced before each model call.",
            "Tool-call parsing tolerates every supported model's format.",
        ],
        clusters: &["Reasoning"],
    },
    Seed {
        id: "tool_dispatch",
        name: "Tool Dispatch",
        purpose: "Execute tool calls behind the safety gate, in parallel where provably safe.",
        invariants: &[
            "Every tool call passes the safety check before execution.",
            "Only parallel-safe tools run concurrently.",
            "A denied or blocked call never mutates state.",
        ],
        clusters: &["Action"],
    },
    Seed {
        id: "memory",
        name: "Memory & Learning",
        purpose: "Persist episodic memory, context, and the self-model across sessions.",
        invariants: &[
            "Context compression preserves task-critical state.",
            "Memory writes are additive or consolidated, never silently lossy.",
        ],
        clusters: &["Cognition"],
    },
    Seed {
        id: "safety",
        name: "Safety & Verification",
        purpose: "Gate mutations, validate paths, and verify outcomes before a task is done.",
        invariants: &[
            "Destructive actions require an explicit gate.",
            "Verification runs before a task is marked Completed.",
            "Path validation rejects traversal outside the workspace.",
        ],
        clusters: &["Safety & Verify"],
    },
    Seed {
        id: "evolution",
        name: "Self-Evolution",
        purpose: "Read, analyse, and rewrite the code the primary loop runs on — the second-order loop.",
        invariants: &[
            "A clean git working tree is required before an evolve action.",
            "Every proposed change is reviewed and verified before merge.",
            "Private source never crosses the external model trust boundary.",
        ],
        clusters: &["Evolution"],
    },
    Seed {
        id: "interface",
        name: "Interface",
        purpose: "CLI, TUI, and IDE surfaces that let humans drive and observe the system.",
        invariants: &[
            "Rendering never blocks the task loop.",
            "User interrupts are honored within one loop iteration.",
        ],
        clusters: &["Interface"],
    },
    Seed {
        id: "observability",
        name: "Observability",
        purpose: "Log, audit, and measure every action for feedback and cost accounting.",
        invariants: &[
            "Every tool call is audited to the JSONL trail.",
            "Token and cost accounting is recorded per run.",
        ],
        clusters: &["Observability"],
    },
    Seed {
        id: "foundation",
        name: "Foundation",
        purpose: "Configuration, errors, hooks, and evaluation harnesses the rest builds on.",
        invariants: &[
            "Configuration precedence is deterministic.",
            "Hooks fire at defined lifecycle points.",
        ],
        clusters: &["Foundation", "Eval / Bench"],
    },
];

/// Map a cluster name to the capability that owns it.
fn capability_for_cluster(cluster: &str) -> &'static str {
    for seed in SEEDS {
        if seed.clusters.contains(&cluster) {
            return seed.id;
        }
    }
    "foundation"
}

/// Build the logical model: hand-authored capabilities enriched with the modules,
/// dependencies, and sizes derived from the live graph.
pub fn build_logical_model(graph: &Graph, _root: &Path) -> LogicalModel {
    // Per-capability module set and token totals, from production code nodes.
    let mut modules: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut tokens: BTreeMap<&str, usize> = BTreeMap::new();
    for node in graph.nodes.iter().filter(|n| n.layer == NodeLayer::Code) {
        let component = component_of(&node.id);
        let cap = capability_for_cluster(cluster_of(&component));
        modules.entry(cap).or_default().insert(component);
        *tokens.entry(cap).or_default() += node.tokens;
    }

    // Capability-level dependency edges, collapsed from module DependsOn edges.
    let mut weights: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for edge in &graph.edges {
        if !matches!(edge.edge_type, EdgeType::DependsOn) {
            continue;
        }
        let from = capability_for_cluster(cluster_of(&component_of(&edge.from)));
        let to = capability_for_cluster(cluster_of(&component_of(&edge.to)));
        if from != to {
            *weights.entry((from, to)).or_default() += 1;
        }
    }

    let capabilities = SEEDS
        .iter()
        .map(|seed| {
            let deps: Vec<String> = weights
                .keys()
                .filter(|(from, _)| *from == seed.id)
                .map(|(_, to)| to.to_string())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            Capability {
                id: seed.id.to_string(),
                name: seed.name.to_string(),
                purpose: seed.purpose.to_string(),
                invariants: seed.invariants.iter().map(|s| s.to_string()).collect(),
                clusters: seed.clusters.iter().map(|s| s.to_string()).collect(),
                modules: modules
                    .get(seed.id)
                    .map(|m| m.iter().cloned().collect())
                    .unwrap_or_default(),
                depends_on: deps,
                tokens: tokens.get(seed.id).copied().unwrap_or(0),
            }
        })
        .collect();

    let edges = weights
        .into_iter()
        .map(|((from, to), weight)| LogicalEdge {
            from: from.to_string(),
            to: to.to_string(),
            weight,
        })
        .collect();

    LogicalModel {
        capabilities,
        edges,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/logical_test.rs"]
mod logical_test;
