//! Tier Bench — measures the auto context tier machinery on this repo.
//!
//! Builds the evolve graph from `src/` and reports, per context tier, the
//! *measured* token cost (real source projection: comment-stripping, skeleton
//! extraction, component map), the wall time, and the I/O reads — side by side
//! with the composer's per-node heuristic estimate. It then resolves the tier
//! ladder (`auto` mode) against several model windows, demonstrating the lazy
//! short-circuit: large windows do zero reduction I/O.
//!
//! # Usage
//!
//! ```bash
//! cargo run --release --example tier_bench
//! cargo run --release --example tier_bench -- --dump /tmp/tier-dump
//! ```
//!
//! Flags:
//! - `--dump DIR`  Also write `map.txt`, `lite.txt` and `compact.txt` into DIR
//!
//! Must run from the repository root (the graph is built from `./src`).

use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};

use selfware::evolve::{
    build_map, extract_rust_skeleton, fit_tier, reduce_source, ContextComposer, FitBudget,
    GraphBuilder, NodeLayer, TierMeasurer, TIER_LADDER,
};
use selfware::token_count::estimate_content_tokens;

fn main() -> Result<()> {
    let dump_dir = parse_dump_dir();

    println!("Building evolve graph from ./src ...");
    let scan_start = Instant::now();
    let graph = GraphBuilder::new(Path::new("src"))
        .scan_src()
        .context("graph scan failed (run from the repository root)")?;
    println!(
        "graph: {} nodes, {} edges (scan took {:.1} ms)\n",
        graph.nodes.len(),
        graph.edges.len(),
        scan_start.elapsed().as_secs_f64() * 1e3
    );

    section_per_tier(&graph);
    section_fit_windows(&graph);

    if let Some(dir) = dump_dir {
        section_dump(&graph, &dir)?;
    }

    Ok(())
}

/// Section 1 — per-tier measured vs heuristic token cost.
fn section_per_tier(graph: &selfware::evolve::Graph) {
    println!("== Section 1: per-tier measurement (measured vs composer heuristic) ==");
    println!(
        "{:<13} {:>13} {:>10} {:>9} {:>13} {:>8}",
        "tier", "measured tok", "time ms", "io reads", "heuristic tok", "m/h"
    );
    println!("{}", "-".repeat(70));

    let measurer = TierMeasurer::new(graph, Path::new("."));
    let mut last_io = measurer.io_reads();

    for mode in TIER_LADDER {
        let start = Instant::now();
        let measured = measurer.measure(&mode);
        let elapsed_ms = start.elapsed().as_secs_f64() * 1e3;
        let io_delta = measurer.io_reads() - last_io;
        last_io = measurer.io_reads();

        let mut composer = ContextComposer::new(graph.clone());
        composer.set_mode(mode.clone());
        let heuristic = composer.estimate_tokens();

        // Map's heuristic is 0 by design (a compiled artifact, not a per-node
        // projection) — there is no meaningful ratio to print.
        if heuristic == 0 {
            println!(
                "{:<13} {:>13} {:>10.1} {:>9} {:>13} {:>8}",
                mode.name(),
                measured,
                elapsed_ms,
                io_delta,
                "-",
                "-"
            );
        } else {
            println!(
                "{:<13} {:>13} {:>10.1} {:>9} {:>13} {:>7.2}x",
                mode.name(),
                measured,
                elapsed_ms,
                io_delta,
                heuristic,
                measured as f64 / heuristic as f64
            );
        }
    }
    println!();
}

/// Section 2 — ladder resolution (`auto`) per model window.
fn section_fit_windows(graph: &selfware::evolve::Graph) {
    println!("== Section 2: tier ladder resolution per context window (auto) ==");
    println!(
        "{:>9} {:>13} {:<13} {:>13} {:>5} {:>10} {:>13}",
        "window",
        "usable budget",
        "resolved tier",
        "measured tok",
        "fits",
        "time ms",
        "cum io reads"
    );
    println!("{}", "-".repeat(82));

    for context_length in [
        8192usize, 32768, 131072, 262144, 1048576, 3_000_000, 4_500_000,
    ] {
        let budget = FitBudget::new(context_length, 4096, 0.70);
        // Fresh measurer per window so the I/O cost of one resolution is visible.
        let measurer = TierMeasurer::new(graph, Path::new("."));
        let start = Instant::now();
        let outcome = fit_tier(&measurer, &budget);
        let elapsed_ms = start.elapsed().as_secs_f64() * 1e3;

        println!(
            "{:>9} {:>13} {:<13} {:>13} {:>5} {:>10.1} {:>13}",
            context_length,
            budget.usable(),
            outcome.mode.name(),
            outcome.measured_tokens,
            outcome.fits,
            elapsed_ms,
            measurer.io_reads()
        );
    }
    println!();
}

/// Section 3 — dump tier artifacts (map / lite / compact) to a directory.
fn section_dump(graph: &selfware::evolve::Graph, dir: &Path) -> Result<()> {
    println!("== Section 3: artifact dump -> {} ==", dir.display());
    std::fs::create_dir_all(dir)
        .with_context(|| format!("cannot create dump dir {}", dir.display()))?;

    // map.txt: the compiled component index.
    let map = build_map(graph, Path::new("."));
    write_artifact(dir, "map.txt", &map.rendered)?;

    // lite.txt / compact.txt: per-node projections over readable Code nodes.
    let mut lite_parts: Vec<String> = Vec::new();
    let mut compact_parts: Vec<String> = Vec::new();
    for node in graph.nodes.iter().filter(|n| n.layer == NodeLayer::Code) {
        let Some(rel) = node.path.as_deref() else {
            continue;
        };
        let Ok(src) = std::fs::read_to_string(Path::new(".").join(rel)) else {
            continue;
        };
        if rel.ends_with(".rs") {
            lite_parts.push(extract_rust_skeleton(Path::new(rel), &src).render());
        }
        compact_parts.push(reduce_source(&src));
    }

    write_artifact(dir, "lite.txt", &lite_parts.join("\n\n"))?;
    write_artifact(dir, "compact.txt", &compact_parts.join("\n\n"))?;
    println!();
    Ok(())
}

fn write_artifact(dir: &Path, name: &str, content: &str) -> Result<()> {
    let path = dir.join(name);
    std::fs::write(&path, content).with_context(|| format!("cannot write {}", path.display()))?;
    println!(
        "  {:<12} {:>9} bytes  {:>9} tokens",
        name,
        content.len(),
        estimate_content_tokens(content)
    );
    Ok(())
}

/// Parse `--dump DIR` (both `--dump DIR` and `--dump=DIR` forms).
fn parse_dump_dir() -> Option<std::path::PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--dump" {
            return args.next().map(std::path::PathBuf::from);
        }
        if let Some(dir) = arg.strip_prefix("--dump=") {
            return Some(std::path::PathBuf::from(dir));
        }
    }
    None
}
