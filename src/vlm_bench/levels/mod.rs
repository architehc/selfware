//! Benchmark level implementations, one per difficulty tier.

pub mod l1_tui_state;
pub mod l2_diagnostics;
pub mod l3_architecture;
pub mod l4_profiling;
pub mod l5_layout;
pub mod mega_evolution;

pub use l1_tui_state::L1TuiState;
pub use l2_diagnostics::L2Diagnostics;
pub use l3_architecture::L3Architecture;
pub use l4_profiling::L4Profiling;
pub use l5_layout::L5Layout;
pub use mega_evolution::MegaEvolution;

use super::VlmBenchLevel;

/// Build the default set of all benchmark levels.
pub fn all_levels() -> Vec<Box<dyn VlmBenchLevel>> {
    vec![
        Box::new(l1_tui_state::L1TuiState::new()),
        Box::new(l2_diagnostics::L2Diagnostics::new()),
        Box::new(l3_architecture::L3Architecture::new()),
        Box::new(l4_profiling::L4Profiling::new()),
        Box::new(l5_layout::L5Layout::new()),
        Box::new(mega_evolution::MegaEvolution::new()),
    ]
}

#[cfg(test)]
#[path = "../../../tests/unit/vlm_bench/levels/mod_test.rs"]
mod tests;
