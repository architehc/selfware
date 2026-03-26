//! Threat Modeling Assistant
//!
//! STRIDE analysis, attack surface mapping, security architecture review,
//! and risk assessment for software systems.

pub mod analyzer;
pub mod mitigations;
pub mod model;
pub mod types;

// Re-export main types
pub use types::*;
pub use analyzer::{AttackSurfaceMapper, EntryPointDetector, SecurityScanner, StrideAnalyzer, ThreatPattern};
pub use model::ThreatModel;

#[cfg(test)]
mod tests;
