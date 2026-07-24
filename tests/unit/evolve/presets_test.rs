//! Unit tests for the self-improvement preset library.

use super::*;

#[test]
fn every_preset_is_complete() {
    let all = presets();
    assert!(all.len() >= 7, "expected a real library, got {}", all.len());
    for p in &all {
        assert!(!p.id.is_empty());
        assert!(!p.task.is_empty(), "{} needs a task", p.id);
        assert!(!p.invariants.is_empty(), "{} needs invariants", p.id);
        assert!(!p.verify.is_empty(), "{} needs a verify step", p.id);
        assert!(!p.context_recipe.is_empty(), "{} needs a recipe", p.id);
    }
}

#[test]
fn the_three_selected_targets_are_present() {
    for id in [
        "symbol-context-selection",
        "dedup-clutter-strip",
        "split-giant-files",
    ] {
        assert!(preset(id).is_some(), "missing selected target: {id}");
    }
}

#[test]
fn directions_span_expansion_space() {
    let dirs: std::collections::BTreeSet<_> =
        presets().into_iter().map(|p| p.direction).collect();
    for d in ["context", "refactor", "capability", "automation", "comprehension"] {
        assert!(dirs.contains(d), "expansion space missing direction: {d}");
    }
}

#[test]
fn rendered_prompt_carries_invariants_and_verification() {
    let p = preset("split-giant-files").unwrap();
    let prompt = render_prompt(&p);
    assert!(prompt.contains("Invariants you MUST preserve"));
    assert!(prompt.contains("pure refactor") || prompt.contains("behavior change"));
    assert!(prompt.contains("Verify:"));
    assert!(prompt.contains("Context recipe:"));
}
