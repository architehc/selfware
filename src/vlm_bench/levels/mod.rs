//! Benchmark level implementations, one per difficulty tier.

pub mod l1_tui_state;
pub mod l2_diagnostics;
pub mod l3_architecture;
pub mod l4_profiling;
pub mod l5_layout;
pub mod mega_evolution;

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
mod tests {
    use super::*;

    #[test]
    fn test_all_levels_count() {
        let levels = all_levels();
        assert_eq!(levels.len(), 6);
    }

    #[test]
    fn test_all_levels_names_unique() {
        let levels = all_levels();
        let names: Vec<&str> = levels.iter().map(|l| l.name()).collect();
        let mut deduped = names.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(names.len(), deduped.len(), "Level names must be unique");
    }

    #[test]
    fn test_all_levels_difficulty_ascending() {
        let levels = all_levels();
        for window in levels.windows(2) {
            assert!(
                window[0].difficulty() <= window[1].difficulty(),
                "{} ({}) should be <= {} ({})",
                window[0].name(),
                window[0].difficulty(),
                window[1].name(),
                window[1].difficulty(),
            );
        }
    }
}
