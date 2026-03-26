//! Swarm Factory
//!
//! Helper functions to create pre-configured swarms.

use super::coordinator::Swarm;
use super::types::{Agent, AgentRole};

/// Create a standard development swarm
pub fn create_dev_swarm() -> Swarm {
    let mut swarm = Swarm::new();

    swarm.add_agent(Agent::new("Archie", AgentRole::Architect));
    swarm.add_agent(Agent::new("Cody", AgentRole::Coder));
    swarm.add_agent(Agent::new("Tessa", AgentRole::Tester));
    swarm.add_agent(Agent::new("Rex", AgentRole::Reviewer));

    swarm
}

/// Create a security-focused swarm
pub fn create_security_swarm() -> Swarm {
    let mut swarm = Swarm::new();

    swarm.add_agent(Agent::new("Guardian", AgentRole::Security));
    swarm.add_agent(Agent::new("Rex", AgentRole::Reviewer));
    swarm.add_agent(Agent::new("Tessa", AgentRole::Tester));

    swarm
}
