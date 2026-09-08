//! Process-wide cache of the parsed evolve graph.
//!
//! Parsing `.selfware/evolve-graph.yaml` costs ~1-2s (110K lines of YAML),
//! so the graph query tools share one `Arc<GraphIndex>` keyed on the
//! file's (path, mtime, len): an unchanged file returns the cached `Arc`,
//! a rewritten file triggers exactly one re-parse. Callers on the async hot
//! path must invoke through `tokio::task::spawn_blocking` (see
//! `tools::graph`), mirroring the evolve server's pattern.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::SystemTime;

use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};

use super::graph_index::GraphIndex;
use super::OntologyStore;

struct CachedIndex {
    path: PathBuf,
    mtime: SystemTime,
    len: u64,
    index: Arc<GraphIndex>,
}

static CACHE: OnceLock<RwLock<Option<CachedIndex>>> = OnceLock::new();

/// Where `selfware self-evolve` writes the graph under a project root
/// (see `evolve::server`).
pub fn graph_path(root: &Path) -> PathBuf {
    root.join(".selfware").join("evolve-graph.yaml")
}

/// The shared index for `root`'s evolve graph, parsing at most once per
/// file revision. Errors honestly when the graph has never been built.
pub fn shared_graph_index(root: &Path) -> Result<Arc<GraphIndex>> {
    let path = graph_path(root);
    let cache = CACHE.get_or_init(|| RwLock::new(None));
    // The write lock is held across the parse on purpose: it guarantees one
    // YAML load per revision even under concurrent first calls, and readers
    // here are tool calls, not latency-sensitive hot paths.
    let mut slot = cache
        .write()
        .map_err(|_| anyhow!("graph cache lock poisoned"))?;
    Ok(cached_or_load(&mut slot, &path)?.clone())
}

/// Cache-hit core of [`shared_graph_index`], factored out so tests can drive
/// it with a local slot instead of the process-wide cache.
fn cached_or_load<'a>(
    slot: &'a mut Option<CachedIndex>,
    path: &Path,
) -> Result<&'a Arc<GraphIndex>> {
    let metadata = std::fs::metadata(path).map_err(|_| {
        anyhow!(
            "no evolve graph at {}; run `selfware self-evolve` to build one",
            path.display()
        )
    })?;
    let mtime = metadata
        .modified()
        .context("evolve graph mtime unavailable")?;
    let len = metadata.len();
    let cache_hit = slot
        .as_ref()
        .is_some_and(|cached| cached.path == path && cached.mtime == mtime && cached.len == len);
    if cache_hit {
        return Ok(&slot.as_ref().expect("hit just checked").index);
    }
    let bytes =
        std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let yaml_hash = format!("{:x}", Sha256::digest(&bytes));
    let graph = OntologyStore::new(path)
        .load()
        .with_context(|| format!("failed to parse {}", path.display()))?;
    let index = Arc::new(GraphIndex::from_graph(Arc::new(graph), &yaml_hash));
    *slot = Some(CachedIndex {
        path: path.to_path_buf(),
        mtime,
        len,
        index,
    });
    Ok(&slot.as_ref().expect("just populated").index)
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/graph_cache_test.rs"]
mod graph_cache_test;
