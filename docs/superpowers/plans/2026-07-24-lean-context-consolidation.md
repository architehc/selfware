# Lean Context Consolidation + Auto-Tier Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the evolve `ContextComposer` auto-fit its context tier to the active model's window (measured, not heuristic), unify the agent's parallel `ContextLevel` vocabulary onto the shared `ContextMode`, and prune the dead `ContextGuard`.

**Architecture:** New `src/evolve/context_fit.rs` resolves a request-level `RequestedMode::Auto` to the richest concrete tier whose *measured* token size fits `fit_ratio × (context_length − output_reserve)`, lazily evaluating the ladder FullExtended → Full → Compact → Lite → Map. The evolve server resolves `auto` at build/refresh; the composer only ever holds concrete tiers. Skeleton extraction moves to `src/evolve/skeleton.rs` so the agent's L2 and the composer's Lite measurement share one implementation.

**Tech Stack:** Rust (single crate `selfware`), axum evolve server, `crate::token_count::estimate_content_tokens` (HF tokenizer → tiktoken fallback) for all token counting.

Spec: `docs/superpowers/specs/2026-07-24-lean-context-consolidation-design.md` (read it first).

## Global Constraints

- All token counting goes through `crate::token_count::estimate_content_tokens` — never new bytes/4-style heuristics (CONSOLIDATION_PLAN item 4).
- No new crate dependencies. `tempfile` is already available for tests.
- `ContextMode` gains **no** new variants. `Auto` lives only in `RequestedMode` (request/config level). The composer never stores `Auto`.
- Unit tests follow the in-source `#[cfg(test)] #[path = "../../tests/unit/..."]` pattern used by `src/evolve/context_reduce.rs:280-282`.
- Evolve server tests live in `tests/evolve/`, registered in `tests/evolve/mod.rs`, using its existing axum `oneshot` helpers.
- Verify each task with `cargo test --test unit` / `cargo test --test evolve` (scoped) before committing.
- Commit after every task. Do not push.

---

### Task 1: Move skeleton extraction into `src/evolve/skeleton.rs`

Skeleton extraction currently lives in `src/agent/context_map.rs` but the Lite-tier
measurement (Task 2) needs it in evolve. Move it verbatim; leave a re-export so
agent call sites keep compiling.

**Files:**
- Create: `src/evolve/skeleton.rs`
- Modify: `src/evolve/mod.rs` (register module + re-export, follow the existing pattern at line ~45)
- Modify: `src/agent/context_map.rs:37-166,1316-1566` (delete moved items, add re-export)
- Test: `tests/unit/agent/context_map/context_map_test.rs` (unchanged — exercises the re-export)

**Interfaces:**
- Produces (used by Tasks 2 and 5):
  - `selfware::evolve::skeleton::{SkeletonItem, FileSkeleton, extract_rust_skeleton}`
  - `extract_rust_skeleton(path: &Path, content: &str) -> FileSkeleton`
  - `FileSkeleton { path: PathBuf, items: Vec<SkeletonItem>, token_count: usize }` with `fn render(&self) -> String`

- [ ] **Step 1: Create `src/evolve/skeleton.rs`**

Header plus verbatim moves from `src/agent/context_map.rs`:

```rust
//! Skeleton (signature-level) extraction for Rust sources.
//!
//! Shared by the agent's L2 context level (`agent::context_map`) and the
//! evolve composer's Lite-tier measurement (`evolve::context_fit`). This is
//! intentionally fast and approximate — regex-style line scanning, not a full
//! AST parse.

use std::path::{Path, PathBuf};

use crate::token_count::estimate_content_tokens;
```

Then move, unchanged:
- `SkeletonItem` enum and `FileSkeleton` struct + `impl FileSkeleton { render }`
  (currently `src/agent/context_map.rs:37-166`)
- `extract_rust_skeleton` and its private helpers `is_fn_line`, `extract_fn_name`,
  `is_struct_line`, `is_enum_line`, `is_trait_line`, `is_impl_line`,
  `extract_name_after`, `extract_const_parts`, `extract_impl_target`
  (currently `src/agent/context_map.rs:1316-1566`)

- [ ] **Step 2: Register the module in `src/evolve/mod.rs`**

Add `pub mod skeleton;` with the other module declarations and add
`skeleton::{extract_rust_skeleton, FileSkeleton, SkeletonItem}` to the public
re-export block (mirror how `ContextComposer` is re-exported around line 45).

- [ ] **Step 3: Replace the moved code in `src/agent/context_map.rs`**

Delete the items listed in Step 1 from `src/agent/context_map.rs` and add:

```rust
pub use crate::evolve::skeleton::{extract_rust_skeleton, FileSkeleton, SkeletonItem};
```

Keep the `use crate::token_count::estimate_content_tokens;` import at the top —
`load_full` and `add_external_context` still use it.

- [ ] **Step 4: Verify**

Run: `cargo test --test unit context_map`
Expected: PASS (existing tests exercise the re-export; no test edits needed)

- [ ] **Step 5: Commit**

```bash
git add src/evolve/skeleton.rs src/evolve/mod.rs src/agent/context_map.rs
git commit -m "refactor: move skeleton extraction to evolve::skeleton for shared use"
```

---

### Task 2: `src/evolve/context_fit.rs` — budget, measurer, ladder

The core of the auto-tier feature. Pure module: no server, no config dependency.

**Files:**
- Create: `src/evolve/context_fit.rs`
- Modify: `src/evolve/mod.rs` (register + re-export)
- Test: `tests/unit/evolve/context_fit_test.rs` (wired via in-source `#[path]` mod)

**Interfaces:**
- Consumes: `evolve::skeleton::extract_rust_skeleton` (Task 1), `evolve::context_reduce::reduce_source`, `evolve::map::build_map`, `evolve::{ContextMode, Graph, NodeLayer}`, `crate::token_count::estimate_content_tokens`
- Produces (used by Tasks 3-4):
  - `RequestedMode::{Auto, Fixed(ContextMode)}`, `RequestedMode::parse(&str) -> Result<Self, String>`, `RequestedMode::name(&self) -> &'static str`
  - `FitBudget { context_length, output_reserve, fit_ratio }`, `FitBudget::new(context_length: usize, max_tokens: usize, fit_ratio: f64) -> Self`, `FitBudget::usable(&self) -> usize`
  - `TierMeasurer<'a>::new(graph: &'a Graph, root: &'a Path) -> Self`, `.measure(&self, mode: &ContextMode) -> usize`, `.io_reads(&self) -> usize`
  - `FitOutcome { mode: ContextMode, measured_tokens: usize, budget_tokens: usize, fits: bool }`
  - `fit_tier(measurer: &TierMeasurer, budget: &FitBudget) -> FitOutcome`
  - `TIER_LADDER: [ContextMode; 5]`

- [ ] **Step 1: Write the failing test `tests/unit/evolve/context_fit_test.rs`**

```rust
use std::fs;

use selfware::evolve::context_fit::{
    fit_tier, FitBudget, RequestedMode, TierMeasurer,
};
use selfware::evolve::{ContextMode, Graph, Node};

/// One Rust file whose Full > Compact > Lite, with an inline test block so
/// FullExtended > Full.
const ALPHA_RS: &str = r#"
//! Module doc comment that compact strips.

/// Doc comment on a public function.
pub fn alpha_one(x: usize) -> usize {
    // line comment
    let y = x + 1;
    y * 2
}

pub fn alpha_two(s: &str) -> String {
    format!("{s}!")
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

    (dir, Graph { nodes: vec![code, test_node], edges: vec![] })
}

fn budget_for(tokens: usize) -> FitBudget {
    // fit_ratio 1.0 and zero reserve so `usable()` is exactly `tokens`.
    FitBudget { context_length: tokens, output_reserve: 0, fit_ratio: 1.0 }
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
```

Wire it from the new source file (mirroring `src/evolve/context_reduce.rs:280-282`):

```rust
#[cfg(test)]
#[path = "../../tests/unit/evolve/context_fit_test.rs"]
mod context_fit_test;
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test unit context_fit`
Expected: FAIL to compile (`selfware::evolve::context_fit` does not exist)

- [ ] **Step 3: Implement `src/evolve/context_fit.rs`**

```rust
//! Auto-fitting the context tier to the model's window.
//!
//! `RequestedMode` is what config and the HTTP API ask for (`auto` or a pinned
//! tier). `fit_tier` resolves `auto` to the richest concrete tier whose
//! *measured* token size fits the usable budget. Reduced tiers are measured by
//! really projecting the sources (strip comments / skeleton / component map),
//! never by fixed fractions — the fractions remain only as per-node fallbacks
//! when a file cannot be read.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::Path;

use super::context_reduce::reduce_source;
use super::map::build_map;
use super::skeleton::extract_rust_skeleton;
use super::{ContextMode, Graph, NodeLayer};

/// What the operator asked for: automatic fitting or a pinned tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestedMode {
    Auto,
    Fixed(ContextMode),
}

impl RequestedMode {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "map" => Ok(Self::Fixed(ContextMode::Map)),
            "lite" => Ok(Self::Fixed(ContextMode::Lite)),
            "compact" | "skeleton" => Ok(Self::Fixed(ContextMode::Compact)),
            "full" => Ok(Self::Fixed(ContextMode::Full)),
            "full_extended" => Ok(Self::Fixed(ContextMode::FullExtended)),
            other => Err(format!(
                "unknown context mode '{other}' \
                 (expected auto|map|lite|compact|full|full_extended)"
            )),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Fixed(mode) => mode.name(),
        }
    }
}

/// Token budget for one composed context, derived from the model window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FitBudget {
    pub context_length: usize,
    pub output_reserve: usize,
    pub fit_ratio: f64,
}

impl FitBudget {
    /// `output_reserve` follows the server's existing convention
    /// (`evolve/server.rs:117`): `max_tokens.min(context_length / 4)`.
    pub fn new(context_length: usize, max_tokens: usize, fit_ratio: f64) -> Self {
        Self {
            context_length,
            output_reserve: max_tokens.min(context_length / 4),
            fit_ratio,
        }
    }

    /// Tokens the composed context may occupy.
    pub fn usable(&self) -> usize {
        (self.context_length.saturating_sub(self.output_reserve) as f64 * self.fit_ratio) as usize
    }
}

/// The tier ladder, richest first.
pub const TIER_LADDER: [ContextMode; 5] = [
    ContextMode::FullExtended,
    ContextMode::Full,
    ContextMode::Compact,
    ContextMode::Lite,
    ContextMode::Map,
];

/// Result of fitting the tier ladder to a budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FitOutcome {
    pub mode: ContextMode,
    pub measured_tokens: usize,
    pub budget_tokens: usize,
    /// False only when even the smallest tier (Map) exceeds the budget — the
    /// caller must surface that as a warning, never silently truncate.
    pub fits: bool,
}

/// Per-node fallback fractions (measured tree-wide) used only when a node's
/// source file cannot be read.
const SIGNATURE_FRACTION: f64 = 0.18;
const COMMENT_STRIPPED_FRACTION: f64 = 0.82;

/// Measures the real token cost of each concrete tier for one graph snapshot.
///
/// Full/FullExtended come from scan-time token counts (no I/O). Compact, Lite
/// and Map read and project sources on first request and cache the result for
/// the lifetime of the measurer (one graph revision).
pub struct TierMeasurer<'a> {
    graph: &'a Graph,
    root: &'a Path,
    cache: RefCell<HashMap<&'static str, usize>>,
    io_reads: Cell<usize>,
}

impl<'a> TierMeasurer<'a> {
    pub fn new(graph: &'a Graph, root: &'a Path) -> Self {
        Self {
            graph,
            root,
            cache: RefCell::new(HashMap::new()),
            io_reads: Cell::new(0),
        }
    }

    /// Number of source files actually read (observability for tests/logs).
    pub fn io_reads(&self) -> usize {
        self.io_reads.get()
    }

    pub fn measure(&self, mode: &ContextMode) -> usize {
        match mode {
            ContextMode::FullExtended => self.measured_full_extended(),
            ContextMode::Full => self.measured_full(),
            ContextMode::Compact => self.cached("compact", Self::measure_compact),
            ContextMode::Lite => self.cached("lite", Self::measure_lite),
            ContextMode::Map => self.cached("map", Self::measure_map),
            ContextMode::Preset(_) => self.measured_full(),
        }
    }

    fn cached(&self, key: &'static str, f: fn(&Self) -> usize) -> usize {
        if let Some(hit) = self.cache.borrow().get(key) {
            return *hit;
        }
        let value = f(self);
        self.cache.borrow_mut().insert(key, value);
        value
    }

    fn measured_full_extended(&self) -> usize {
        self.graph
            .nodes
            .iter()
            .filter(|n| matches!(n.layer, NodeLayer::Code | NodeLayer::Test))
            .map(|n| n.tokens)
            .sum()
    }

    fn measured_full(&self) -> usize {
        self.graph
            .nodes
            .iter()
            .filter(|n| n.layer == NodeLayer::Code)
            .map(|n| n.tokens.saturating_sub(n.inline_test_tokens))
            .sum()
    }

    fn read_node_source(&self, node: &super::Node) -> Option<(String, String)> {
        let rel = node.path.as_deref()?;
        let src = std::fs::read_to_string(self.root.join(rel)).ok()?;
        self.io_reads.set(self.io_reads.get() + 1);
        Some((rel.to_string(), src))
    }

    fn measure_compact(&self) -> usize {
        let mut total = 0usize;
        for node in self.graph.nodes.iter().filter(|n| n.layer == NodeLayer::Code) {
            let code_tokens = node.tokens.saturating_sub(node.inline_test_tokens);
            total += match self.read_node_source(node) {
                Some((_, src)) => {
                    crate::token_count::estimate_content_tokens(&reduce_source(&src))
                }
                None => (code_tokens as f64 * COMMENT_STRIPPED_FRACTION).round() as usize,
            };
        }
        total
    }

    fn measure_lite(&self) -> usize {
        let mut total = 0usize;
        for node in self.graph.nodes.iter().filter(|n| n.layer == NodeLayer::Code) {
            let code_tokens = node.tokens.saturating_sub(node.inline_test_tokens);
            total += match self.read_node_source(node) {
                Some((rel, src)) if rel.ends_with(".rs") => {
                    extract_rust_skeleton(Path::new(&rel), &src).token_count
                }
                _ => (code_tokens as f64 * SIGNATURE_FRACTION).round() as usize,
            };
        }
        total
    }

    fn measure_map(&self) -> usize {
        build_map(self.graph, self.root).map_tokens
    }
}

/// Resolve `auto`: the richest tier whose measured size fits `budget.usable()`.
pub fn fit_tier(measurer: &TierMeasurer, budget: &FitBudget) -> FitOutcome {
    let usable = budget.usable();
    for mode in TIER_LADDER {
        let measured = measurer.measure(&mode);
        if measured <= usable {
            return FitOutcome {
                mode,
                measured_tokens: measured,
                budget_tokens: usable,
                fits: true,
            };
        }
    }
    FitOutcome {
        mode: ContextMode::Map,
        measured_tokens: measurer.measure(&ContextMode::Map),
        budget_tokens: usable,
        fits: false,
    }
}
```

Note: `Node` fields used above (`tokens`, `inline_test_tokens`, `path`, `layer`)
match `src/evolve/mod.rs:112` and `evolve/graph.rs:785-818`. If `Node::code`
sets different defaults than the test expects, adjust the test fixture — do not
change production field semantics.

- [ ] **Step 4: Register the module in `src/evolve/mod.rs`**

Add `pub mod context_fit;` and re-export
`context_fit::{fit_tier, FitBudget, FitOutcome, RequestedMode, TierMeasurer, TIER_LADDER}`.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --test unit context_fit`
Expected: PASS (5 tests)

- [ ] **Step 6: Commit**

```bash
git add src/evolve/context_fit.rs src/evolve/mod.rs tests/unit/evolve/context_fit_test.rs
git commit -m "feat(evolve): measured tier fitting ladder for auto context mode"
```

---

### Task 3: Config keys `context_mode` + `context_fit_ratio`

**Files:**
- Modify: `src/config/mod.rs` (near `default_context_length` / the `Config` struct at :115-124)
- Test: `tests/unit/config/` — follow the existing config test pattern (look at `tests/unit/config/model/model_test.rs` referenced from `src/config/model.rs:196-198`; add the test to the config mod's existing test file if one exists for `mod.rs`, otherwise create `tests/unit/config/context_fit_test.rs` and wire it with the same `#[path]` pattern)

**Interfaces:**
- Consumes: `RequestedMode::parse` (Task 2) for validation semantics
- Produces: `Config.context_mode: String` (default `"auto"`), `Config.context_fit_ratio: f64` (default `0.70`), `default_context_mode()`, `default_context_fit_ratio()`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn context_mode_defaults_to_auto_and_fit_ratio_to_070() {
    let config: Config = toml::from_str("").unwrap();
    assert_eq!(config.context_mode, "auto");
    assert!((config.context_fit_ratio - 0.70).abs() < f64::EPSILON);
}

#[test]
fn context_mode_and_fit_ratio_parse_from_toml() {
    let config: Config = toml::from_str(
        "context_mode = \"compact\"\ncontext_fit_ratio = 0.5\n",
    )
    .unwrap();
    assert_eq!(config.context_mode, "compact");
    assert!((config.context_fit_ratio - 0.5).abs() < f64::EPSILON);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test unit context_mode_defaults`
Expected: FAIL (unknown fields / missing fields on `Config`)

- [ ] **Step 3: Implement**

In `src/config/mod.rs`, next to `default_context_length`:

```rust
pub fn default_context_mode() -> String {
    "auto".to_string()
}
pub fn default_context_fit_ratio() -> f64 {
    0.70
}
```

In the `Config` struct, after `context_length`:

```rust
/// Evolve context tier: "auto" (fit to the model window) or a pinned tier
/// (map|lite|compact|full|full_extended).
#[serde(default = "default_context_mode")]
pub context_mode: String,
/// Fraction of the usable window a composed context may occupy in auto mode.
#[serde(default = "default_context_fit_ratio")]
pub context_fit_ratio: f64,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test unit context`
Expected: PASS (new tests + existing config tests)

- [ ] **Step 5: Commit**

```bash
git add src/config/mod.rs tests/unit/config/
git commit -m "feat(config): context_mode and context_fit_ratio keys"
```

---

### Task 4: Evolve server wiring — resolve `auto` at build/refresh and via API

**Files:**
- Modify: `src/evolve/server.rs` (struct at :43-68, `build` at :92-153, `refresh_graph` at :250-270, `context_mode_handler` at :805-836, `context_json` at :838-852)
- Test: `tests/evolve/auto_context_fit_test.rs` (new; register in `tests/evolve/mod.rs`)

**Interfaces:**
- Consumes: `fit_tier, FitBudget, RequestedMode, TierMeasurer` (Task 2), `Config.context_mode/context_fit_ratio` (Task 3)
- Produces: server resolves `auto` on startup, refresh, and `POST /api/context/mode {"mode":"auto"}`; `context_json` gains a `requested_mode` field

- [ ] **Step 1: Write the failing test `tests/evolve/auto_context_fit_test.rs`**

Register `mod auto_context_fit_test;` in `tests/evolve/mod.rs` (alphabetical, near `mod context_test;`). Reuse the `oneshot` helpers already in `tests/evolve/mod.rs` — look at `tests/evolve/context_test.rs` for the exact helper names and match that style.

```rust
use std::fs;

use selfware::config::Config;
use selfware::evolve::{EvolveServer, Graph, Node};

/// Build a temp project with one real Rust file and a matching graph node.
fn fixture(tokens: usize) -> (tempfile::TempDir, Graph) {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let body = "pub fn f() {}\n".repeat((tokens / 4).max(1));
    fs::write(src_dir.join("a.rs"), &body).unwrap();
    let mut node = Node::code("crate::a", "src/a.rs");
    node.tokens = selfware::token_count::estimate_content_tokens(&body);
    (dir, Graph { nodes: vec![node], edges: vec![] })
}

#[tokio::test]
async fn auto_mode_resolves_full_on_large_window_and_degrades_on_small() {
    let (dir, graph) = fixture(20_000);

    // Large window: auto resolves to a full tier and fits.
    let mut config = Config::default();
    config.context_length = 1_000_000;
    config.context_mode = "auto".to_string();
    let server = EvolveServer::with_config(graph.clone(), dir.path(), &config).unwrap();
    let json = get_json(&server, "/api/context").await; // use the mod.rs helper
    assert!(["full", "full_extended"].contains(&json["mode"].as_str().unwrap()));
    assert_eq!(json["requested_mode"].as_str().unwrap(), "auto");
    assert_eq!(json["fits_context_window"].as_bool().unwrap(), true);

    // 8k window: auto degrades below full, still reports coherently.
    let mut config = Config::default();
    config.context_length = 8_192;
    config.context_mode = "auto".to_string();
    let server = EvolveServer::with_config(graph, dir.path(), &config).unwrap();
    let json = get_json(&server, "/api/context").await;
    assert!(["map", "lite", "compact"].contains(&json["mode"].as_str().unwrap()));
    assert_eq!(json["requested_mode"].as_str().unwrap(), "auto");
}

#[tokio::test]
async fn invalid_context_mode_in_config_is_an_error() {
    let (dir, graph) = fixture(1_000);
    let mut config = Config::default();
    config.context_mode = "bogus".to_string();
    assert!(EvolveServer::with_config(graph, dir.path(), &config).is_err());
}

#[tokio::test]
async fn post_mode_auto_refits_and_pinned_mode_sticks() {
    let (dir, graph) = fixture(20_000);
    let mut config = Config::default();
    config.context_length = 1_000_000;
    let server = EvolveServer::with_config(graph, dir.path(), &config).unwrap();

    let json = post_mode(&server, "auto").await; // helper: POST /api/context/mode with session header
    assert_eq!(json["requested_mode"].as_str().unwrap(), "auto");

    let json = post_mode(&server, "lite").await;
    assert_eq!(json["mode"].as_str().unwrap(), "lite");
    assert_eq!(json["requested_mode"].as_str().unwrap(), "lite");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test evolve auto_context_fit`
Expected: FAIL (`requested_mode` missing; `"auto"` rejected by `context_mode_handler`)

- [ ] **Step 3: Implement server changes**

In `src/evolve/server.rs`:

a) Import and struct fields (struct at :43-68):

```rust
use super::context_fit::{fit_tier, FitBudget, FitOutcome, RequestedMode, TierMeasurer};
```

```rust
    requested_mode: Arc<RwLock<RequestedMode>>,
    fit_budget: FitBudget,
```

b) In `build()` replace lines :111-112 (`let mut composer ...; composer.set_mode(ContextMode::Full);`) with:

```rust
        let requested = RequestedMode::parse(&config.context_mode).map_err(anyhow::Error::msg)?;
        if !(0.1..=1.0).contains(&config.context_fit_ratio) {
            anyhow::bail!(
                "context_fit_ratio must be within 0.1..=1.0, got {}",
                config.context_fit_ratio
            );
        }
        let fit_budget = FitBudget::new(config.context_length, config.max_tokens, config.context_fit_ratio);
        let mut composer = ContextComposer::new(graph.clone());
        composer.set_mode(fit_mode(&requested, &graph, &project_root, &fit_budget));
```

and store the new fields in the struct literal:

```rust
            requested_mode: Arc::new(RwLock::new(requested)),
            fit_budget,
```

c) Add a free helper next to `refresh_graph`:

```rust
/// Resolve a requested mode against the current graph. `Auto` measures the
/// tier ladder and logs the decision — a warning when even Map overflows.
fn fit_mode(
    requested: &RequestedMode,
    graph: &Graph,
    root: &Path,
    budget: &FitBudget,
) -> ContextMode {
    match requested {
        RequestedMode::Fixed(mode) => mode.clone(),
        RequestedMode::Auto => {
            let outcome: FitOutcome = fit_tier(&TierMeasurer::new(graph, root), budget);
            if outcome.fits {
                tracing::info!(
                    "auto context tier: {} ({} <= {} tokens)",
                    outcome.mode.name(),
                    outcome.measured_tokens,
                    outcome.budget_tokens
                );
            } else {
                tracing::warn!(
                    "auto context tier: even map tier ({} tokens) exceeds the {}-token \
                     budget; selecting map and relying on the overflow backstop",
                    outcome.measured_tokens,
                    outcome.budget_tokens
                );
            }
            outcome.mode
        }
    }
}
```

d) In `refresh_graph()` replace the mode-preservation block (:264-267) with:

```rust
        let requested = self
            .requested_mode
            .read()
            .map_err(|_| anyhow::anyhow!("context lock poisoned"))?
            .clone();
        let mut composer = ContextComposer::new(refreshed.clone());
        composer.set_mode(fit_mode(&requested, &refreshed, &self.project_root, &self.fit_budget));
        *current = composer;
```

(Delete the now-unused `let current_mode = current.mode().clone();` line.)

e) In `context_mode_handler` (:812-824) add `"auto"` handling and track the
requested mode:

```rust
    let requested = match body.mode.as_str() {
        "auto" => RequestedMode::Auto,
        "map" => RequestedMode::Fixed(ContextMode::Map),
        "lite" => RequestedMode::Fixed(ContextMode::Lite),
        "compact" | "skeleton" => RequestedMode::Fixed(ContextMode::Compact),
        "full" => RequestedMode::Fixed(ContextMode::Full),
        "full_extended" => RequestedMode::Fixed(ContextMode::FullExtended),
        "preset" => RequestedMode::Fixed(ContextMode::Preset(
            body.preset
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| bad_request("preset mode requires a preset name"))?,
        )),
        _ => return Err(bad_request("unknown context mode")),
    };
    *server
        .requested_mode
        .write()
        .map_err(|_| internal_error(anyhow::anyhow!("context lock poisoned")))? = requested.clone();
    let mode = match &requested {
        RequestedMode::Fixed(mode) => mode.clone(),
        RequestedMode::Auto => {
            let graph = server.graph_snapshot().map_err(internal_error)?;
            fit_mode(&requested, &graph, &server.project_root, &server.fit_budget)
        }
    };
```

then keep the existing `composer.set_mode(mode); composer.summary()` block, and
pass the requested mode into `context_json` (see f).

f) `context_json` gains the requested mode (update all call sites —
`workspace_handler` :283, `context_handler` :320, `context_mode_handler` :834):

```rust
fn context_json(summary: &super::ContextSummary, context_length: usize, requested: &RequestedMode) -> Result<Value> {
    let mut value = serde_json::to_value(summary)?;
    if let Some(object) = value.as_object_mut() {
        object.insert("context_length".to_string(), json!(context_length));
        object.insert("requested_mode".to_string(), json!(requested.name()));
        object.insert(
            "fits_context_window".to_string(),
            json!(summary.estimated_tokens <= context_length),
        );
        object.insert(
            "overflow_tokens".to_string(),
            json!(summary.estimated_tokens.saturating_sub(context_length)),
        );
    }
    Ok(value)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --test evolve auto_context_fit` and `cargo test --test evolve context`
Expected: PASS (new tests + existing context/server tests — watch for the
`context_json` signature change breaking other tests; fix call sites)

- [ ] **Step 5: Commit**

```bash
git add src/evolve/server.rs tests/evolve/auto_context_fit_test.rs tests/evolve/mod.rs
git commit -m "feat(evolve): resolve auto context tier at build, refresh, and mode API"
```

---

### Task 5: Unify `ContextLevel` onto `ContextMode` in the agent

Mechanical vocabulary unification. The `ContextMap` engine, budget math, LRU
eviction, and modalities are unchanged — only the enum changes.

Mapping: `ContextLevel::Tree` → `ContextMode::Map`,
`ContextLevel::Skeleton` → `ContextMode::Lite`, `ContextLevel::Full` → `ContextMode::Full`.

**Files:**
- Modify: `src/agent/context_map.rs` (remove enum at :24-33; ~30 use sites)
- Modify: `src/agent/context_management.rs` (~15 sites, e.g. :315-618)
- Modify: `src/agent/mod.rs:1646,1657`
- Modify: `src/agent/recovery.rs:809,823,827`
- Modify: `tests/unit/agent/context_map/context_map_test.rs` (~20 sites)
- Modify: `tests/integration/live_context_tests.rs` (~8 sites)

**Interfaces:**
- Consumes: `crate::evolve::ContextMode` (existing; no `Auto` variant is added)
- Produces: no new API — all existing `ContextMap` method signatures keep their
  shapes with `ContextMode` in place of `ContextLevel`

- [ ] **Step 1: Delete the enum and rename variants**

In `src/agent/context_map.rs` delete the `ContextLevel` enum (:24-33) and add:

```rust
use crate::evolve::ContextMode;
```

Then apply these exact replacements in all six files listed above (order
matters — variants first, then the type name):

- `ContextLevel::Tree` → `ContextMode::Map`
- `ContextLevel::Skeleton` → `ContextMode::Lite`
- `ContextLevel::Full` → `ContextMode::Full`
- remaining `ContextLevel` (type positions) → `ContextMode`
- `super::context_map::ContextLevel` / `context_map::ContextLevel` import paths → `crate::evolve::ContextMode`

- [ ] **Step 2: Fix non-Copy fallout**

`ContextLevel` was `Copy`; `ContextMode` is not (it has `Preset(String)`).
Run `cargo check 2>&1 | head -50` and fix each move-out-of-borrow error by
cloning. Known sites from exploration:

- `ContextMap::level_of`: `self.entries.get(path).map(|e| e.level.clone())`
- `compress_to_fit` candidate collection: `map(|e| (e.path.clone(), e.last_accessed, e.level.clone()))`
- `ContextSuggestion { current_level: entry.level.clone(), ... }` (two sites in `recommend_context`)
- Exhaustiveness: `match` arms on the old 3-variant enum now need a catch-all.
  In `stats()` add `_ => {}` (Compact/FullExtended/Preset never occur as agent
  runtime levels). In `estimate_level_tokens` add `_ => full-file estimate`
  (same arm body as `ContextMode::Full`).
- Do **not** add derives to `ContextMode` to make this compile. If a site needs
  `Ord`/`Hash` on the level (none were found in exploration), stop and
  re-discuss instead of widening the shared enum.

- [ ] **Step 3: Fix the doc header**

Update the module doc comment at `src/agent/context_map.rs:1-9` so the L1/L2/L3
legend names the shared tiers: L1 tree → `ContextMode::Map`, L2 skeleton →
`ContextMode::Lite`, L3 full → `ContextMode::Full`.

- [ ] **Step 4: Verify**

Run: `cargo test --test unit context_map`
Expected: PASS
Run: `cargo test --test unit` (full unit suite)
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/agent/ tests/unit/agent/context_map/ tests/integration/live_context_tests.rs
git commit -m "refactor(agent): unify ContextLevel onto shared evolve::ContextMode"
```

---

### Task 6: Prune the dead `ContextGuard`

**Files:**
- Delete: `src/safety/context_guard.rs`
- Modify: `src/safety/mod.rs:10,29-32`
- Modify: `src/evolve/presets.rs:103-114` (obsolete seed referencing context_guard)
- Modify: `docs/CONSOLIDATION_PLAN.md` (append decision record)
- Test: `tests/unit/evolve/presets_test.rs` (only if it asserts the removed seed id)

**Interfaces:**
- Produces: nothing — pure removal. Any compile error naming `ContextGuard`,
  `TaintLevel`, `ContextPollutionKind`, `ContextSourceProvenance`, or
  `ContextTraceabilityReport` outside the deleted file is a bug to fix by
  removing that reference, not by keeping the module.

- [ ] **Step 1: Confirm the guard is still dead**

Run: `grep -rn "ContextGuard\|context_guard" src/ tests/ --include="*.rs" | grep -v "src/safety/" | grep -v "presets"`
Expected: no production call sites (only the presets seed text and possibly
test references). If a real call site appeared since this plan was written,
STOP and re-discuss instead of deleting.

- [ ] **Step 2: Delete and clean references**

```bash
rm src/safety/context_guard.rs
```

In `src/safety/mod.rs` remove `pub mod context_guard;` (:10) and the
`pub use context_guard::{...}` re-export (:29-32).

In `src/evolve/presets.rs` delete the entire `Seed { id: "unify-safety-gate-context", ... }`
entry (:103-114) — its premise (merge into context_guard) is obsolete.
Check `tests/unit/evolve/presets_test.rs` for that seed id and update the test
(count assertions or id lists) to match.

- [ ] **Step 3: Record the decision**

Append to `docs/CONSOLIDATION_PLAN.md`:

```markdown
## 8. ContextGuard pruned (2026-07-24)

`src/safety/context_guard.rs` (620 lines, heuristic substring scanner) had zero
production call sites — only its own tests. Deleted along with the
`safety::context_guard` re-exports. If injection defense is needed later
(roadmap Rec 2), rebuild it against a real ingestion path (web search / MCP
payloads entering the agent loop) rather than resurrecting the heuristic scanner.
```

- [ ] **Step 4: Verify**

Run: `cargo test --test unit safety` and `cargo test --test evolve presets`
Expected: PASS
Run: `cargo check --all-targets`
Expected: no errors

- [ ] **Step 5: Commit**

```bash
git add -A src/safety/ src/evolve/presets.rs tests/unit/evolve/presets_test.rs docs/CONSOLIDATION_PLAN.md
git commit -m "chore(safety): prune dead ContextGuard heuristic scanner"
```

---

### Task 7: Documentation sweep

**Files:**
- Modify: `docs/superpowers/specs/2026-07-18-self-evolve-context-selector-design.md` (stale §5)
- Modify: `docs/configuration.md` (new keys)

- [ ] **Step 1: Annotate the stale spec**

At the top of `docs/superpowers/specs/2026-07-18-self-evolve-context-selector-design.md`
add:

```markdown
> **2026-07-24 update:** §5 "Context Loading Modes" predates the `Map`/`Compact`
> tiers and the auto-fitting ladder. See
> `2026-07-24-lean-context-consolidation-design.md` for the current tier model
> (Map/Lite/Compact/Full/FullExtended + `auto`).
```

- [ ] **Step 2: Document the new config keys**

In `docs/configuration.md`, in the section covering `context_length`, add:

```markdown
### `context_mode`

Evolve workspace context tier. `"auto"` (default) measures each tier and picks
the richest one fitting `context_fit_ratio` of the usable window; or pin a tier:
`"map"`, `"lite"`, `"compact"`, `"full"`, `"full_extended"`.

### `context_fit_ratio`

Fraction of the usable window (`context_length` minus output reserve) the
composed context may occupy in `auto` mode. Default `0.70`, range `0.1..=1.0`.
```

- [ ] **Step 3: Commit**

```bash
git add docs/
git commit -m "docs: auto context tier configuration and stale spec annotation"
```

---

### Task 8: Live smoke test — OpenRouter Kimi

Verifies the feature end-to-end against the real model the user runs. Run
interactively (needs the user's API key), not by a subagent.

**Files:**
- Create (scratch, not committed): `/tmp/selfware-kimi-smoke/selfware.toml`

- [ ] **Step 1: Preflight**

```bash
test -n "$OPENROUTER_API_KEY" && echo key-present || echo key-missing
```

If missing, ask the user for it (or for the location of their selfware config
with an OpenRouter `api_key`). Do not proceed without a key.

- [ ] **Step 2: Discover the exact Kimi model id and its context window**

```bash
curl -s https://openrouter.ai/api/v1/models \
  -H "Authorization: Bearer $OPENROUTER_API_KEY" \
  | jq -r '.data[] | select(.id | test("kimi"; "i")) | "\(.id) \(.context_length)"'
```

Pick the Kimi K3 entry the user means (if several, confirm with the user). Note
the `context_length` OpenRouter reports for it — use that value in the config.

- [ ] **Step 3: Build and launch the evolve server with `context_mode = "auto"`**

```bash
cargo build --release
mkdir -p /tmp/selfware-kimi-smoke
cat > /tmp/selfware-kimi-smoke/selfware.toml <<EOF
endpoint = "https://openrouter.ai/api/v1"
model = "<model id from step 2>"
api_key = "$OPENROUTER_API_KEY"
context_length = <context_length from step 2>
context_mode = "auto"
EOF
```

Find the exact evolve subcommand (`./target/release/selfware --help | grep -i -A2 evolve`;
the dispatch is `src/cli/mod.rs:2987` → `run_self_evolve_with_config`), then
launch it from the selfware repo root with this config and watch the log for
the `auto context tier:` line.

- [ ] **Step 4: Verify large-window resolution**

```bash
curl -s http://127.0.0.1:<port>/api/workspace | jq '{mode: .context.mode, requested: .context.requested_mode, fits: .context.fits_context_window, tokens: .context.estimated_tokens, window: .context.context_length}'
```

Expected on the selfware repo with a Kimi-scale window: `requested` is `auto`,
`mode` is `full` or `full_extended`, `fits` is `true`, and
`tokens <= 0.7 * (context_length - output_reserve)`. The server log shows the
`auto context tier:` info line with measured sizes.

- [ ] **Step 5: Verify small-window degradation**

Stop the server, set `context_length = 8192` in the smoke config, relaunch, and
re-run the same curl. Expected: `mode` degrades to `map` or `lite`, and if even
`map` exceeds the budget the log shows the `auto context tier: even map tier
... exceeds` warning instead of a context-overflow failure.

- [ ] **Step 6: Verify the API round-trip**

```bash
TOKEN=$(curl -s http://127.0.0.1:<port>/api/workspace | jq -r .session_token)
curl -s -X POST http://127.0.0.1:<port>/api/context/mode \
  -H "content-type: application/json" -H "x-selfware-session: $TOKEN" \
  -d '{"mode":"auto"}' | jq '{mode, requested_mode, fits_context_window}'
curl -s -X POST http://127.0.0.1:<port>/api/context/mode \
  -H "content-type: application/json" -H "x-selfware-session: $TOKEN" \
  -d '{"mode":"bogus"}' | jq .
```

Expected: first call re-fits and returns `requested_mode: "auto"`; second
returns HTTP 400 with an `unknown context mode` error.

- [ ] **Step 7: Report**

Report resolved tiers, measured token sizes per step, and the exact model id
used. No commit (scratch config only); kill the server when done.

---

## Self-Review Notes (completed by plan author)

- Spec coverage: §3.1 Auto/RequestedMode → Tasks 2-4; §3.2 budget → Task 2;
  §3.3 measured tiers → Tasks 1-2; §3.4 config → Task 3; §3.5 picker → Task 4
  (requested/resolved in context_json; Map was already in the picker);
  §4 data flow → Task 4; §5 vocabulary unification → Tasks 1+5; §6 guard
  pruning → Task 6; §7 error handling → Tasks 2 (`fits:false`), 4 (warning),
  3-4 (validation); §8 testing → Tasks 2/4 unit+integration, Task 8 live smoke.
- The one intentionally untestable-by-unit-test behavior (no network in tests)
  is the OpenRouter round-trip — covered by Task 8.
