use std::fs;

use selfware::evolve::context_fit::{fit_tier, FitBudget, RequestedMode, TierMeasurer};
use selfware::evolve::{ContextMode, Graph, Node};

/// One Rust file whose Full > Compact > Lite, with an inline test block so
/// FullExtended > Full. Bodies are large enough that the comment-stripped
/// Compact projection outweighs the signature-only Lite skeleton.
const ALPHA_RS: &str = r#"
//! Module doc comment that compact strips.
//! A second line of module docs to pad the comment overhead.

use std::collections::HashMap;

/// Doc comment on a public function.
pub fn alpha_one(x: usize) -> usize {
    // line comment explaining the arithmetic below
    let mut acc = x + 1;
    // accumulate over a small range so the body has real weight
    for i in 0..x {
        acc += i * 2;
        acc = acc % 1024;
    }
    acc * 2
}

/// Another documented helper with a match-heavy body.
pub fn alpha_two(s: &str) -> String {
    // classify the input length, then build the output string
    let label = match s.len() {
        0 => "empty",
        1..=4 => "short",
        5..=12 => "medium",
        _ => "long",
    };
    let mut out = String::with_capacity(s.len() + 16);
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            out.push(ch);
        }
    }
    format!("{label}:{out}!")
}

/// Count words per line into a map; body is pure logic, no comments survive.
pub fn alpha_three(lines: &[&str]) -> HashMap<usize, usize> {
    let mut counts = HashMap::new();
    for (idx, line) in lines.iter().enumerate() {
        let words = line.split_whitespace().count();
        counts.insert(idx, words);
    }
    counts
}

/// Extra signature so the Lite skeleton outweighs the two-card Map.
pub fn alpha_four(flag: bool) -> Option<&'static str> {
    if flag {
        Some("yes")
    } else {
        None
    }
}

/// Another signature-only entry for the skeleton.
pub fn alpha_five(values: &[u64]) -> u64 {
    values.iter().copied().sum()
}

/// And one more, keeping the measured Lite > Map margin comfortable.
pub fn alpha_six(input: &str, times: usize) -> String {
    input.repeat(times)
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(super::alpha_one(1), 4);
    }
}
"#;

fn fixture() -> (tempfile::TempDir, Graph) {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("alpha.rs"), ALPHA_RS).unwrap();

    let mut code = Node::code("crate::alpha", "src/alpha.rs");
    code.tokens = selfware::token_count::estimate_content_tokens(ALPHA_RS);
    let test_block = "#[cfg(test)]\nmod tests {\n    #[test]\n    fn it_works() {\n        assert_eq!(super::alpha_one(1), 4);\n    }\n}\n";
    code.inline_test_tokens = selfware::token_count::estimate_content_tokens(test_block);
    code.inline_test_ranges = 1;

    let mut test_node = Node::code("crate::alpha_tests", "src/alpha.rs");
    test_node.layer = selfware::evolve::NodeLayer::Test;
    test_node.tokens = code.inline_test_tokens;

    (
        dir,
        Graph {
            nodes: vec![code, test_node],
            edges: vec![],
        },
    )
}

fn budget_for(tokens: usize) -> FitBudget {
    // fit_ratio 1.0 and zero reserve so `usable()` is exactly `tokens`.
    FitBudget {
        context_length: tokens,
        output_reserve: 0,
        fit_ratio: 1.0,
    }
}

#[test]
fn fit_tier_picks_richest_tier_that_fits() {
    let (dir, graph) = fixture();
    let measurer = TierMeasurer::new(&graph, dir.path());

    let full_extended = measurer.measure(&ContextMode::FullExtended);
    let full = measurer.measure(&ContextMode::Full);
    let compact = measurer.measure(&ContextMode::Compact);
    let lite = measurer.measure(&ContextMode::Lite);
    let map = measurer.measure(&ContextMode::Map);

    // Measured tiers are strictly ordered on this fixture.
    assert!(full_extended > full, "{full_extended} > {full}");
    assert!(full > compact, "{full} > {compact}");
    assert!(compact > lite, "{compact} > {lite}");
    assert!(lite > map, "{lite} > {map}");

    // Budget between full and full_extended resolves to Full.
    let outcome = fit_tier(&measurer, &budget_for(full));
    assert_eq!(outcome.mode, ContextMode::Full);
    assert!(outcome.fits);

    // Budget between lite and compact resolves to Lite.
    let outcome = fit_tier(&measurer, &budget_for(lite));
    assert_eq!(outcome.mode, ContextMode::Lite);
    assert!(outcome.fits);
}

#[test]
fn fit_tier_falls_to_map_with_fits_false_when_nothing_fits() {
    let (dir, graph) = fixture();
    let measurer = TierMeasurer::new(&graph, dir.path());
    let outcome = fit_tier(&measurer, &budget_for(1));
    assert_eq!(outcome.mode, ContextMode::Map);
    assert!(!outcome.fits, "even Map exceeds a 1-token budget");
    assert!(outcome.measured_tokens > outcome.budget_tokens);
}

#[test]
fn fit_tier_short_circuits_io_when_full_fits() {
    let (dir, graph) = fixture();
    let measurer = TierMeasurer::new(&graph, dir.path());
    let full = measurer.measure(&ContextMode::Full);
    let reads_before = measurer.io_reads();
    let outcome = fit_tier(&measurer, &budget_for(usize::MAX));
    assert_eq!(outcome.mode, ContextMode::FullExtended);
    assert_eq!(
        measurer.io_reads(),
        reads_before,
        "FullExtended/Full are scan-time counts; no file I/O expected"
    );
}

#[test]
fn fit_budget_usable_subtracts_reserve_and_applies_ratio() {
    let budget = FitBudget::new(100_000, 65_536, 0.70);
    // output_reserve = min(65_536, 100_000/4) = 25_000; usable = 75_000 * 0.70
    assert_eq!(budget.output_reserve, 25_000);
    assert_eq!(budget.usable(), 52_500);
}

#[test]
fn requested_mode_parse_accepts_auto_and_tiers() {
    assert_eq!(RequestedMode::parse("auto").unwrap(), RequestedMode::Auto);
    assert_eq!(
        RequestedMode::parse("lite").unwrap(),
        RequestedMode::Fixed(ContextMode::Lite)
    );
    assert_eq!(
        RequestedMode::parse("full_extended").unwrap(),
        RequestedMode::Fixed(ContextMode::FullExtended)
    );
    assert!(RequestedMode::parse("bogus").is_err());
}
