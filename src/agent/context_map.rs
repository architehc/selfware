//! Hierarchical context map for token-aware codebase ingestion.
//!
//! Manages a three-level view of the codebase (tiers from the shared
//! `crate::evolve::ContextMode` vocabulary):
//! - **L1**: Project tree (directory names, file names, sizes) — `ContextMode::Map`, always loaded
//! - **L2**: File skeletons (function/struct/trait signatures, no bodies) — `ContextMode::Lite`, on demand
//! - **L3**: Full file content — `ContextMode::Full`, on demand, auto-downgradable
//!
//! Each entry tracks its token cost so the agent can make budget-aware decisions
//! about what to load, upgrade, or evict.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use tracing::{debug, info};

use crate::evolve::ContextMode;
use crate::token_count::estimate_content_tokens;
use crate::tools::codemap::{
    update_context_map_tokens, update_files_in_context, update_total_budget,
};

// ─── Skeleton Types ─────────────────────────────────────────────────────────

// Skeleton extraction lives in `evolve::skeleton` (shared with the evolve
// composer); re-exported here so existing agent call sites keep working.
pub use crate::evolve::skeleton::{extract_rust_skeleton, FileSkeleton, SkeletonItem};

// ─── Token Cost Tracking ────────────────────────────────────────────────────

/// Token costs for a file at each context level.
#[derive(Debug, Clone, Default)]
pub struct LevelCosts {
    pub l1: usize,
    pub l2: usize,
    pub l3: usize,
}

/// Result of a pre-load budget check.
#[derive(Debug, Clone)]
pub struct LoadEstimate {
    pub fits: bool,
    pub estimated_tokens: usize,
    pub usage_pct: f32,
    pub current_total: usize,
    pub budget: usize,
}

// ─── Context Modality ───────────────────────────────────────────────────────

/// Task-driven context loading strategy.
#[derive(Debug, Clone)]
pub enum ContextModality {
    /// Connecting/integrating code across files.
    Merge {
        source_files: Vec<PathBuf>,
        target_files: Vec<PathBuf>,
    },
    /// Exploring/reviewing the codebase.
    Review,
    /// Writing new code or modifying specific files.
    Implement {
        target: PathBuf,
        related: Vec<PathBuf>,
    },
    /// Tracing execution paths for debugging.
    Debug {
        entry_point: PathBuf,
        call_chain: Vec<PathBuf>,
    },
    /// Decomposing a monolith into smaller components.
    Refactor {
        source: PathBuf,
        targets: Vec<PathBuf>,
        orchestrator: Option<PathBuf>,
    },
    /// Brand-new feature with minimal coupling.
    Greenfield { integration_points: Vec<PathBuf> },
}

impl ContextModality {
    /// Infer modality from task description keywords.
    pub fn from_task(task: &str) -> Self {
        let lower = task.to_lowercase();
        if lower.contains("merge")
            || lower.contains("connect")
            || lower.contains("wire")
            || lower.contains("thread")
            || lower.contains("integrate")
        {
            ContextModality::Merge {
                source_files: vec![],
                target_files: vec![],
            }
        } else if lower.contains("refactor")
            || lower.contains("extract")
            || lower.contains("decompose")
            || lower.contains("split")
            || lower.contains("break up")
        {
            ContextModality::Refactor {
                source: PathBuf::new(),
                targets: vec![],
                orchestrator: None,
            }
        } else if lower.contains("review")
            || lower.contains("read all")
            || lower.contains("explore")
            || lower.contains("understand")
            || lower.contains("audit")
        {
            ContextModality::Review
        } else if lower.contains("debug")
            || lower.contains("why does")
            || lower.contains("trace")
            || lower.contains("investigate")
            || lower.contains("bug")
        {
            ContextModality::Debug {
                entry_point: PathBuf::new(),
                call_chain: vec![],
            }
        } else if lower.contains("new feature")
            || lower.contains("new module")
            || lower.contains("create")
            || lower.contains("brand new")
            || lower.contains("greenfield")
        {
            ContextModality::Greenfield {
                integration_points: vec![],
            }
        } else {
            // Default: implement/modify
            ContextModality::Implement {
                target: PathBuf::new(),
                related: vec![],
            }
        }
    }

    /// Returns which files should be at L3, L2, or L1 for this modality.
    pub fn loading_plan(&self) -> LoadingPlan {
        match self {
            ContextModality::Merge {
                source_files,
                target_files,
            } => LoadingPlan {
                l3_files: source_files
                    .iter()
                    .chain(target_files.iter())
                    .cloned()
                    .collect(),
                l2_files: vec![], // callers discovered dynamically
                description: "Merge: both sides at L3, callers at L2".into(),
            },
            ContextModality::Review => LoadingPlan {
                l3_files: vec![], // one at a time, rotating
                l2_files: vec![], // all files at L2
                description: "Review: all files at L2, promote to L3 on demand".into(),
            },
            ContextModality::Implement { target, related } => LoadingPlan {
                l3_files: vec![target.clone()],
                l2_files: related.clone(),
                description: "Implement: target at L3, dependencies at L2".into(),
            },
            ContextModality::Debug {
                entry_point,
                call_chain,
            } => LoadingPlan {
                l3_files: std::iter::once(entry_point.clone())
                    .chain(call_chain.iter().cloned())
                    .collect(),
                l2_files: vec![],
                description: "Debug: call chain at L3".into(),
            },
            ContextModality::Refactor {
                source,
                targets,
                orchestrator,
            } => {
                let mut l3 = vec![source.clone()];
                l3.extend(targets.iter().cloned());
                if let Some(orch) = orchestrator {
                    l3.push(orch.clone());
                }
                LoadingPlan {
                    l3_files: l3,
                    l2_files: vec![],
                    description: "Refactor: source + targets + orchestrator at L3".into(),
                }
            }
            ContextModality::Greenfield { integration_points } => LoadingPlan {
                l3_files: vec![],
                l2_files: integration_points.clone(),
                description: "Greenfield: integration points at L2, rest minimal".into(),
            },
        }
    }
}

/// Plan for which files to load at which level.
#[derive(Debug, Clone)]
pub struct LoadingPlan {
    pub l3_files: Vec<PathBuf>,
    pub l2_files: Vec<PathBuf>,
    pub description: String,
}

// ─── File Entry ─────────────────────────────────────────────────────────────

/// A single file's representation in the context map.
#[derive(Debug, Clone)]
struct FileEntry {
    path: PathBuf,
    level: ContextMode,
    /// Token cost at the currently loaded level.
    current_tokens: usize,
    /// Known costs at each level (0 if not yet computed).
    costs: LevelCosts,
    /// When this entry was last accessed (for LRU eviction).
    last_accessed: Instant,
    /// Skeleton (populated when level is Lite or Full).
    skeleton: Option<FileSkeleton>,
    /// Full content (populated when level is Full).
    full_content: Option<String>,
    /// File size in bytes (for L1 display).
    file_size: u64,
}

// ─── Context Map ────────────────────────────────────────────────────────────

/// Hierarchical context manager that tracks what's loaded and at what cost.
pub struct ContextMap {
    entries: HashMap<PathBuf, FileEntry>,
    /// Total tokens currently used across all entries.
    total_tokens: usize,
    /// Content budget (75% of token_budget by default).
    budget: usize,
    /// Compression headroom (20% of token_budget).
    compression_headroom: usize,
    /// Thinking reserve (5% of token_budget).
    thinking_reserve: usize,
    /// Current modality (inferred from task).
    modality: Option<ContextModality>,
    /// Project root for resolving paths.
    project_root: PathBuf,
}

impl ContextMap {
    pub fn new(
        token_budget: usize,
        content_ratio: f32,
        compression_ratio: f32,
        thinking_ratio: f32,
    ) -> Self {
        let budget = (token_budget as f32 * content_ratio) as usize;
        let compression_headroom = (token_budget as f32 * compression_ratio) as usize;
        let thinking_reserve = (token_budget as f32 * thinking_ratio) as usize;

        info!(
            "ContextMap initialized: budget={} (content={}%, compress={}%, think={}%)",
            token_budget,
            (content_ratio * 100.0) as u32,
            (compression_ratio * 100.0) as u32,
            (thinking_ratio * 100.0) as u32,
        );

        update_total_budget(token_budget);
        update_context_map_tokens(0);
        update_files_in_context(0);

        Self {
            entries: HashMap::new(),
            total_tokens: 0,
            budget,
            compression_headroom,
            thinking_reserve,
            modality: None,
            project_root: super::current_project_root(),
        }
    }

    /// Publish the context-map portion of the shared `ContextBudget` so tools
    /// like `context_budget` see a single, combined view of memory + map usage.
    fn sync_budget(&self) {
        update_context_map_tokens(self.total_tokens);
        update_files_in_context(self.entries.len());
    }

    // ── Budget queries ──────────────────────────────────────────────────

    /// Content budget (the 75% zone).
    pub fn budget(&self) -> usize {
        self.budget
    }

    /// Current total tokens used.
    pub fn total_tokens(&self) -> usize {
        self.total_tokens
    }

    /// How much budget remains for new content.
    pub fn remaining(&self) -> usize {
        self.budget.saturating_sub(self.total_tokens)
    }

    /// Usage as a fraction of budget (0.0–1.0).
    pub fn usage_fraction(&self) -> f32 {
        if self.budget == 0 {
            return 1.0;
        }
        self.total_tokens as f32 / self.budget as f32
    }

    /// Whether compression should trigger (at content budget boundary).
    pub fn should_compress(&self) -> bool {
        self.total_tokens > self.budget
    }

    /// The compression headroom zone in tokens.
    pub fn compression_headroom(&self) -> usize {
        self.compression_headroom
    }

    /// The thinking reserve zone in tokens.
    pub fn thinking_reserve(&self) -> usize {
        self.thinking_reserve
    }

    // ── Modality ────────────────────────────────────────────────────────

    /// Set the context modality from a task description.
    pub fn set_modality_from_task(&mut self, task: &str) {
        let modality = ContextModality::from_task(task);
        info!("Context modality: {:?}", modality);
        self.modality = Some(modality);
    }

    /// Get the current modality.
    pub fn modality(&self) -> Option<&ContextModality> {
        self.modality.as_ref()
    }

    /// Get loading plan for current modality.
    pub fn loading_plan(&self) -> Option<LoadingPlan> {
        self.modality.as_ref().map(|m| m.loading_plan())
    }

    // ── Pre-load estimation ─────────────────────────────────────────────

    /// Check if loading a file at the given level fits in budget.
    pub async fn can_load(&self, path: &Path, level: ContextMode) -> LoadEstimate {
        let estimated = self.estimate_level_tokens(path, level).await;
        // Subtract current cost if already loaded (upgrade scenario).
        let current_cost = self
            .entries
            .get(path)
            .map(|e| e.current_tokens)
            .unwrap_or(0);
        let net_cost = estimated.saturating_sub(current_cost);
        let new_total = self.total_tokens + net_cost;
        let fits = new_total <= self.budget;
        let usage_pct = new_total as f32 / self.budget.max(1) as f32;

        LoadEstimate {
            fits,
            estimated_tokens: estimated,
            usage_pct,
            current_total: self.total_tokens,
            budget: self.budget,
        }
    }

    /// Estimate token cost for a file at a given level without loading it.
    async fn estimate_level_tokens(&self, path: &Path, level: ContextMode) -> usize {
        // If we already have cached costs, use them.
        if let Some(entry) = self.entries.get(path) {
            match level {
                ContextMode::Map => return entry.costs.l1,
                ContextMode::Lite if entry.costs.l2 > 0 => return entry.costs.l2,
                ContextMode::Full if entry.costs.l3 > 0 => return entry.costs.l3,
                _ => {}
            }
        }

        // Estimate from file size.
        let full_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.project_root.join(path)
        };

        let file_size = tokio::fs::metadata(&full_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        match level {
            ContextMode::Map => 10, // single line entry
            ContextMode::Lite => {
                // Signatures are ~5-15% of full file for code
                (file_size as usize / 3 / 10).max(20) // chars/3 for tokens, /10 for skeleton ratio
            }
            ContextMode::Full => {
                // Code: ~chars/3 tokens
                (file_size as usize / 3).max(1)
            }
            // Compact/FullExtended/Preset never occur as agent runtime levels;
            // estimate them like full content.
            _ => (file_size as usize / 3).max(1),
        }
    }

    // ── Load / upgrade / downgrade ──────────────────────────────────────

    /// Register a file at L1 (tree level). Very cheap.
    pub fn register_tree_entry(&mut self, path: PathBuf, file_size: u64) {
        let l1_cost = 10; // ~10 tokens for a tree line
        if let Some(existing) = self.entries.get_mut(&path) {
            existing.costs.l1 = l1_cost;
            existing.file_size = file_size;
            return; // already registered at some level
        }

        self.entries.insert(
            path.clone(),
            FileEntry {
                path,
                level: ContextMode::Map,
                current_tokens: l1_cost,
                costs: LevelCosts {
                    l1: l1_cost,
                    ..Default::default()
                },
                last_accessed: Instant::now(),
                skeleton: None,
                full_content: None,
                file_size,
            },
        );
        self.total_tokens += l1_cost;
        self.sync_budget();
    }

    /// Load or upgrade a file to L2 (skeleton).
    pub fn load_skeleton(&mut self, path: &Path, skeleton: FileSkeleton) {
        let token_cost = skeleton.token_count;
        let path_buf = path.to_path_buf();

        // Get or create the entry first.
        let entry = self.entries.entry(path_buf).or_insert_with(|| FileEntry {
            path: path.to_path_buf(),
            level: ContextMode::Map,
            current_tokens: 10,
            costs: LevelCosts {
                l1: 10,
                ..Default::default()
            },
            last_accessed: Instant::now(),
            skeleton: None,
            full_content: None,
            file_size: 0,
        });

        // Update token accounting.
        self.total_tokens = self.total_tokens.saturating_sub(entry.current_tokens);
        entry.level = ContextMode::Lite;
        entry.current_tokens = token_cost;
        entry.costs.l2 = token_cost;
        entry.last_accessed = Instant::now();
        // Drop full content if downgrading from L3.
        entry.full_content = None;
        self.total_tokens += token_cost;

        // Store skeleton.
        entry.skeleton = Some(skeleton);

        debug!(
            "Loaded skeleton for {}: {} tokens (total: {}/{})",
            path.display(),
            token_cost,
            self.total_tokens,
            self.budget
        );
        self.sync_budget();
    }

    /// Load or upgrade a file to L3 (full content).
    pub fn load_full(&mut self, path: &Path, content: String) {
        let token_cost = estimate_content_tokens(&content);
        let entry = self
            .entries
            .entry(path.to_path_buf())
            .or_insert_with(|| FileEntry {
                path: path.to_path_buf(),
                level: ContextMode::Map,
                current_tokens: 10,
                costs: LevelCosts {
                    l1: 10,
                    ..Default::default()
                },
                last_accessed: Instant::now(),
                skeleton: None,
                full_content: None,
                file_size: 0,
            });

        self.total_tokens = self.total_tokens.saturating_sub(entry.current_tokens);
        entry.level = ContextMode::Full;
        entry.current_tokens = token_cost;
        entry.costs.l3 = token_cost;
        entry.full_content = Some(content);
        entry.last_accessed = Instant::now();
        self.total_tokens += token_cost;

        debug!(
            "Loaded full content for {}: {} tokens (total: {}/{})",
            path.display(),
            token_cost,
            self.total_tokens,
            self.budget
        );
        self.sync_budget();
    }

    /// Downgrade a file from L3 to L2 (skeleton), freeing tokens.
    /// Returns how many tokens were freed, or 0 if no skeleton available.
    pub fn downgrade_to_skeleton(&mut self, path: &Path) -> usize {
        let entry = match self.entries.get_mut(path) {
            Some(e) if e.level == ContextMode::Full => e,
            _ => return 0,
        };

        let old_cost = entry.current_tokens;
        let skeleton_cost = entry.costs.l2;
        if skeleton_cost == 0 || entry.skeleton.is_none() {
            // No skeleton available — can't downgrade, only evict entirely.
            return 0;
        }

        entry.level = ContextMode::Lite;
        entry.current_tokens = skeleton_cost;
        entry.full_content = None;
        let freed = old_cost.saturating_sub(skeleton_cost);
        self.total_tokens = self.total_tokens.saturating_sub(freed);

        debug!(
            "Downgraded {} from L3→L2: freed {} tokens (total: {}/{})",
            path.display(),
            freed,
            self.total_tokens,
            self.budget
        );
        self.sync_budget();

        freed
    }

    /// Evict a file entirely (back to L1/tree).
    pub fn evict_to_tree(&mut self, path: &Path) -> usize {
        let entry = match self.entries.get_mut(path) {
            Some(e) if e.level != ContextMode::Map => e,
            _ => return 0,
        };

        let old_cost = entry.current_tokens;
        let tree_cost = entry.costs.l1.max(10);
        entry.level = ContextMode::Map;
        entry.current_tokens = tree_cost;
        entry.skeleton = None;
        entry.full_content = None;
        let freed = old_cost.saturating_sub(tree_cost);
        self.total_tokens = self.total_tokens.saturating_sub(freed);
        self.sync_budget();
        freed
    }

    /// Free enough tokens to fit `needed` by downgrading L3→L2 then L2→L1,
    /// starting with the least recently accessed files.
    ///
    /// Contract: `needed` is the TOTAL free room required afterwards — i.e.
    /// compression stops once `self.remaining() >= needed` (or nothing more
    /// can be freed). Callers must pass the full estimated cost of what they
    /// want to load, NOT `estimate - remaining` (that double-subtracts the
    /// existing headroom and stops ~`remaining()` tokens short).
    /// Returns total tokens freed.
    pub fn compress_to_fit(&mut self, needed: usize) -> usize {
        if self.remaining() >= needed {
            return 0;
        }

        let deficit = needed.saturating_sub(self.remaining());
        let mut freed = 0usize;

        // Collect paths sorted by last_accessed (oldest first).
        let mut candidates: Vec<(PathBuf, Instant, ContextMode)> = self
            .entries
            .values()
            .filter(|e| e.level != ContextMode::Map)
            .map(|e| (e.path.clone(), e.last_accessed, e.level.clone()))
            .collect();
        candidates.sort_by_key(|(_, t, _)| *t);

        // Pass 1: downgrade L3 → L2.
        for (path, _, level) in &candidates {
            if freed >= deficit {
                break;
            }
            if *level == ContextMode::Full {
                freed += self.downgrade_to_skeleton(path);
            }
        }

        // Pass 1b: Full entries without a skeleton can't be downgraded
        // (load_full never populates skeletons, so directly-read files hit
        // this). Evict them straight to L1 (tree) instead of leaving them
        // incompressible.
        if freed < deficit {
            for (path, _, level) in &candidates {
                if freed >= deficit {
                    break;
                }
                // Snapshot said Full and it is STILL Full (pass 1 skipped it
                // for lack of a skeleton).
                if *level == ContextMode::Full && self.level_of(path) == Some(ContextMode::Full) {
                    freed += self.evict_to_tree(path);
                }
            }
        }

        // Pass 2: evict L2 → L1.
        if freed < deficit {
            for (path, _, level) in &candidates {
                if freed >= deficit {
                    break;
                }
                if *level == ContextMode::Lite {
                    freed += self.evict_to_tree(path);
                }
            }
        }

        if freed > 0 {
            info!(
                "Compressed context: freed {} tokens (needed {}, total now: {}/{})",
                freed, needed, self.total_tokens, self.budget
            );
        }

        self.sync_budget();
        freed
    }

    // ── L1 Tree rendering ───────────────────────────────────────────────

    /// Render the L1 project tree as a compact string.
    pub fn render_tree(&self) -> String {
        /// Max files to render in the tree. Keeps system prompt bounded.
        const MAX_TREE_LINES: usize = 500;

        let mut lines: Vec<String> = Vec::new();
        let mut sorted: Vec<_> = self.entries.values().collect();
        sorted.sort_by(|a, b| a.path.cmp(&b.path));

        let total_files = sorted.len();
        lines.push(format!(
            "# Project tree ({} files, {}/{} tokens used)\n",
            total_files, self.total_tokens, self.budget,
        ));

        // Show loaded/skeleton files first (more relevant), then tree-only up to cap
        sorted.sort_by_key(|e| match e.level {
            ContextMode::Full => 0,
            ContextMode::Lite => 1,
            // Map and any non-agent tier sort last (tree-only).
            _ => 2,
        });

        for (shown, entry) in sorted.iter().enumerate() {
            if shown >= MAX_TREE_LINES {
                lines.push(format!(
                    "\n  ... and {} more files (use directory_tree for full listing)",
                    total_files - shown
                ));
                break;
            }
            let level_marker = match entry.level {
                ContextMode::Lite => "◆",
                ContextMode::Full => "█",
                // Map (tree-only) and any non-agent tier.
                _ => "·",
            };
            let size_str = if entry.file_size > 1024 {
                format!("{}K", entry.file_size / 1024)
            } else {
                format!("{}B", entry.file_size)
            };
            lines.push(format!(
                "  {} {} ({}, ~{}tok)",
                level_marker,
                entry.path.display(),
                size_str,
                entry.current_tokens
            ));
        }

        lines.push(String::new());
        lines.push(
            "Legend: · = tree only, ◆ = skeleton loaded, █ = full content loaded".to_string(),
        );

        lines.join("\n")
    }

    // ── Context boundary marker ─────────────────────────────────────────

    /// Build the RoPE-aware context boundary marker.
    pub fn render_boundary(&self) -> String {
        let l1_count = self
            .entries
            .values()
            .filter(|e| e.level == ContextMode::Map)
            .count();
        let l2_count = self
            .entries
            .values()
            .filter(|e| e.level == ContextMode::Lite)
            .count();
        let l3_count = self
            .entries
            .values()
            .filter(|e| e.level == ContextMode::Full)
            .count();
        let l3_files: Vec<String> = self
            .entries
            .values()
            .filter(|e| e.level == ContextMode::Full)
            .map(|e| e.path.display().to_string())
            .collect();

        let focus = if l3_files.is_empty() {
            "none".to_string()
        } else {
            l3_files.join(", ")
        };

        format!(
            "<context_boundary>\n\
             Context: {} files at L1 (tree), {} at L2 (skeleton), {} at L3 (full)\n\
             Token usage: {}/{} ({:.0}%)\n\
             Active focus: {}\n\
             Everything above is reference material. Continue the task below.\n\
             </context_boundary>",
            l1_count,
            l2_count,
            l3_count,
            self.total_tokens,
            self.budget,
            self.usage_fraction() * 100.0,
            focus,
        )
    }

    // ── Queries ─────────────────────────────────────────────────────────

    /// Get the current level of a file.
    pub fn level_of(&self, path: &Path) -> Option<ContextMode> {
        self.entries.get(path).map(|e| e.level.clone())
    }

    /// Get all files at a given level.
    pub fn files_at_level(&self, level: ContextMode) -> Vec<&Path> {
        self.entries
            .values()
            .filter(|e| e.level == level)
            .map(|e| e.path.as_path())
            .collect()
    }

    /// Get the full content of a file (if loaded at L3).
    pub fn full_content(&self, path: &Path) -> Option<&str> {
        self.entries
            .get(path)
            .and_then(|e| e.full_content.as_deref())
    }

    /// Get the skeleton of a file (if loaded at L2+).
    pub fn skeleton(&self, path: &Path) -> Option<&FileSkeleton> {
        self.entries.get(path).and_then(|e| e.skeleton.as_ref())
    }

    /// Mark a file as accessed (updates LRU timestamp).
    pub fn touch(&mut self, path: &Path) {
        if let Some(entry) = self.entries.get_mut(path) {
            entry.last_accessed = Instant::now();
        }
    }

    /// Number of tracked files.
    pub fn file_count(&self) -> usize {
        self.entries.len()
    }

    // ── Intelligent Context Management ────────────────────────────────────

    /// Optimize the context by auto-downgrading stale files.
    /// Files not accessed within `staleness_secs` get downgraded one level.
    /// Returns total tokens freed.
    pub fn auto_optimize(&mut self, staleness_secs: u64) -> usize {
        let cutoff = Instant::now() - std::time::Duration::from_secs(staleness_secs);
        let mut stale_l3: Vec<PathBuf> = Vec::new();
        let mut stale_l2: Vec<PathBuf> = Vec::new();

        for entry in self.entries.values() {
            if entry.last_accessed < cutoff {
                match entry.level {
                    ContextMode::Full => stale_l3.push(entry.path.clone()),
                    ContextMode::Lite => stale_l2.push(entry.path.clone()),
                    _ => {}
                }
            }
        }

        let mut freed = 0usize;
        for path in &stale_l3 {
            freed += self.downgrade_to_skeleton(path);
        }
        for path in &stale_l2 {
            freed += self.evict_to_tree(path);
        }

        if freed > 0 {
            info!(
                "Auto-optimized: freed {} tokens ({} L3→L2, {} L2→L1)",
                freed,
                stale_l3.len(),
                stale_l2.len()
            );
        }
        self.sync_budget();
        freed
    }

    /// Add a web search result or external reference to the context.
    /// These are tracked as virtual files at L3 with the given content.
    pub fn add_external_context(&mut self, key: &str, content: String, source: &str) {
        let virtual_path = PathBuf::from(format!("<external>/{}", key));
        let token_cost = estimate_content_tokens(&content);

        // Auto-compress if needed.
        if self.remaining() < token_cost {
            self.compress_to_fit(token_cost);
        }

        let entry = self
            .entries
            .entry(virtual_path.clone())
            .or_insert_with(|| FileEntry {
                path: virtual_path,
                level: ContextMode::Map,
                current_tokens: 10,
                costs: LevelCosts {
                    l1: 10,
                    ..Default::default()
                },
                last_accessed: Instant::now(),
                skeleton: None,
                full_content: None,
                file_size: 0,
            });

        self.total_tokens = self.total_tokens.saturating_sub(entry.current_tokens);
        entry.level = ContextMode::Full;
        entry.current_tokens = token_cost;
        entry.costs.l3 = token_cost;
        entry.full_content = Some(format!("// Source: {}\n{}", source, content));
        entry.last_accessed = Instant::now();
        self.total_tokens += token_cost;

        debug!(
            "Added external context '{}' from {}: {} tokens",
            key, source, token_cost
        );
        self.sync_budget();
    }

    /// Get the most relevant files for a query using simple keyword matching.
    /// Returns paths sorted by relevance score (highest first).
    /// For deeper semantic search, use BM25/vector search externally
    /// and pass results through `add_external_context` or `load_full`.
    pub fn find_relevant_files(&self, query: &str) -> Vec<(PathBuf, f32)> {
        let query_lower = query.to_lowercase();
        let query_terms: Vec<&str> = query_lower.split_whitespace().collect();

        let mut scored: Vec<(PathBuf, f32)> = self
            .entries
            .values()
            .filter_map(|entry| {
                let path_str = entry.path.to_string_lossy().to_lowercase();
                let mut score = 0.0f32;

                // Score by path match.
                for term in &query_terms {
                    if path_str.contains(term) {
                        score += 1.0;
                    }
                }

                // Score by skeleton content match (if available).
                if let Some(ref skeleton) = entry.skeleton {
                    let rendered = skeleton.render().to_lowercase();
                    for term in &query_terms {
                        if rendered.contains(term) {
                            score += 0.5;
                        }
                    }
                }

                // Score by full content match (if available).
                if let Some(ref content) = entry.full_content {
                    let content_lower = content.to_lowercase();
                    for term in &query_terms {
                        if content_lower.contains(term) {
                            score += 0.3;
                        }
                    }
                }

                if score > 0.0 {
                    Some((entry.path.clone(), score))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored
    }

    /// Promote the most relevant files to L3 based on a query,
    /// automatically managing budget by downgrading less relevant files.
    /// Returns which files were promoted.
    pub async fn focus_on_query(&mut self, query: &str, max_promote: usize) -> Vec<PathBuf> {
        let relevant = self.find_relevant_files(query);
        let mut promoted = Vec::new();

        for (path, _score) in relevant.into_iter().take(max_promote) {
            let current_level = self.level_of(&path);
            if current_level == Some(ContextMode::Full) {
                self.touch(&path);
                promoted.push(path);
                continue;
            }

            // Estimate cost and ensure headroom.
            let estimate = self.can_load(&path, ContextMode::Full).await;
            if !estimate.fits {
                // compress_to_fit takes the TOTAL free room required, not
                // the additional deficit over remaining().
                self.compress_to_fit(estimate.estimated_tokens);
            }

            // We can't load the actual content here (no filesystem access in ContextMap),
            // but we mark it as needing promotion. The caller should load the content.
            promoted.push(path);
        }

        promoted
    }

    /// Meta-analysis: suggest what context should be loaded and what's irrelevant
    /// for the current task. Returns a `ContextRecommendation` with:
    /// - files that should be promoted (loaded at higher detail)
    /// - files that are irrelevant and should be evicted
    /// - estimated token savings
    pub async fn recommend_context(&self, task: &str) -> ContextRecommendation {
        let modality = ContextModality::from_task(task);
        let plan = modality.loading_plan();

        let mut promote = Vec::new();
        let mut evict = Vec::new();
        let mut keep = Vec::new();

        // Task keyword extraction for relevance scoring.
        let task_lower = task.to_lowercase();
        let task_terms: Vec<&str> = task_lower
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
            .filter(|s| s.len() > 2)
            .collect();

        for entry in self.entries.values() {
            let path_str = entry.path.to_string_lossy().to_lowercase();

            // Score relevance based on path match + skeleton match.
            let mut relevance = 0.0f32;
            for term in &task_terms {
                if path_str.contains(term) {
                    relevance += 2.0;
                }
            }
            if let Some(ref skeleton) = entry.skeleton {
                let rendered = skeleton.render().to_lowercase();
                for term in &task_terms {
                    if rendered.contains(term) {
                        relevance += 1.0;
                    }
                }
            }

            // Check if the file is in the loading plan.
            let in_l3_plan = plan.l3_files.iter().any(|f| {
                let f_str = f.to_string_lossy().to_lowercase();
                path_str.contains(&f_str) || f_str.contains(&path_str)
            });
            let in_l2_plan = plan.l2_files.iter().any(|f| {
                let f_str = f.to_string_lossy().to_lowercase();
                path_str.contains(&f_str) || f_str.contains(&path_str)
            });

            if in_l3_plan || relevance >= 3.0 {
                if entry.level != ContextMode::Full {
                    promote.push(ContextSuggestion {
                        path: entry.path.clone(),
                        current_level: entry.level.clone(),
                        suggested_level: ContextMode::Full,
                        relevance,
                        reason: if in_l3_plan {
                            "In loading plan for task modality".into()
                        } else {
                            format!("High relevance score ({:.1})", relevance)
                        },
                        estimated_tokens: entry.costs.l3.max(
                            self.estimate_level_tokens(&entry.path, ContextMode::Full)
                                .await,
                        ),
                    });
                } else {
                    keep.push(entry.path.clone());
                }
            } else if in_l2_plan || relevance >= 1.0 {
                if entry.level == ContextMode::Full {
                    // Could downgrade to save tokens
                    evict.push(ContextSuggestion {
                        path: entry.path.clone(),
                        current_level: entry.level.clone(),
                        suggested_level: ContextMode::Lite,
                        relevance,
                        reason: "Moderate relevance — skeleton sufficient".into(),
                        estimated_tokens: entry.costs.l2,
                    });
                } else {
                    keep.push(entry.path.clone());
                }
            } else if entry.level != ContextMode::Map && relevance < 0.5 {
                evict.push(ContextSuggestion {
                    path: entry.path.clone(),
                    current_level: entry.level.clone(),
                    suggested_level: ContextMode::Map,
                    relevance,
                    reason: "Low relevance for current task".into(),
                    estimated_tokens: 10, // tree cost
                });
            }
        }

        // Sort promotions by relevance (highest first), evictions by savings.
        promote.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        evict.sort_by(|a, b| {
            let savings_a = a.estimated_tokens.saturating_sub(10);
            let savings_b = b.estimated_tokens.saturating_sub(10);
            savings_b.cmp(&savings_a)
        });

        let potential_savings: usize = evict
            .iter()
            .map(|s| {
                self.entries
                    .get(&s.path)
                    .map(|e| e.current_tokens.saturating_sub(s.estimated_tokens))
                    .unwrap_or(0)
            })
            .sum();

        ContextRecommendation {
            promote,
            evict,
            keep,
            potential_token_savings: potential_savings,
            modality_description: plan.description,
        }
    }

    /// Summary stats for logging/display.
    pub fn stats(&self) -> ContextMapStats {
        let mut stats = ContextMapStats::default();
        for entry in self.entries.values() {
            match entry.level {
                ContextMode::Map => {
                    stats.l1_count += 1;
                    stats.l1_tokens += entry.current_tokens;
                }
                ContextMode::Lite => {
                    stats.l2_count += 1;
                    stats.l2_tokens += entry.current_tokens;
                }
                ContextMode::Full => {
                    stats.l3_count += 1;
                    stats.l3_tokens += entry.current_tokens;
                }
                // Compact/FullExtended/Preset never occur as agent runtime levels.
                _ => {}
            }
        }
        stats.total_tokens = self.total_tokens;
        stats.budget = self.budget;
        stats
    }
}

#[derive(Debug, Default)]
pub struct ContextMapStats {
    pub l1_count: usize,
    pub l1_tokens: usize,
    pub l2_count: usize,
    pub l2_tokens: usize,
    pub l3_count: usize,
    pub l3_tokens: usize,
    pub total_tokens: usize,
    pub budget: usize,
}

// ─── Context Recommendation Types ───────────────────────────────────────────

/// A suggestion to change a file's context level.
#[derive(Debug, Clone)]
pub struct ContextSuggestion {
    pub path: PathBuf,
    pub current_level: ContextMode,
    pub suggested_level: ContextMode,
    pub relevance: f32,
    pub reason: String,
    pub estimated_tokens: usize,
}

/// Result of `recommend_context()` — a complete plan for context optimization.
#[derive(Debug, Clone)]
pub struct ContextRecommendation {
    /// Files to promote to higher detail levels.
    pub promote: Vec<ContextSuggestion>,
    /// Files to evict or downgrade (irrelevant for current task).
    pub evict: Vec<ContextSuggestion>,
    /// Files already at the right level.
    pub keep: Vec<PathBuf>,
    /// Estimated tokens that could be freed by applying evictions.
    pub potential_token_savings: usize,
    /// Description of the detected modality.
    pub modality_description: String,
}

impl ContextRecommendation {
    /// Render as a human-readable summary.
    pub fn render(&self) -> String {
        let mut out = format!(
            "Context recommendation ({})\n\
             Potential savings: {} tokens\n",
            self.modality_description, self.potential_token_savings
        );

        if !self.promote.is_empty() {
            out.push_str("\nPromote to full detail:\n");
            for s in &self.promote {
                out.push_str(&format!(
                    "  ↑ {} ({:?}→{:?}, ~{} tok) — {}\n",
                    s.path.display(),
                    s.current_level,
                    s.suggested_level,
                    s.estimated_tokens,
                    s.reason
                ));
            }
        }

        if !self.evict.is_empty() {
            out.push_str("\nEvict or downgrade:\n");
            for s in &self.evict {
                out.push_str(&format!(
                    "  ↓ {} ({:?}→{:?}) — {}\n",
                    s.path.display(),
                    s.current_level,
                    s.suggested_level,
                    s.reason
                ));
            }
        }

        out
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "../../tests/unit/agent/context_map/context_map_test.rs"]
mod tests;
