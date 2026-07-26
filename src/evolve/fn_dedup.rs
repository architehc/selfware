//! Function-level duplication detector.
//!
//! Whole-file dedup (see [`super::dedup`]) only catches byte-identical files,
//! so it misses the real "excess code": the same logic copy-pasted into two
//! functions in different files. This module extracts every `fn` body, then:
//!   * groups by a hash of the whitespace-normalized body → **exact** clones,
//!   * compares size-bucketed bodies by Jaccard over normalized line sets →
//!     **near** clones above a threshold.
//!
//! Trivial functions (below `MIN_FN_LINES` normalized lines) are ignored so
//! one-line getters and `Default` impls don't drown the signal. Only Rust
//! source under the project is scanned; generated/test config files never
//! reach this analysis.

use anyhow::Result;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use super::dedup::{jaccard, normalized_lines};

/// Minimum normalized-line count for a function to be considered — below this
/// the body is too small for duplication to be meaningful.
const MIN_FN_LINES: usize = 6;
/// Jaccard threshold over normalized line sets for a near-duplicate.
const SIMILARITY_THRESHOLD: f64 = 0.85;

/// Where a function lives.
#[derive(Debug, Clone, Serialize)]
pub struct FnLocation {
    pub name: String,
    pub path: String,
    pub line: usize,
    pub lines: usize,
}

/// A pair of functions flagged as duplicate/near-duplicate.
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateFnPair {
    pub kind: String,
    pub similarity: f64,
    pub first: FnLocation,
    pub second: FnLocation,
}

struct FnBody {
    loc: FnLocation,
    hash: String,
    lines: HashSet<String>,
}

pub struct FnDedupAnalyzer {
    root: PathBuf,
}

impl FnDedupAnalyzer {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Extract functions from every Rust source file, then report duplicate and
    /// near-duplicate pairs, ranked by (similarity × size) so the biggest,
    /// closest clones come first.
    pub fn find(&self) -> Result<Vec<DuplicateFnPair>> {
        let mut fns: Vec<FnBody> = Vec::new();
        for entry in WalkDir::new(&self.root).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.extension().is_some_and(|e| e == "rs") {
                if let Ok(text) = std::fs::read_to_string(p) {
                    let rel = p
                        .strip_prefix(&self.root)
                        .unwrap_or(p)
                        .to_string_lossy()
                        .replace('\\', "/");
                    extract_functions(&text, &rel, &mut fns);
                }
            }
        }

        let mut pairs: Vec<DuplicateFnPair> = Vec::new();

        // Exact clones: group by normalized-body hash.
        let mut by_hash: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, f) in fns.iter().enumerate() {
            by_hash.entry(f.hash.as_str()).or_default().push(i);
        }
        let mut exact_pairs: HashSet<(usize, usize)> = HashSet::new();
        for group in by_hash.values().filter(|g| g.len() > 1) {
            for w in 0..group.len() {
                for x in (w + 1)..group.len() {
                    let (a, b) = (group[w], group[x]);
                    exact_pairs.insert((a.min(b), a.max(b)));
                    pairs.push(DuplicateFnPair {
                        kind: "exact".to_string(),
                        similarity: 1.0,
                        first: fns[a].loc.clone(),
                        second: fns[b].loc.clone(),
                    });
                }
            }
        }

        // Near clones: bucket by size so we only compare bodies of similar length.
        let mut by_bucket: HashMap<usize, Vec<usize>> = HashMap::new();
        for (i, f) in fns.iter().enumerate() {
            by_bucket.entry(f.loc.lines / 5).or_default().push(i);
        }
        for (&bucket, members) in &by_bucket {
            let mut candidates = members.clone();
            if let Some(adjacent) = by_bucket.get(&(bucket + 1)) {
                candidates.extend(adjacent);
            }
            for &a in members {
                for &b in candidates.iter().filter(|&&c| c > a) {
                    let key = (a.min(b), a.max(b));
                    if exact_pairs.contains(&key) {
                        continue;
                    }
                    let sim = jaccard(&fns[a].lines, &fns[b].lines);
                    if sim >= SIMILARITY_THRESHOLD {
                        pairs.push(DuplicateFnPair {
                            kind: "near".to_string(),
                            similarity: (sim * 100.0).round() / 100.0,
                            first: fns[a].loc.clone(),
                            second: fns[b].loc.clone(),
                        });
                    }
                }
            }
        }

        pairs.sort_by(|a, b| {
            let sa = a.similarity * a.first.lines as f64;
            let sb = b.similarity * b.first.lines as f64;
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(pairs)
    }
}

/// Extract top-level and nested `fn` bodies from a source file via brace
/// matching. Records the normalized line set and a hash for each body large
/// enough to matter.
fn extract_functions(text: &str, rel_path: &str, out: &mut Vec<FnBody>) {
    let bytes = text.as_bytes();
    let mut search = 0;
    while search < text.len() {
        let Some(rel) = text[search..].find("fn ") else {
            break;
        };
        let idx = search + rel;
        // Require `fn` to start a token (preceded by whitespace or start).
        let ok_prefix = idx == 0 || matches!(bytes[idx - 1], b' ' | b'\t' | b'\n' | b'(' | b'<');
        // Find the function name after `fn `.
        let after = idx + 3;
        let name: String = text[after..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !ok_prefix || name.is_empty() {
            search = idx + 3;
            continue;
        }
        // Find the opening brace of the body (skip signature; bail on `;` which
        // means a trait method declaration with no body).
        let mut i = after + name.len();
        let mut brace_start = None;
        while i < bytes.len() {
            match bytes[i] {
                b'{' => {
                    brace_start = Some(i);
                    break;
                }
                b';' => break,
                _ => {}
            }
            i += 1;
        }
        let Some(bstart) = brace_start else {
            search = idx + 3;
            continue;
        };
        // Match braces to find the body end.
        let mut depth = 0usize;
        let mut j = bstart;
        while j < bytes.len() {
            match bytes[j] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        let body = &text[bstart..(j + 1).min(bytes.len())];
        let norm = normalized_lines(body);
        if norm.len() >= MIN_FN_LINES {
            let line = text[..idx].bytes().filter(|&b| b == b'\n').count() + 1;
            let mut concat: Vec<&String> = norm.iter().collect();
            concat.sort();
            let joined = concat
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            out.push(FnBody {
                loc: FnLocation {
                    name,
                    path: rel_path.to_string(),
                    line,
                    lines: body.lines().count(),
                },
                hash: format!("{:x}", Sha256::digest(joined.as_bytes())),
                lines: norm,
            });
        }
        search = (j + 1).min(text.len());
    }
}
