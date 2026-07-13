//! Context-selection benchmark for coding tasks, run over selfware's own source.
//!
//! This is a standalone experiment (it does not touch the core agent). It answers
//! a concrete question: given a coding task and a fixed context-window budget,
//! which files/symbols should we put in the prompt, and how well do different
//! context-building strategies do at different context lengths?
//!
//! It reuses selfware's own primitives:
//!   * `analysis::bm25::BM25Index` — lexical relevance ranking (across files and, for excerpts, across blocks within a file)
//!   * `token_count::estimate_content_tokens` — the same token accounting the agent uses
//!   * `agent::context_map::extract_rust_skeleton` — L2 (signature-only) file view and item line boundaries used to carve excerpts
//!
//! The corpus is selfware/src/**/*.rs. The "gold" context for each coding problem
//! is the set of files that the corresponding fix actually touches (mined from the
//! IMPROVE_03_CONTEXT_MEMORY.md audit), which makes the benchmark self-labeling.
//!
//! Build:  cargo build --release --features context-select --bin context_select_bench
//! Run:    ./target/release/context_select_bench                       # retrieval metrics
//!         ./target/release/context_select_bench --budgets 8000,32000,128000,1048576
//!         ./target/release/context_select_bench --breadth-frac 0.4    # tune reserve quota
//!         ./target/release/context_select_bench --e2e 3 --e2e-budget 32000   # + GLM-5.2 check

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use selfware::agent::context_map::{extract_rust_skeleton, SkeletonItem};
use selfware::analysis::bm25::BM25Index;
use selfware::token_count::estimate_content_tokens;

/// Default fraction of the budget reserved for skeleton breadth in the `reserve`
/// strategy (the rest is spent on full files for depth). Overridable via --breadth-frac.
const DEFAULT_BREADTH_FRAC: f64 = 0.35;

// ---------------------------------------------------------------------------
// Corpus
// ---------------------------------------------------------------------------

/// One source file, pre-rendered at the fidelity levels it can be injected at.
struct Doc {
    /// Repo-relative path, e.g. "src/memory/mod.rs".
    path: String,
    /// Full source text (L3).
    content: String,
    /// Token cost of the full source, per selfware's own estimator.
    full_tokens: usize,
    /// Rendered signature-only skeleton (L2).
    skeleton: String,
    /// Token cost of the skeleton.
    skel_tokens: usize,
    /// 0-based line indices where top-level items start (block boundaries for excerpts).
    /// Always begins with 0 (the file head: imports / module docs).
    boundaries: Vec<usize>,
}

/// Every `SkeletonItem` variant carries a 1-based source line; pull it out uniformly.
fn item_line(it: &SkeletonItem) -> usize {
    use SkeletonItem::*;
    match it {
        Function { line, .. }
        | Struct { line, .. }
        | Enum { line, .. }
        | Trait { line, .. }
        | Impl { line, .. }
        | Module { line, .. }
        | Const { line, .. }
        | Use { line, .. } => *line,
    }
}

/// Walk `root`/src for `.rs` files and build the corpus.
fn build_corpus(root: &Path) -> Result<Vec<Doc>> {
    let src = root.join("src");
    let mut docs = Vec::new();
    let mut stack = vec![src];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let content = match std::fs::read_to_string(&p) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            let skel = extract_rust_skeleton(&p, &content);
            let skeleton = skel.render();
            let full_tokens = estimate_content_tokens(&content);
            let skel_tokens = estimate_content_tokens(&skeleton);

            // Block boundaries for excerpt extraction: item start lines (1-based) → 0-based,
            // deduped and sorted, with the file head (0) prepended.
            let line_count = content.lines().count();
            let mut boundaries: Vec<usize> = skel
                .items
                .iter()
                .map(|it| item_line(it).saturating_sub(1).min(line_count))
                .collect();
            boundaries.push(0);
            boundaries.sort_unstable();
            boundaries.dedup();

            docs.push(Doc {
                path: rel,
                content,
                full_tokens,
                skeleton,
                skel_tokens,
                boundaries,
            });
        }
    }
    docs.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(docs)
}

// ---------------------------------------------------------------------------
// "Most interesting parts" extraction: query-relevant blocks within one file
// ---------------------------------------------------------------------------

/// Tokenize a query into lowercased content words (length >= 3), deduped.
/// Used for within-file block scoring, where corpus-level BM25 degenerates
/// (common terms appear in nearly every block, so IDF collapses to zero).
fn query_terms(query: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for raw in query.split(|c: char| !c.is_alphanumeric() && c != '_') {
        let w = raw.to_lowercase();
        if w.len() >= 3 && seen.insert(w.clone()) {
            terms.push(w);
        }
    }
    terms
}

/// Count query-term occurrences in `text` (case-insensitive substring hits).
/// A simple, non-degenerate relevance signal at the block level.
fn block_score(text: &str, terms: &[String]) -> usize {
    let lower = text.to_lowercase();
    terms
        .iter()
        .map(|t| lower.matches(t.as_str()).count())
        .sum()
}

/// Carve `doc` into item-level blocks, rank them against `query` by term-frequency,
/// and return the highest-scoring blocks that fit in `cap_tokens`, rendered in
/// source order with elision markers between gaps. Returns `None` if no block is
/// relevant or none fits. This is the automatic extraction of the most interesting
/// parts of a file — real code, but far cheaper than the whole file.
fn extract_relevant_spans(doc: &Doc, query: &str, cap_tokens: usize) -> Option<(String, usize)> {
    let lines: Vec<&str> = doc.content.lines().collect();
    if lines.is_empty() || cap_tokens == 0 {
        return None;
    }

    // Build (start,end) blocks from boundaries.
    let mut blocks: Vec<(usize, usize, String)> = Vec::new();
    let bounds = &doc.boundaries;
    for i in 0..bounds.len() {
        let start = bounds[i];
        let end = bounds.get(i + 1).copied().unwrap_or(lines.len());
        if start >= end {
            continue;
        }
        let text = lines[start..end].join("\n");
        blocks.push((start, end, text));
    }
    if blocks.is_empty() {
        return None;
    }

    // Rank blocks by query term-frequency; keep only blocks that actually match.
    let terms = query_terms(query);
    let mut scored: Vec<(usize, usize)> = blocks
        .iter()
        .enumerate()
        .map(|(i, (_, _, text))| (i, block_score(text, &terms)))
        .filter(|(_, s)| *s > 0)
        .collect();
    if scored.is_empty() {
        return None; // no lexical overlap — nothing "interesting" here for this query
    }
    // Highest score first; ties broken by earlier position for determinism.
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    // Greedily accept top-scoring blocks until we would exceed the per-file cap.
    let mut chosen: Vec<usize> = Vec::new();
    let mut used = 0usize;
    for (idx, _) in &scored {
        let bt = estimate_content_tokens(&blocks[*idx].2);
        if used + bt <= cap_tokens {
            used += bt;
            chosen.push(*idx);
        }
    }
    if chosen.is_empty() {
        return None; // even the single best block did not fit
    }
    chosen.sort_unstable();

    // Render in source order with elision markers between non-adjacent blocks.
    let mut out = format!("// {} (relevant excerpt)\n", doc.path);
    let mut prev_end: Option<usize> = None;
    for &idx in &chosen {
        let (start, end, text) = &blocks[idx];
        if let Some(pe) = prev_end {
            if *start > pe {
                out.push_str(&format!("// ... ({} lines elided) ...\n", start - pe));
            }
        } else if *start > 0 {
            out.push_str(&format!("// ... ({} lines elided) ...\n", start));
        }
        out.push_str(text);
        out.push('\n');
        prev_end = Some(*end);
    }
    if let Some(pe) = prev_end {
        if pe < lines.len() {
            out.push_str(&format!("// ... ({} lines elided) ...\n", lines.len() - pe));
        }
    }
    let tokens = estimate_content_tokens(&out);
    Some((out, tokens))
}

// ---------------------------------------------------------------------------
// Context-building strategies (the "context building function")
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Level {
    Full,
    Excerpt,
    Skeleton,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Strategy {
    /// Rank by BM25, pack whole files at full fidelity until the budget is spent.
    FullFileBm25,
    /// Hybrid: pack top-ranked files full; when the next file does not fit full,
    /// fall back to its skeleton. Maximizes coverage per token, but big full files
    /// can still exhaust the budget before breadth kicks in.
    SkeletonThenFull,
    /// Breadth only: pack skeletons of as many ranked files as fit.
    SkeletonOnly,
    /// Budget-reserve: cap depth (full files) at (1 - breadth_frac) of the budget,
    /// then guarantee the remaining breadth_frac for skeletons of further files.
    /// Fixes the starvation the hybrid strategy suffers at tight budgets.
    BudgetReserve,
    /// Excerpt: extract the query-relevant blocks of each ranked file (real code,
    /// compact) and pack those across as many files as fit.
    Excerpt,
}

impl Strategy {
    fn name(self) -> &'static str {
        match self {
            Strategy::FullFileBm25 => "full-bm25",
            Strategy::SkeletonThenFull => "hybrid",
            Strategy::SkeletonOnly => "skeleton",
            Strategy::BudgetReserve => "reserve",
            Strategy::Excerpt => "excerpt",
        }
    }
    fn all() -> [Strategy; 5] {
        [
            Strategy::FullFileBm25,
            Strategy::SkeletonThenFull,
            Strategy::SkeletonOnly,
            Strategy::BudgetReserve,
            Strategy::Excerpt,
        ]
    }
}

/// A selected file plus the fidelity level and token cost it was included at.
struct Picked {
    path: String,
    level: Level,
    tokens: usize,
    /// Rendered content for Excerpt picks (Full/Skeleton render from the corpus).
    excerpt: Option<String>,
}

struct Selection {
    picks: Vec<Picked>,
    tokens_used: usize,
}

impl Selection {
    fn pick(&self, path: &str) -> Option<&Picked> {
        self.picks.iter().find(|p| p.path == path)
    }

    /// Concatenate the selection into a single prompt-ready context string.
    fn render(&self, corpus: &[Doc]) -> String {
        let mut out = String::new();
        for pick in &self.picks {
            let doc = corpus.iter().find(|d| d.path == pick.path).unwrap();
            match pick.level {
                Level::Full => {
                    out.push_str(&format!("// ===== FULL: {} =====\n", doc.path));
                    out.push_str(&doc.content);
                    out.push('\n');
                }
                Level::Excerpt => {
                    out.push_str(&format!("// ===== EXCERPT: {} =====\n", doc.path));
                    out.push_str(pick.excerpt.as_deref().unwrap_or(""));
                    out.push('\n');
                }
                Level::Skeleton => {
                    out.push_str(&format!("// ===== SKELETON: {} =====\n", doc.path));
                    out.push_str(&doc.skeleton);
                    out.push('\n');
                }
            }
        }
        out
    }
}

/// Rank the whole corpus for `query`: BM25 hits first (scored docs), then the
/// remaining docs in stable path order as a deterministic tail.
fn ranked_order<'a>(query: &str, corpus: &'a [Doc], index: &BM25Index) -> Vec<&'a Doc> {
    let ranked = index.search_immutable(query, corpus.len());
    let mut order: Vec<&Doc> = Vec::with_capacity(corpus.len());
    let mut seen = std::collections::HashSet::new();
    for r in &ranked {
        if let Some(doc) = corpus.iter().find(|d| d.path == r.id) {
            if seen.insert(doc.path.clone()) {
                order.push(doc);
            }
        }
    }
    for doc in corpus {
        if seen.insert(doc.path.clone()) {
            order.push(doc);
        }
    }
    order
}

/// The context-building function under test.
fn build_context(
    query: &str,
    budget: usize,
    strategy: Strategy,
    breadth_frac: f64,
    corpus: &[Doc],
    index: &BM25Index,
) -> Selection {
    let order = ranked_order(query, corpus, index);
    let mut picks = Vec::new();
    let mut used = 0usize;

    let push = |picks: &mut Vec<Picked>,
                used: &mut usize,
                doc: &Doc,
                level: Level,
                tokens: usize,
                excerpt: Option<String>| {
        *used += tokens;
        picks.push(Picked {
            path: doc.path.clone(),
            level,
            tokens,
            excerpt,
        });
    };

    match strategy {
        Strategy::FullFileBm25 => {
            for doc in &order {
                if used + doc.full_tokens <= budget {
                    push(
                        &mut picks,
                        &mut used,
                        doc,
                        Level::Full,
                        doc.full_tokens,
                        None,
                    );
                }
            }
        }
        Strategy::SkeletonOnly => {
            for doc in &order {
                if used + doc.skel_tokens <= budget {
                    push(
                        &mut picks,
                        &mut used,
                        doc,
                        Level::Skeleton,
                        doc.skel_tokens,
                        None,
                    );
                }
            }
        }
        Strategy::SkeletonThenFull => {
            for doc in &order {
                if used + doc.full_tokens <= budget {
                    push(
                        &mut picks,
                        &mut used,
                        doc,
                        Level::Full,
                        doc.full_tokens,
                        None,
                    );
                } else if used + doc.skel_tokens <= budget {
                    push(
                        &mut picks,
                        &mut used,
                        doc,
                        Level::Skeleton,
                        doc.skel_tokens,
                        None,
                    );
                }
            }
        }
        Strategy::BudgetReserve => {
            let depth_budget = ((budget as f64) * (1.0 - breadth_frac)) as usize;
            // Pass 1 — depth: full files, capped at depth_budget.
            for doc in &order {
                if used + doc.full_tokens <= depth_budget {
                    push(
                        &mut picks,
                        &mut used,
                        doc,
                        Level::Full,
                        doc.full_tokens,
                        None,
                    );
                }
            }
            // Pass 2 — breadth: skeletons of further files, using the whole remaining budget.
            let already: std::collections::HashSet<String> =
                picks.iter().map(|p| p.path.clone()).collect();
            for doc in &order {
                if already.contains(&doc.path) {
                    continue;
                }
                if used + doc.skel_tokens <= budget {
                    push(
                        &mut picks,
                        &mut used,
                        doc,
                        Level::Skeleton,
                        doc.skel_tokens,
                        None,
                    );
                }
            }
        }
        Strategy::Excerpt => {
            // Per-file cap keeps any one file from hogging the budget → breadth.
            let per_file_cap = (budget / 6).max(1200);
            for doc in &order {
                let remaining = budget.saturating_sub(used);
                if remaining == 0 {
                    break;
                }
                let cap = per_file_cap.min(remaining).min(doc.full_tokens.max(1));
                if let Some((text, toks)) = extract_relevant_spans(doc, query, cap) {
                    if used + toks <= budget {
                        push(&mut picks, &mut used, doc, Level::Excerpt, toks, Some(text));
                    }
                }
            }
        }
    }

    Selection {
        picks,
        tokens_used: used,
    }
}

// ---------------------------------------------------------------------------
// Coding problems + gold context (self-labeling from IMPROVE_03 audit)
// ---------------------------------------------------------------------------

struct Problem {
    id: &'static str,
    query: &'static str,
    gold: &'static [&'static str],
}

fn problems() -> Vec<Problem> {
    vec![
        Problem {
            id: "mem-eviction-on2",
            query: "Fix the O(N^2) message eviction loop in agent memory add_message; \
                    maintain a running token counter for O(1) eviction",
            gold: &["src/memory/mod.rs"],
        },
        Problem {
            id: "unify-token-accounting",
            query: "Unify the duplicate token accounting between ContextMap and AgentMemory \
                    so context budget is tracked in one place",
            gold: &[
                "src/agent/context_map.rs",
                "src/memory/mod.rs",
                "src/agent/context_management.rs",
            ],
        },
        Problem {
            id: "rag-into-context-map",
            query: "ContextMap find_relevant_files is keyword-only; integrate semantic RAG \
                    embedding search as the primary ranking signal for relevant files",
            gold: &["src/agent/context_map.rs", "src/cognitive/rag.rs"],
        },
        Problem {
            id: "consolidation-readback",
            query: "Memory consolidation is write-only: the long-term store writes JSON but \
                    nothing reads it back. Load consolidated records at startup",
            gold: &["src/consolidation/mod.rs", "src/consolidation/store.rs"],
        },
        Problem {
            id: "approach-stack-unbounded",
            query: "Cap the unbounded approach_stack in cognitive state so long multi-step \
                    tasks cannot grow it without limit",
            gold: &["src/cognitive/state.rs"],
        },
        Problem {
            id: "compression-circuit-breaker",
            query: "The context compression circuit breaker never auto-resets once it opens; \
                    add time-based half-open recovery",
            gold: &["src/agent/compression.rs"],
        },
        Problem {
            id: "rag-rename-events",
            query: "The RAG file watcher ignores file rename events; handle renames by \
                    re-chunking the moved file",
            gold: &["src/cognitive/rag.rs"],
        },
        Problem {
            id: "trim-critical-pin",
            query: "trim_message_history can pin too many critical messages; add a secondary \
                    token cap on pinned critical context",
            gold: &["src/agent/context_management.rs"],
        },
        Problem {
            id: "kg-unbounded-vecs",
            query: "In the knowledge graph, the patterns and smells vectors are unbounded; \
                    add capacity limits with importance-based eviction",
            gold: &["src/cognitive/knowledge_graph.rs"],
        },
        Problem {
            id: "episodic-pattern-detect",
            query: "detect_patterns in episodic memory groups errors by the first five words \
                    which is brittle; cluster episodes semantically instead",
            gold: &["src/cognitive/episodic.rs"],
        },
    ]
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

#[derive(Default, Clone)]
struct Metrics {
    recall_any: f64,    // gold file present at any level
    recall_full: f64,   // gold file present at full fidelity
    recall_code: f64,   // gold file present with real code (full OR excerpt)
    precision: f64,     // fraction of selected files that are gold
    utilization: f64,   // tokens_used / budget
    gold_code_pct: f64, // real-code tokens of gold included / gold full-token mass
}

fn score(problem: &Problem, sel: &Selection, budget: usize, corpus: &[Doc]) -> Metrics {
    let n_gold = problem.gold.len().max(1) as f64;
    let mut any = 0.0;
    let mut full = 0.0;
    let mut code = 0.0;
    let mut gold_code_tok = 0usize;
    let mut gold_total_tok = 0usize;
    for g in problem.gold {
        let doc_tokens = corpus
            .iter()
            .find(|d| &d.path == g)
            .map(|d| d.full_tokens)
            .unwrap_or(0);
        gold_total_tok += doc_tokens;
        if let Some(p) = sel.pick(g) {
            match p.level {
                Level::Full => {
                    any += 1.0;
                    full += 1.0;
                    code += 1.0;
                    gold_code_tok += doc_tokens;
                }
                Level::Excerpt => {
                    any += 1.0;
                    code += 1.0;
                    gold_code_tok += p.tokens.min(doc_tokens);
                }
                Level::Skeleton => any += 1.0,
            }
        }
    }
    let gold_paths: std::collections::HashSet<&str> = problem.gold.iter().copied().collect();
    let selected_gold = sel
        .picks
        .iter()
        .filter(|p| gold_paths.contains(p.path.as_str()))
        .count() as f64;
    let precision = if sel.picks.is_empty() {
        0.0
    } else {
        selected_gold / sel.picks.len() as f64
    };
    Metrics {
        recall_any: any / n_gold,
        recall_full: full / n_gold,
        recall_code: code / n_gold,
        precision,
        utilization: sel.tokens_used as f64 / budget as f64,
        gold_code_pct: if gold_total_tok == 0 {
            1.0
        } else {
            gold_code_tok as f64 / gold_total_tok as f64
        },
    }
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

struct Args {
    budgets: Vec<usize>,
    breadth_frac: f64,
    e2e: usize,
    e2e_budgets: Vec<usize>,
    e2e_strategy: Strategy,
    root: PathBuf,
    json_out: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut budgets = vec![8_000usize, 32_000, 128_000, 1_048_576];
    let mut breadth_frac = DEFAULT_BREADTH_FRAC;
    let mut e2e = 0usize;
    // Default e2e sweep: incrementally bigger context requests to the model.
    let mut e2e_budgets = vec![8_000usize, 32_000, 128_000];
    let mut e2e_strategy = Strategy::Excerpt;
    let mut root = PathBuf::from(".");
    let mut json_out = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--budgets" => {
                if let Some(v) = it.next() {
                    budgets = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                }
            }
            "--breadth-frac" => {
                breadth_frac = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(DEFAULT_BREADTH_FRAC)
                    .clamp(0.0, 0.9);
            }
            "--e2e" => e2e = it.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "--e2e-budget" => {
                if let Some(v) = it.next().and_then(|v| v.parse().ok()) {
                    e2e_budgets = vec![v];
                }
            }
            "--e2e-budgets" => {
                if let Some(v) = it.next() {
                    e2e_budgets = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                }
            }
            "--e2e-strategy" => {
                if let Some(v) = it.next() {
                    e2e_strategy = match v.as_str() {
                        "full-bm25" => Strategy::FullFileBm25,
                        "hybrid" => Strategy::SkeletonThenFull,
                        "skeleton" => Strategy::SkeletonOnly,
                        "reserve" => Strategy::BudgetReserve,
                        _ => Strategy::Excerpt,
                    };
                }
            }
            "--root" => {
                if let Some(v) = it.next() {
                    root = PathBuf::from(v);
                }
            }
            "--json" => json_out = it.next().map(PathBuf::from),
            "-h" | "--help" => {
                println!(
                    "context_select_bench [--budgets a,b,c] [--breadth-frac F] [--e2e N] \
                     [--e2e-budgets a,b,c | --e2e-budget T] [--e2e-strategy S] [--root DIR] \
                     [--json FILE]\n\
                     strategies: full-bm25 hybrid skeleton reserve excerpt\n\
                     --e2e sweeps the first N problems across incrementally bigger contexts."
                );
                std::process::exit(0);
            }
            _ => {}
        }
    }
    e2e_budgets.sort_unstable();
    Args {
        budgets,
        breadth_frac,
        e2e,
        e2e_budgets,
        e2e_strategy,
        root,
        json_out,
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args();
    let corpus = build_corpus(&args.root).context("building corpus from src/")?;
    if corpus.is_empty() {
        anyhow::bail!(
            "no .rs files found under {:?}/src — run from the repo root or pass --root",
            args.root
        );
    }

    let mut index = BM25Index::new();
    for doc in &corpus {
        index.add(doc.path.clone(), doc.content.clone());
    }

    let probs = problems();
    println!(
        "Corpus: {} files under {}/src   Problems: {}   Budgets: {:?}   breadth_frac={:.2}\n",
        corpus.len(),
        args.root.display(),
        probs.len(),
        args.budgets,
        args.breadth_frac,
    );

    #[derive(Default)]
    struct Agg {
        ra: f64,
        rf: f64,
        rc: f64,
        gc: f64,
        pr: f64,
        n: usize,
    }
    let mut agg: BTreeMap<(&'static str, usize), Agg> = BTreeMap::new();
    let mut rows: Vec<serde_json::Value> = Vec::new();

    for strategy in Strategy::all() {
        println!("Strategy: {}", strategy.name());
        println!(
            "  {:<26} {:>8} {:>10} {:>11} {:>11} {:>9} {:>7} {:>6}",
            "problem",
            "budget",
            "recall_any",
            "recall_full",
            "recall_code",
            "goldCode%",
            "prec",
            "util"
        );
        for p in &probs {
            for &budget in &args.budgets {
                let sel = build_context(
                    p.query,
                    budget,
                    strategy,
                    args.breadth_frac,
                    &corpus,
                    &index,
                );
                let m = score(p, &sel, budget, &corpus);
                let e = agg.entry((strategy.name(), budget)).or_default();
                e.ra += m.recall_any;
                e.rf += m.recall_full;
                e.rc += m.recall_code;
                e.gc += m.gold_code_pct;
                e.pr += m.precision;
                e.n += 1;
                println!(
                    "  {:<26} {:>8} {:>10.2} {:>11.2} {:>11.2} {:>8.0}% {:>7.2} {:>5.0}%",
                    p.id,
                    budget,
                    m.recall_any,
                    m.recall_full,
                    m.recall_code,
                    m.gold_code_pct * 100.0,
                    m.precision,
                    m.utilization * 100.0
                );
                rows.push(serde_json::json!({
                    "strategy": strategy.name(),
                    "problem": p.id,
                    "budget": budget,
                    "recall_any": m.recall_any,
                    "recall_full": m.recall_full,
                    "recall_code": m.recall_code,
                    "gold_code_pct": m.gold_code_pct,
                    "precision": m.precision,
                    "utilization": m.utilization,
                    "files_selected": sel.picks.len(),
                    "tokens_used": sel.tokens_used,
                }));
            }
        }
        println!();
    }

    println!("Aggregate (mean over {} problems):", probs.len());
    println!(
        "  {:<12} {:>8} {:>11} {:>12} {:>12} {:>10} {:>7}",
        "strategy", "budget", "recall_any", "recall_full", "recall_code", "goldCode%", "prec"
    );
    for ((strat, budget), a) in &agg {
        let n = a.n as f64;
        println!(
            "  {:<12} {:>8} {:>11.2} {:>12.2} {:>12.2} {:>9.0}% {:>7.2}",
            strat,
            budget,
            a.ra / n,
            a.rf / n,
            a.rc / n,
            (a.gc / n) * 100.0,
            a.pr / n
        );
    }
    println!();

    if let Some(path) = &args.json_out {
        let payload = serde_json::json!({
            "corpus_files": corpus.len(),
            "budgets": args.budgets,
            "breadth_frac": args.breadth_frac,
            "rows": rows,
        });
        std::fs::write(path, serde_json::to_string_pretty(&payload)?)?;
        println!("Wrote per-problem results to {}", path.display());
    }

    if args.e2e > 0 {
        run_e2e(&args, &corpus, &index, &probs).await?;
    } else {
        println!(
            "(retrieval-only run. Add --e2e N to validate against GLM-5.2 on the first N problems.)"
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// End-to-end subset: does the selected context let GLM-5.2 locate the fix?
// ---------------------------------------------------------------------------

async fn run_e2e(args: &Args, corpus: &[Doc], index: &BM25Index, probs: &[Problem]) -> Result<()> {
    use selfware::api::{ApiClient, Message, ThinkingMode};
    use selfware::config::Config;

    let config = Config::load(Some("selfware.toml"))
        .context("loading selfware.toml (needs SELFWARE_API_KEY for OpenRouter)")?;
    let client = ApiClient::new(&config).context("building API client")?;

    println!(
        "\nEnd-to-end sweep on {} problem(s) via model '{}', strategy '{}'.",
        args.e2e.min(probs.len()),
        config.model,
        args.e2e_strategy.name(),
    );
    println!(
        "Incrementally bigger context requests: budgets {:?}\n",
        args.e2e_budgets
    );

    let system = Message::system(
        "You are a senior Rust engineer working in the `selfware` codebase. \
         You are given a code context (files may be full, query-relevant excerpts, \
         or signature-only skeletons) and a task. Answer with ONLY the repo-relative \
         path(s) of the file(s) you would edit, one per line. No prose.",
    );

    // pass_at[budget] = hits accumulated across problems.
    let mut pass_at: BTreeMap<usize, usize> = BTreeMap::new();
    let n = args.e2e.min(probs.len());

    for p in probs.iter().take(args.e2e) {
        println!("{}  (gold: {})", p.id, p.gold.join(", "));
        // Ascending budgets = progressively larger requests to the model.
        for &budget in &args.e2e_budgets {
            let sel = build_context(
                p.query,
                budget,
                args.e2e_strategy,
                args.breadth_frac,
                corpus,
                index,
            );
            let context = sel.render(corpus);
            let user = Message::user(format!(
                "# Code context\n{context}\n\n# Task\n{}\n\n\
                 Which file(s) would you edit? Repo-relative paths only.",
                p.query
            ));

            let answer = match client
                .chat(vec![system.clone(), user], None, ThinkingMode::Disabled)
                .await
            {
                Ok(resp) => resp
                    .choices
                    .first()
                    .map(|c| c.message.content.text().to_string())
                    .unwrap_or_default(),
                Err(e) => {
                    println!("    {:>8} tok  ERROR: {e}", budget);
                    continue;
                }
            };

            let hit = p.gold.iter().any(|g| {
                answer.contains(g)
                    || Path::new(g)
                        .parent()
                        .and_then(|par| par.file_name())
                        .and_then(|s| s.to_str())
                        .map(|dir| {
                            let fname = Path::new(g)
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("");
                            answer.contains(&format!("{dir}/{fname}"))
                        })
                        .unwrap_or(false)
            });
            if hit {
                *pass_at.entry(budget).or_default() += 1;
            }
            let gold_in_context = p.gold.iter().filter(|g| sel.pick(g).is_some()).count();
            println!(
                "    {:>8} tok  {}  (gold in ctx: {}/{}, {} files, {} tok used)  model: {}",
                budget,
                if hit { "HIT " } else { "miss" },
                gold_in_context,
                p.gold.len(),
                sel.picks.len(),
                sel.tokens_used,
                answer
                    .replace('\n', " | ")
                    .chars()
                    .take(60)
                    .collect::<String>()
            );
        }
    }

    println!("\nPass@budget (located a gold file), mean over {n} problems:");
    for &budget in &args.e2e_budgets {
        let h = pass_at.get(&budget).copied().unwrap_or(0);
        let rate = if n > 0 { h as f64 / n as f64 } else { 0.0 };
        println!("  {:>8} tok   {}/{}   {:.0}%", budget, h, n, rate * 100.0);
    }
    Ok(())
}
