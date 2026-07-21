//! OntologyEvolver: proposes and applies ontology changes with stability gates.

use anyhow::Result;
use std::path::PathBuf;

use super::{Edge, EdgeType, Graph, Node, NodeLayer, OntologyStore};

#[derive(Debug, Clone)]
pub enum OntologyProposal {
    AddConcept { name: String, description: String },
    MergeConcepts { from: String, into: String },
    SplitConcept { concept: String, new_name: String },
}

#[derive(Debug, Clone)]
pub struct OntologyVersion {
    pub id: String,
    pub operations: Vec<OntologyOperation>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OntologyOperation {
    AddNode { layer: String, id: String },
    RemoveNode { id: String },
    AddEdge { from: String, to: String },
    RemoveEdge { from: String, to: String },
}

pub struct OntologyEvolver {
    ontology_path: PathBuf,
}

impl OntologyEvolver {
    pub fn new(ontology_path: impl AsRef<std::path::Path>) -> Self {
        Self {
            ontology_path: ontology_path.as_ref().to_path_buf(),
        }
    }

    pub fn propose_change(&self, proposal: OntologyProposal) -> Result<OntologyVersion> {
        let ops = match proposal {
            OntologyProposal::AddConcept { name, .. } => {
                vec![OntologyOperation::AddNode {
                    layer: "concept".to_string(),
                    id: name,
                }]
            }
            OntologyProposal::SplitConcept { new_name, .. } => {
                // Splitting introduces the new concept node; redistributing
                // edges is a manual follow-up.
                vec![OntologyOperation::AddNode {
                    layer: "concept".to_string(),
                    id: new_name,
                }]
            }
            OntologyProposal::MergeConcepts { from, into } => self.merge_operations(&from, &into),
        };
        Ok(OntologyVersion {
            id: uuid::Uuid::new_v4().to_string(),
            operations: ops,
        })
    }

    /// Operations for merging `from` into `into`: rewire every edge touching
    /// `from` to `into`, then remove the `from` node.
    fn merge_operations(&self, from: &str, into: &str) -> Vec<OntologyOperation> {
        let mut ops = Vec::new();
        if let Ok(graph) = OntologyStore::new(&self.ontology_path).load() {
            for edge in &graph.edges {
                let rewired_from = if edge.from == from { into } else { &edge.from };
                let rewired_to = if edge.to == from { into } else { &edge.to };
                if edge.from == from || edge.to == from {
                    ops.push(OntologyOperation::RemoveEdge {
                        from: edge.from.clone(),
                        to: edge.to.clone(),
                    });
                    if rewired_from != rewired_to {
                        ops.push(OntologyOperation::AddEdge {
                            from: rewired_from.to_string(),
                            to: rewired_to.to_string(),
                        });
                    }
                }
            }
        }
        ops.push(OntologyOperation::RemoveNode {
            id: from.to_string(),
        });
        ops
    }

    /// Apply a version's operations to the ontology file on disk. A missing
    /// file is treated as an empty graph and created on save.
    pub fn apply_change(&self, version: OntologyVersion) -> Result<()> {
        let store = OntologyStore::new(&self.ontology_path);
        let mut graph = store.load().unwrap_or_default();

        for op in &version.operations {
            apply_operation(&mut graph, op);
        }

        store.save(&graph)
    }
}

fn apply_operation(graph: &mut Graph, op: &OntologyOperation) {
    match op {
        OntologyOperation::AddNode { layer, id } => {
            if graph.nodes.iter().any(|n| n.id == *id) {
                return;
            }
            let layer = match layer.as_str() {
                "code" => NodeLayer::Code,
                "preset" => NodeLayer::Preset,
                _ => NodeLayer::Concept,
            };
            let mut node = Node::code(id, id);
            node.layer = layer;
            node.path = None;
            graph.nodes.push(node);
        }
        OntologyOperation::RemoveNode { id } => {
            graph.nodes.retain(|n| n.id != *id);
            graph.edges.retain(|e| e.from != *id && e.to != *id);
        }
        OntologyOperation::AddEdge { from, to } => {
            let endpoints_exist = graph.nodes.iter().any(|n| n.id == *from)
                && graph.nodes.iter().any(|n| n.id == *to);
            let duplicate = graph
                .edges
                .iter()
                .any(|e| e.from == *from && e.to == *to && e.edge_type == EdgeType::DependsOn);
            if endpoints_exist && !duplicate {
                graph.edges.push(Edge {
                    from: from.clone(),
                    to: to.clone(),
                    edge_type: EdgeType::DependsOn,
                });
            }
        }
        OntologyOperation::RemoveEdge { from, to } => {
            graph.edges.retain(|e| !(e.from == *from && e.to == *to));
        }
    }
}
