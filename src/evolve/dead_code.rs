//! Reachability-based dead-code detection.
//!
//! `cargo`'s `dead_code` lint misses unused `pub` items and false-positives on
//! `cfg`-gated cross-platform code (a Linux-only helper looks "never used" on a
//! macOS build). This analysis works on source *text* instead: it counts every
//! identifier occurrence across the whole tree — including code behind inactive
//! `cfg`s — so a symbol referenced only from another platform's branch is still
//! seen as live. A symbol whose name appears **only at its own definition(s)**
//! is reported as dead.
//!
//! This is deliberately conservative (it under-reports rather than over-reports
//! dead code): if a name appears anywhere else, we treat it as reachable. Names
//! that are dispatched by trait/std machinery (`new`, `fmt`, `from`, …) and
//! entry points (`main`, `#[test]`) are excluded.

use anyhow::Result;
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use walkdir::WalkDir;

fn def_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(?P<indent>\s*)(?P<vis>pub(?:\([^)]*\))?\s+)?(?:async\s+|unsafe\s+|const\s+|extern\s+\S+\s+)*(?P<kind>fn|struct|enum|trait|type|const|static)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)",
        )
        .expect("valid def regex")
    })
}

fn ident_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[A-Za-z_][A-Za-z0-9_]*").expect("valid ident regex"))
}

/// Names dispatched by trait/std machinery or otherwise reachable without an
/// explicit textual reference — excluded to avoid false positives.
const DISPATCHED: &[&str] = &[
    "new",
    "default",
    "fmt",
    "from",
    "into",
    "try_from",
    "try_into",
    "drop",
    "clone",
    "eq",
    "ne",
    "hash",
    "cmp",
    "partial_cmp",
    "deref",
    "deref_mut",
    "as_ref",
    "as_mut",
    "next",
    "poll",
    "add",
    "sub",
    "mul",
    "div",
    "index",
    "call",
    "main",
    "run",
    "execute",
    "handle",
    "visit",
];

/// A symbol that appears to be dead (referenced nowhere but its definition).
#[derive(Debug, Clone, Serialize)]
pub struct DeadSymbol {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub line: usize,
    pub is_pub: bool,
    /// True if the definition sits under a `#[cfg(...)]` — likely intentional
    /// platform/feature code rather than truly dead; surfaced but de-ranked.
    pub cfg_gated: bool,
}

struct Def {
    name: String,
    kind: String,
    path: String,
    line: usize,
    is_pub: bool,
    cfg_gated: bool,
    is_test: bool,
}

pub struct DeadCodeAnalyzer {
    root: PathBuf,
}

impl DeadCodeAnalyzer {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn find(&self) -> Result<Vec<DeadSymbol>> {
        let mut defs: Vec<Def> = Vec::new();
        let mut occ: HashMap<String, usize> = HashMap::new();

        // Definitions come from `src/` (the shippable code we might delete), but
        // *references* are counted across tests and examples too — otherwise a
        // `pub fn` exercised only by its (relocated) unit test looks dead.
        let src_root = self.root.join("src");
        let ref_roots = ["src", "tests", "examples"];
        for dir in ref_roots {
            let dir_path = self.root.join(dir);
            let is_src = dir == "src";
            for entry in WalkDir::new(&dir_path).into_iter().filter_map(|e| e.ok()) {
                let p = entry.path();
                if p.extension().is_some_and(|e| e == "rs") {
                    let Ok(text) = std::fs::read_to_string(p) else {
                        continue;
                    };
                    if is_src {
                        let rel = p
                            .strip_prefix(&src_root)
                            .unwrap_or(p)
                            .to_string_lossy()
                            .replace('\\', "/");
                        collect_defs(&text, &rel, &mut defs);
                    }
                    for m in ident_re().find_iter(&text) {
                        *occ.entry(m.as_str().to_string()).or_default() += 1;
                    }
                }
            }
        }

        // How many definitions share each name (each contributes one textual
        // occurrence at its own signature).
        let mut def_count: HashMap<&str, usize> = HashMap::new();
        for d in &defs {
            *def_count.entry(d.name.as_str()).or_default() += 1;
        }

        let mut dead: Vec<DeadSymbol> = defs
            .iter()
            .filter(|d| {
                let name = d.name.as_str();
                if DISPATCHED.contains(&name) || name.starts_with("__") || d.is_test {
                    return false;
                }
                // Occurrences beyond the definition signatures themselves.
                let total = occ.get(name).copied().unwrap_or(0);
                let self_refs = def_count.get(name).copied().unwrap_or(1);
                total <= self_refs
            })
            .map(|d| DeadSymbol {
                name: d.name.clone(),
                kind: d.kind.clone(),
                path: d.path.clone(),
                line: d.line,
                is_pub: d.is_pub,
                cfg_gated: d.cfg_gated,
            })
            .collect();

        // Real dead code (not cfg-gated) first; then by kind for stable output.
        dead.sort_by(|a, b| {
            a.cfg_gated
                .cmp(&b.cfg_gated)
                .then_with(|| a.path.cmp(&b.path))
                .then_with(|| a.line.cmp(&b.line))
        });
        Ok(dead)
    }
}

fn collect_defs(text: &str, rel_path: &str, out: &mut Vec<Def>) {
    let lines: Vec<&str> = text.lines().collect();
    // Track `#[cfg(test)] mod … {` blocks (brace counting) so helpers and
    // #[test] fns inside test modules are never dead-code candidates.
    let mut test_ranges: Vec<(usize, usize)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if t.starts_with("#[cfg(test)]") {
            // Find the opening brace of the following mod/impl block.
            let mut depth = 0i32;
            let mut started = false;
            let mut end = lines.len();
            for (j, l) in lines.iter().enumerate().skip(i) {
                for ch in l.chars() {
                    match ch {
                        '{' => {
                            depth += 1;
                            started = true;
                        }
                        '}' => {
                            depth -= 1;
                            if started && depth == 0 {
                                end = j + 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if started && depth == 0 {
                    break;
                }
            }
            if started {
                test_ranges.push((i, end));
            }
        }
    }
    let in_test_range = |line_no: usize| {
        test_ranges
            .iter()
            .any(|&(s, e)| line_no >= s && line_no < e)
    };

    for (i, line) in lines.iter().enumerate() {
        let Some(c) = def_re().captures(line) else {
            continue;
        };
        // Walk back past CONTIGUOUS attribute lines (not just one), tracking
        // bracket balance so multi-line attributes like
        //   #[cfg_attr(target_os = "windows",
        //              ignore = "reason")]
        //   #[tokio::test]
        // are all seen (analyzer false-positive class).
        let mut attrs: Vec<&str> = Vec::new();
        let mut j = i;
        let mut open = 0i32;
        while j > 0 {
            let prev = lines[j - 1].trim_start();
            let is_attr_start = prev.starts_with("#[");
            let in_continuation = open > 0;
            if !is_attr_start && !in_continuation {
                break;
            }
            for ch in prev.chars() {
                match ch {
                    '[' => open += 1,
                    ']' => open -= 1,
                    _ => {}
                }
            }
            attrs.push(prev);
            j -= 1;
        }
        let has_attr = |prefix: &str| attrs.iter().any(|a| a.starts_with(prefix));
        // A preceding attribute containing `cfg(` marks platform/feature gating.
        let cfg_gated = has_attr("#[cfg(") || has_attr("#[cfg_attr(");
        // Test code is exercised by the harness, not by call sites — the
        // occurrence heuristic can never see it, so it must never be a
        // dead-code candidate.
        let is_test = has_attr("#[test")
            || has_attr("#[tokio::test")
            || has_attr("#[cfg(test")
            || in_test_range(i);
        out.push(Def {
            name: c["name"].to_string(),
            kind: c["kind"].to_string(),
            path: rel_path.to_string(),
            line: i + 1,
            is_pub: c.name("vis").is_some(),
            cfg_gated: cfg_gated || is_test,
            is_test,
        });
    }
}
