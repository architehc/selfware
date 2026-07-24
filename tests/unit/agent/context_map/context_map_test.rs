use super::*;

#[test]
fn test_context_map_budget_allocation() {
    let map = ContextMap::new(100_000, 0.75, 0.20, 0.05);
    assert_eq!(map.budget(), 75_000);
    assert_eq!(map.compression_headroom(), 20_000);
    assert_eq!(map.thinking_reserve(), 5_000);
    assert_eq!(map.remaining(), 75_000);
}

#[test]
fn test_register_tree_entry() {
    let mut map = ContextMap::new(100_000, 0.75, 0.20, 0.05);
    map.register_tree_entry("src/main.rs".into(), 1024);
    assert_eq!(map.file_count(), 1);
    assert_eq!(
        map.level_of(Path::new("src/main.rs")),
        Some(ContextMode::Map)
    );
    assert!(map.total_tokens() > 0);
}

#[test]
fn test_load_skeleton_upgrades_from_tree() {
    let mut map = ContextMap::new(100_000, 0.75, 0.20, 0.05);
    map.register_tree_entry("src/main.rs".into(), 1024);
    let before = map.total_tokens();

    let skeleton = FileSkeleton {
        path: "src/main.rs".into(),
        items: vec![SkeletonItem::Function {
            name: "main".into(),
            signature: "fn main()".into(),
            line: 1,
        }],
        token_count: 50,
    };
    map.load_skeleton(Path::new("src/main.rs"), skeleton);

    assert_eq!(
        map.level_of(Path::new("src/main.rs")),
        Some(ContextMode::Lite)
    );
    assert!(map.total_tokens() > before);
}

#[test]
fn test_load_full_and_downgrade() {
    let mut map = ContextMap::new(100_000, 0.75, 0.20, 0.05);
    map.register_tree_entry("src/main.rs".into(), 1024);

    // First load skeleton so we can downgrade later.
    let skeleton = FileSkeleton {
        path: "src/main.rs".into(),
        items: vec![],
        token_count: 20,
    };
    map.load_skeleton(Path::new("src/main.rs"), skeleton);

    // Load full (use enough content to exceed skeleton cost of 20 tokens).
    map.load_full(
        Path::new("src/main.rs"),
        "fn main() {\n".to_string() + &"    let x = 1;\n".repeat(50) + "}",
    );
    assert_eq!(
        map.level_of(Path::new("src/main.rs")),
        Some(ContextMode::Full)
    );
    let full_tokens = map.total_tokens();

    // Downgrade.
    let freed = map.downgrade_to_skeleton(Path::new("src/main.rs"));
    assert!(freed > 0);
    assert_eq!(
        map.level_of(Path::new("src/main.rs")),
        Some(ContextMode::Lite)
    );
    assert!(map.total_tokens() < full_tokens);
}

#[test]
fn test_compress_to_fit_frees_oldest() {
    let mut map = ContextMap::new(10_000, 0.75, 0.20, 0.05);
    // Budget is 7500 tokens.

    // Load two files at L3 with enough content to be meaningful.
    map.register_tree_entry("old.rs".into(), 100);
    map.load_skeleton(
        Path::new("old.rs"),
        FileSkeleton {
            path: "old.rs".into(),
            items: vec![],
            token_count: 50,
        },
    );
    // ~1000+ tokens of content
    map.load_full(
        Path::new("old.rs"),
        "fn process() { let x = 1; }\n".repeat(100),
    );

    // Touch new.rs after old.rs so old.rs is older.
    std::thread::sleep(std::time::Duration::from_millis(10));
    map.register_tree_entry("new.rs".into(), 100);
    map.load_skeleton(
        Path::new("new.rs"),
        FileSkeleton {
            path: "new.rs".into(),
            items: vec![],
            token_count: 50,
        },
    );
    map.load_full(
        Path::new("new.rs"),
        "fn handler() { let y = 2; }\n".repeat(100),
    );

    let before = map.total_tokens();
    // Request more space than available.
    let freed = map.compress_to_fit(map.remaining() + 500);
    assert!(freed > 0, "should have freed tokens by downgrading");
    assert!(map.total_tokens() < before);
    // old.rs should be downgraded first (it was accessed earlier).
    assert_eq!(
        map.level_of(Path::new("old.rs")),
        Some(ContextMode::Lite)
    );
}

#[tokio::test]
async fn test_can_load_estimate() {
    let mut map = ContextMap::new(1_000, 0.75, 0.20, 0.05);
    // Budget is 750 tokens.

    // Load a file at L3 that uses most of the budget.
    map.register_tree_entry("existing.rs".into(), 100);
    let big_content = "let x = 1;\n".repeat(200); // ~200 lines, ~600+ tokens
    map.load_full(Path::new("existing.rs"), big_content);

    // Now try to load another file — should not fit.
    map.register_tree_entry("new.rs".into(), 9000);
    let estimate = map.can_load(Path::new("new.rs"), ContextMode::Full).await;
    assert!(estimate.estimated_tokens > 0);
    // Budget mostly consumed + new file estimate → should not fit.
    assert!(estimate.usage_pct > 0.5, "should show significant usage");
}

#[test]
fn test_modality_detection() {
    assert!(matches!(
        ContextModality::from_task("merge the auth module with the session handler"),
        ContextModality::Merge { .. }
    ));
    assert!(matches!(
        ContextModality::from_task("review all rust files"),
        ContextModality::Review
    ));
    assert!(matches!(
        ContextModality::from_task("refactor execution.rs into smaller components"),
        ContextModality::Refactor { .. }
    ));
    assert!(matches!(
        ContextModality::from_task("debug why the no-action loop fires"),
        ContextModality::Debug { .. }
    ));
    assert!(matches!(
        ContextModality::from_task("create a brand new context_map module"),
        ContextModality::Greenfield { .. }
    ));
    // "bug" triggers Debug modality
    assert!(matches!(
        ContextModality::from_task("fix the token counting bug"),
        ContextModality::Debug { .. }
    ));
    // Pure implementation task with no modality keywords
    assert!(matches!(
        ContextModality::from_task("add a new field to AgentConfig"),
        ContextModality::Implement { .. }
    ));
}

#[test]
fn test_extract_rust_skeleton() {
    let code = r#"
use std::collections::HashMap;

pub const MAX_SIZE: usize = 100;

pub struct Config {
    name: String,
    value: usize,
}

pub enum State {
    Running,
    Stopped,
}

pub trait Handler {
    fn handle(&self);
}

impl Config {
    pub fn new(name: String) -> Self {
        Config { name, value: 0 }
    }

    pub fn value(&self) -> usize {
        self.value
    }
}

pub fn process(input: &str) -> Result<(), Error> {
    // Process the input (placeholder implementation)
    let _ = input;
    Ok(())
}

mod tests {
    use super::*;
}
"#;
    let skeleton = extract_rust_skeleton(Path::new("test.rs"), code);
    assert!(!skeleton.items.is_empty());
    assert!(skeleton.token_count > 0);

    // Check that we found the key items.
    let names: Vec<String> = skeleton
        .items
        .iter()
        .map(|item| match item {
            SkeletonItem::Function { name, .. } => format!("fn:{}", name),
            SkeletonItem::Struct { name, .. } => format!("struct:{}", name),
            SkeletonItem::Enum { name, .. } => format!("enum:{}", name),
            SkeletonItem::Trait { name, .. } => format!("trait:{}", name),
            SkeletonItem::Impl { target, .. } => format!("impl:{}", target),
            SkeletonItem::Module { name, .. } => format!("mod:{}", name),
            SkeletonItem::Const { name, .. } => format!("const:{}", name),
            SkeletonItem::Use { path, .. } => format!("use:{}", path),
        })
        .collect();

    assert!(names.contains(&"use:use std::collections::HashMap".to_string()));
    assert!(names.contains(&"const:MAX_SIZE".to_string()));
    assert!(names.contains(&"struct:Config".to_string()));
    assert!(names.contains(&"enum:State".to_string()));
    assert!(names.contains(&"trait:Handler".to_string()));
    assert!(names.iter().any(|n| n.starts_with("impl:")));
    assert!(names.contains(&"fn:new".to_string()));
    assert!(names.contains(&"fn:value".to_string()));
    assert!(names.contains(&"fn:process".to_string()));
    assert!(names.contains(&"mod:tests".to_string()));
}

#[test]
fn test_render_tree() {
    let mut map = ContextMap::new(100_000, 0.75, 0.20, 0.05);
    map.register_tree_entry("src/main.rs".into(), 1024);
    map.register_tree_entry("src/lib.rs".into(), 512);

    let tree = map.render_tree();
    assert!(tree.contains("src/main.rs"));
    assert!(tree.contains("src/lib.rs"));
    assert!(tree.contains("2 files"));
}

#[test]
fn test_render_boundary() {
    let mut map = ContextMap::new(100_000, 0.75, 0.20, 0.05);
    map.register_tree_entry("src/main.rs".into(), 1024);
    map.load_full(Path::new("src/main.rs"), "fn main() {}".into());

    let boundary = map.render_boundary();
    assert!(boundary.contains("<context_boundary>"));
    assert!(boundary.contains("1 at L3 (full)"));
    assert!(boundary.contains("src/main.rs"));
}

#[test]
fn test_loading_plan_merge() {
    let modality = ContextModality::Merge {
        source_files: vec!["a.rs".into()],
        target_files: vec!["b.rs".into()],
    };
    let plan = modality.loading_plan();
    assert_eq!(plan.l3_files.len(), 2);
}

#[test]
fn test_loading_plan_review() {
    let modality = ContextModality::Review;
    let plan = modality.loading_plan();
    // Review mode: no files pre-loaded at L3.
    assert!(plan.l3_files.is_empty());
}

// =========================================================================
// ContextMode tests
// =========================================================================

#[test]
fn test_context_mode_tier_names() {
    // The agent's L1/L2/L3 tiers map onto the shared evolve vocabulary.
    assert_eq!(ContextMode::Map.name(), "map");
    assert_eq!(ContextMode::Lite.name(), "lite");
    assert_eq!(ContextMode::Full.name(), "full");
}

#[test]
fn test_context_level_equality() {
    assert_eq!(ContextMode::Map, ContextMode::Map);
    assert_eq!(ContextMode::Lite, ContextMode::Lite);
    assert_eq!(ContextMode::Full, ContextMode::Full);
    assert_ne!(ContextMode::Map, ContextMode::Full);
}

#[test]
fn test_context_mode_preset_equality() {
    // Preset carries a name; equality is by variant + payload.
    assert_eq!(
        ContextMode::Preset("coding".to_string()),
        ContextMode::Preset("coding".to_string())
    );
    assert_ne!(
        ContextMode::Preset("coding".to_string()),
        ContextMode::Preset("review".to_string())
    );
    assert_ne!(ContextMode::Preset("full".to_string()), ContextMode::Full);
}

#[test]
fn test_context_level_clone() {
    let level = ContextMode::Lite;
    let cloned = level.clone();
    assert_eq!(level, cloned);
}

// =========================================================================
// ContextMap budget query tests
// =========================================================================

#[test]
fn test_budget_returns_content_budget() {
    let map = ContextMap::new(200_000, 0.75, 0.20, 0.05);
    assert_eq!(map.budget(), 150_000);
}

#[test]
fn test_compression_headroom_calculated() {
    let map = ContextMap::new(200_000, 0.75, 0.20, 0.05);
    assert_eq!(map.compression_headroom(), 40_000);
}

#[test]
fn test_thinking_reserve_calculated() {
    let map = ContextMap::new(200_000, 0.75, 0.20, 0.05);
    assert_eq!(map.thinking_reserve(), 10_000);
}

#[test]
fn test_remaining_equals_budget_when_empty() {
    let map = ContextMap::new(100_000, 0.75, 0.20, 0.05);
    assert_eq!(map.remaining(), map.budget());
}

#[test]
fn test_remaining_decreases_with_entries() {
    let mut map = ContextMap::new(100_000, 0.75, 0.20, 0.05);
    let before = map.remaining();
    map.register_tree_entry("test.rs".into(), 5000);
    assert!(map.remaining() < before);
}

#[test]
fn test_file_count_empty() {
    let map = ContextMap::new(100_000, 0.75, 0.20, 0.05);
    assert_eq!(map.file_count(), 0);
}

#[test]
fn test_file_count_after_registration() {
    let mut map = ContextMap::new(100_000, 0.75, 0.20, 0.05);
    map.register_tree_entry("a.rs".into(), 100);
    map.register_tree_entry("b.rs".into(), 200);
    assert_eq!(map.file_count(), 2);
}

#[test]
fn test_total_tokens_empty() {
    let map = ContextMap::new(100_000, 0.75, 0.20, 0.05);
    assert_eq!(map.total_tokens(), 0);
}

#[test]
fn test_total_tokens_increases_with_content() {
    let mut map = ContextMap::new(100_000, 0.75, 0.20, 0.05);
    map.register_tree_entry("test.rs".into(), 5000);
    assert!(map.total_tokens() > 0);
}

#[test]
fn test_level_of_nonexistent() {
    let map = ContextMap::new(100_000, 0.75, 0.20, 0.05);
    assert_eq!(map.level_of(Path::new("nonexistent.rs")), None);
}

#[test]
fn test_level_of_tree_entry() {
    let mut map = ContextMap::new(100_000, 0.75, 0.20, 0.05);
    map.register_tree_entry("test.rs".into(), 100);
    assert_eq!(map.level_of(Path::new("test.rs")), Some(ContextMode::Map));
}

// =========================================================================
// FileSkeleton render tests
// =========================================================================

#[test]
fn test_skeleton_render_function() {
    let skeleton = FileSkeleton {
        path: "src/lib.rs".into(),
        items: vec![SkeletonItem::Function {
            name: "process".to_string(),
            signature: "pub fn process(data: &[u8]) -> Result<()>".to_string(),
            line: 42,
        }],
        token_count: 10,
    };
    let rendered = skeleton.render();
    assert!(rendered.contains("src/lib.rs"));
    assert!(rendered.contains("L42:"));
    assert!(rendered.contains("pub fn process"));
}

#[test]
fn test_skeleton_render_struct() {
    let skeleton = FileSkeleton {
        path: "src/config.rs".into(),
        items: vec![SkeletonItem::Struct {
            name: "Config".to_string(),
            fields_summary: "name: String, value: usize".to_string(),
            line: 10,
        }],
        token_count: 8,
    };
    let rendered = skeleton.render();
    assert!(rendered.contains("struct Config"));
    assert!(rendered.contains("name: String"));
}

#[test]
fn test_skeleton_render_enum() {
    let skeleton = FileSkeleton {
        path: "src/state.rs".into(),
        items: vec![SkeletonItem::Enum {
            name: "State".to_string(),
            variants_summary: "Running, Stopped".to_string(),
            line: 5,
        }],
        token_count: 6,
    };
    let rendered = skeleton.render();
    assert!(rendered.contains("enum State"));
    assert!(rendered.contains("Running"));
}

#[test]
fn test_skeleton_render_trait() {
    let skeleton = FileSkeleton {
        path: "src/handler.rs".into(),
        items: vec![SkeletonItem::Trait {
            name: "Handler".to_string(),
            methods: vec!["fn handle(&self)".to_string()],
            line: 1,
        }],
        token_count: 5,
    };
    let rendered = skeleton.render();
    assert!(rendered.contains("trait Handler"));
    assert!(rendered.contains("fn handle"));
}

#[test]
fn test_skeleton_render_impl() {
    let skeleton = FileSkeleton {
        path: "src/lib.rs".into(),
        items: vec![SkeletonItem::Impl {
            target: "Config".to_string(),
            methods: vec!["fn new()".to_string(), "fn value(&self)".to_string()],
            line: 20,
        }],
        token_count: 7,
    };
    let rendered = skeleton.render();
    assert!(rendered.contains("impl Config"));
    assert!(rendered.contains("fn new()"));
}

#[test]
fn test_skeleton_render_module() {
    let skeleton = FileSkeleton {
        path: "src/lib.rs".into(),
        items: vec![SkeletonItem::Module {
            name: "tests".to_string(),
            line: 100,
        }],
        token_count: 3,
    };
    let rendered = skeleton.render();
    assert!(rendered.contains("mod tests"));
}

#[test]
fn test_skeleton_render_const() {
    let skeleton = FileSkeleton {
        path: "src/lib.rs".into(),
        items: vec![SkeletonItem::Const {
            name: "MAX_SIZE".to_string(),
            type_hint: "usize".to_string(),
            line: 3,
        }],
        token_count: 3,
    };
    let rendered = skeleton.render();
    assert!(rendered.contains("const MAX_SIZE: usize"));
}

#[test]
fn test_skeleton_render_use() {
    let skeleton = FileSkeleton {
        path: "src/lib.rs".into(),
        items: vec![SkeletonItem::Use {
            path: "use std::io".to_string(),
            line: 1,
        }],
        token_count: 2,
    };
    let rendered = skeleton.render();
    // `path` already carries the `use` keyword, so it renders verbatim —
    // NOT doubled into `use use`, which models mistook for a syntax error.
    assert!(rendered.contains("use std::io"));
    assert!(!rendered.contains("use use"));
}

#[test]
fn test_extract_and_render_use_no_double_keyword() {
    // End-to-end regression: extracting real source with `use`/`pub use`
    // and rendering the skeleton must never produce `use use ...`.
    let code = "use std::io;\npub use crate::foo::Bar;\n\nfn main() {}\n";
    let skeleton = extract_rust_skeleton(Path::new("src/lib.rs"), code);
    let rendered = skeleton.render();
    assert!(!rendered.contains("use use"), "rendered:\n{rendered}");
    assert!(rendered.contains("use std::io"));
    assert!(rendered.contains("pub use crate::foo::Bar"));
}

#[test]
fn test_skeleton_render_empty() {
    let skeleton = FileSkeleton {
        path: "src/empty.rs".into(),
        items: vec![],
        token_count: 0,
    };
    let rendered = skeleton.render();
    assert!(rendered.contains("src/empty.rs"));
}

// =========================================================================
// ContextModality detection extended tests
// =========================================================================

#[test]
fn test_modality_wire() {
    assert!(matches!(
        ContextModality::from_task("wire up the new handler"),
        ContextModality::Merge { .. }
    ));
}

#[test]
fn test_modality_thread() {
    assert!(matches!(
        ContextModality::from_task("thread the context through calls"),
        ContextModality::Merge { .. }
    ));
}

#[test]
fn test_modality_integrate() {
    assert!(matches!(
        ContextModality::from_task("integrate the auth module"),
        ContextModality::Merge { .. }
    ));
}

#[test]
fn test_modality_extract() {
    assert!(matches!(
        ContextModality::from_task("extract the parsing logic"),
        ContextModality::Refactor { .. }
    ));
}

#[test]
fn test_modality_decompose() {
    assert!(matches!(
        ContextModality::from_task("decompose the monolith"),
        ContextModality::Refactor { .. }
    ));
}

#[test]
fn test_modality_split() {
    assert!(matches!(
        ContextModality::from_task("split the module"),
        ContextModality::Refactor { .. }
    ));
}

#[test]
fn test_modality_break_up() {
    assert!(matches!(
        ContextModality::from_task("break up the large file"),
        ContextModality::Refactor { .. }
    ));
}

#[test]
fn test_modality_audit() {
    assert!(matches!(
        ContextModality::from_task("audit the security code"),
        ContextModality::Review
    ));
}

#[test]
fn test_modality_investigate() {
    assert!(matches!(
        ContextModality::from_task("investigate the memory leak"),
        ContextModality::Debug { .. }
    ));
}

#[test]
fn test_modality_trace() {
    assert!(matches!(
        ContextModality::from_task("trace the request path"),
        ContextModality::Debug { .. }
    ));
}

#[test]
fn test_modality_why_does() {
    assert!(matches!(
        ContextModality::from_task("why does the loop hang"),
        ContextModality::Debug { .. }
    ));
}

#[test]
fn test_modality_new_feature() {
    assert!(matches!(
        ContextModality::from_task("add a new feature for caching"),
        ContextModality::Greenfield { .. }
    ));
}

#[test]
fn test_modality_new_module() {
    assert!(matches!(
        ContextModality::from_task("create a new module for logging"),
        ContextModality::Greenfield { .. }
    ));
}

#[test]
fn test_modality_greenfield_keyword() {
    assert!(matches!(
        ContextModality::from_task("greenfield implementation"),
        ContextModality::Greenfield { .. }
    ));
}

// =========================================================================
// LoadingPlan tests
// =========================================================================

#[test]
fn test_loading_plan_implement() {
    let modality = ContextModality::Implement {
        target: "src/main.rs".into(),
        related: vec!["src/config.rs".into()],
    };
    let plan = modality.loading_plan();
    assert_eq!(plan.l3_files.len(), 1);
    assert_eq!(plan.l2_files.len(), 1);
    assert!(plan.description.contains("Implement"));
}

#[test]
fn test_loading_plan_debug() {
    let modality = ContextModality::Debug {
        entry_point: "src/main.rs".into(),
        call_chain: vec!["src/handler.rs".into(), "src/db.rs".into()],
    };
    let plan = modality.loading_plan();
    assert_eq!(plan.l3_files.len(), 3); // entry + 2 chain items
}

#[test]
fn test_loading_plan_refactor() {
    let modality = ContextModality::Refactor {
        source: "src/big.rs".into(),
        targets: vec!["src/part_a.rs".into()],
        orchestrator: Some("src/mod.rs".into()),
    };
    let plan = modality.loading_plan();
    assert_eq!(plan.l3_files.len(), 3); // source + target + orchestrator
}

#[test]
fn test_loading_plan_refactor_no_orchestrator() {
    let modality = ContextModality::Refactor {
        source: "src/big.rs".into(),
        targets: vec![],
        orchestrator: None,
    };
    let plan = modality.loading_plan();
    assert_eq!(plan.l3_files.len(), 1);
}

#[test]
fn test_loading_plan_greenfield() {
    let modality = ContextModality::Greenfield {
        integration_points: vec!["src/main.rs".into(), "src/lib.rs".into()],
    };
    let plan = modality.loading_plan();
    assert!(plan.l3_files.is_empty());
    assert_eq!(plan.l2_files.len(), 2);
}

// =========================================================================
// LevelCosts tests
// =========================================================================

#[test]
fn test_level_costs_default() {
    let costs = LevelCosts::default();
    assert_eq!(costs.l1, 0);
    assert_eq!(costs.l2, 0);
    assert_eq!(costs.l3, 0);
}

// =========================================================================
// LoadEstimate tests
// =========================================================================

#[test]
fn test_load_estimate_struct() {
    let est = LoadEstimate {
        fits: true,
        estimated_tokens: 1000,
        usage_pct: 0.5,
        current_total: 5000,
        budget: 10000,
    };
    assert!(est.fits);
    assert_eq!(est.estimated_tokens, 1000);
}

// =========================================================================
// Edge case budget tests
// =========================================================================

#[test]
fn test_zero_budget() {
    let map = ContextMap::new(0, 0.75, 0.20, 0.05);
    assert_eq!(map.budget(), 0);
    assert_eq!(map.remaining(), 0);
}

#[test]
fn test_large_budget() {
    let map = ContextMap::new(1_000_000, 0.75, 0.20, 0.05);
    assert_eq!(map.budget(), 750_000);
    assert_eq!(map.compression_headroom(), 200_000);
}

#[test]
fn test_custom_ratios() {
    let map = ContextMap::new(100_000, 0.50, 0.30, 0.20);
    assert_eq!(map.budget(), 50_000);
    assert_eq!(map.compression_headroom(), 30_000);
    assert_eq!(map.thinking_reserve(), 20_000);
}
