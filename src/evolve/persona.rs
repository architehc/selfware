//! Component persona generator.
//!
//! Produces grounded explanations for selected components in the evolve graph.
//! MVP returns a grounded template; later iterations may integrate an LLM.

use anyhow::Result;

use super::Node;

/// Generates grounded explanations for graph components.
pub struct ComponentPersona {}

impl ComponentPersona {
    pub fn new() -> Self {
        Self {}
    }

    pub fn explain(&self, node: &Node) -> Result<String> {
        Ok(format!(
            "Component {} at {} contains {} files with ~{} tokens.",
            node.id,
            node.path.as_deref().unwrap_or("unknown"),
            node.files,
            node.tokens
        ))
    }
}

impl Default for ComponentPersona {
    fn default() -> Self {
        Self::new()
    }
}
