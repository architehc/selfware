//! Structural outline of the codebase for navigation: module → class → method.
//!
//! The graph shows module dependencies; this shows what's *inside* — every
//! `struct`/`enum`/`trait` (a "class") with the methods defined in its `impl`
//! blocks, plus free functions. It gives users a granular, visual way to drill
//! from a component into its types and their behavior.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use regex::Regex;
use serde::Serialize;
use std::sync::OnceLock;

use walkdir::WalkDir;

fn type_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(struct|enum|trait)\s+([A-Za-z_]\w*)")
            .expect("type regex")
    })
}
fn impl_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^\s*impl(?:<[^>]*>)?\s+(?:([A-Za-z_][\w:]*)(?:<[^>]*>)?\s+for\s+)?([A-Za-z_]\w*)",
        )
        .expect("impl regex")
    })
}
fn fn_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+|unsafe\s+|const\s+|extern\s+\S+\s+)*fn\s+([A-Za-z_]\w*)")
            .expect("fn regex")
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Method {
    pub name: String,
    pub line: usize,
    pub is_pub: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClassEntry {
    pub name: String,
    /// struct | enum | trait
    pub kind: String,
    pub line: usize,
    pub methods: Vec<Method>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileStructure {
    pub module: String,
    pub path: String,
    pub classes: Vec<ClassEntry>,
    pub free_functions: Vec<Method>,
}

pub struct StructureAnalyzer {
    root: PathBuf,
}

impl StructureAnalyzer {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Extract the module → class → method outline for every source file that
    /// has any classes or free functions.
    pub fn outline(&self) -> Result<Vec<FileStructure>> {
        let mut out = Vec::new();
        for entry in WalkDir::new(&self.root).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.extension().map_or(false, |e| e == "rs")
                && !p.to_string_lossy().ends_with("_test.rs")
            {
                if let Ok(text) = std::fs::read_to_string(p) {
                    let rel = p
                        .strip_prefix(&self.root)
                        .unwrap_or(p)
                        .to_string_lossy()
                        .replace('\\', "/");
                    if let Some(fs) = file_structure(&rel, &text) {
                        out.push(fs);
                    }
                }
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }
}

fn module_of(path: &str) -> String {
    let p = path.strip_prefix("src/").unwrap_or(path);
    let p = p.strip_suffix(".rs").unwrap_or(p);
    let parts: Vec<&str> = p
        .split('/')
        .filter(|s| !s.is_empty() && *s != "mod")
        .collect();
    if parts.is_empty() {
        "crate".into()
    } else {
        parts.join("::")
    }
}

/// Parse one file: types → classes, `impl` bodies → methods on the target type,
/// module-level fns → free functions. Impl targets are tracked by brace depth.
fn file_structure(rel_path: &str, text: &str) -> Option<FileStructure> {
    let mut classes: BTreeMap<String, ClassEntry> = BTreeMap::new();
    let mut free = Vec::new();
    // stack of (impl_target, brace_depth_at_open)
    let mut impl_stack: Vec<(String, i32)> = Vec::new();
    let mut depth: i32 = 0;

    for (i, line) in text.lines().enumerate() {
        let ln = i + 1;
        if let Some(c) = type_re().captures(line) {
            let name = c[2].to_string();
            classes.entry(name.clone()).or_insert(ClassEntry {
                name,
                kind: c[1].to_string(),
                line: ln,
                methods: Vec::new(),
            });
        } else if let Some(c) = impl_re().captures(line) {
            // impl target is the type being implemented (group 2).
            impl_stack.push((c[2].to_string(), depth));
        } else if let Some(c) = fn_re().captures(line) {
            let method = Method {
                name: c[1].to_string(),
                line: ln,
                is_pub: line.trim_start().starts_with("pub"),
            };
            match impl_stack.last() {
                Some((target, _)) => classes
                    .entry(target.clone())
                    .or_insert(ClassEntry {
                        name: target.clone(),
                        kind: "impl".to_string(),
                        line: ln,
                        methods: Vec::new(),
                    })
                    .methods
                    .push(method),
                None => free.push(method),
            }
        }

        // Track brace depth; pop impl contexts when their block closes.
        for ch in line.chars() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if let Some((_, open_depth)) = impl_stack.last() {
                        if depth <= *open_depth {
                            impl_stack.pop();
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if classes.is_empty() && free.is_empty() {
        return None;
    }
    Some(FileStructure {
        module: module_of(rel_path),
        path: rel_path.to_string(),
        classes: classes
            .into_values()
            .filter(|c| !c.methods.is_empty() || c.kind != "impl")
            .collect(),
        free_functions: free,
    })
}
