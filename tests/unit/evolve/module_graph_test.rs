//! Unit tests for parsing the crate module manifest from lib.rs source.

use super::*;

const SAMPLE: &str = r#"
pub mod agent;
pub mod api;
#[cfg(test)]
pub(crate) mod test_support;
pub mod tools;

// ============================================================================
// Domain modules
// ============================================================================
pub mod analysis;
pub(crate) mod session;

// Evolution engine
#[cfg(feature = "self-improvement")]
pub mod evolution;

// Backward-compatible re-exports for safety module
pub use safety::redact;
pub use analysis::bm25;

// ============================================================================
// New feature modules
pub mod evolve;

// ============================================================================
// Utility modules
// ============================================================================
pub mod token_count;
"#;

#[test]
fn parses_visibility_and_category() {
    let m = parse(SAMPLE);
    let by = |name: &str| m.modules.iter().find(|d| d.name == name).cloned().unwrap();

    assert_eq!(by("agent").visibility, "pub");
    assert_eq!(by("agent").category, "core");
    assert_eq!(by("session").visibility, "pub(crate)");
    assert_eq!(by("session").category, "domain");
    assert_eq!(by("evolve").category, "feature");
    assert_eq!(by("token_count").category, "utility");
}

#[test]
fn captures_cfg_test_and_feature_gates() {
    let m = parse(SAMPLE);
    let by = |name: &str| m.modules.iter().find(|d| d.name == name).cloned().unwrap();

    assert!(by("test_support").test_only);
    assert_eq!(by("test_support").visibility, "pub(crate)");
    assert!(!by("agent").test_only);

    assert_eq!(by("evolution").cfg_feature.as_deref(), Some("self-improvement"));
    assert_eq!(by("agent").cfg_feature, None);
    // The feature gate must not leak onto the next declaration.
    assert_eq!(by("evolve").cfg_feature, None);
}

#[test]
fn collects_reexports_with_owning_module() {
    let m = parse(SAMPLE);
    assert!(m.reexports.iter().any(|r| r.path == "safety::redact" && r.module == "safety"));
    assert!(m.reexports.iter().any(|r| r.path == "analysis::bm25" && r.module == "analysis"));
    // `pub mod` lines are not re-exports.
    assert!(!m.reexports.iter().any(|r| r.path == "agent"));
}

#[test]
fn module_count_is_stable() {
    let m = parse(SAMPLE);
    let names: Vec<_> = m.modules.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(
        names,
        ["agent", "api", "test_support", "tools", "analysis", "session", "evolution", "evolve", "token_count"]
    );
}
