use anyhow::Result;
use crate::config::Config;

pub struct AgentMemory {
    context_window: usize,
}

pub struct MemoryEntry {
    pub timestamp: String,
    pub role: String,
    pub content: String,
}

impl AgentMemory {
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            context_window: config.agent.token_budget,
        })
    }
}
