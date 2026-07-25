//! Module graph seeded from `src/lib.rs`.
//!
//! Instead of a node per file (thousands), start from the crate's own top-level
//! module declarations — exactly what `lib.rs` exposes — as the initial nodes.
//! Each node records its visibility, the section it lives under, and any `#[cfg]`
//! feature gate; re-exports become edges. This is the canonical, navigable entry
//! graph a reader actually reasons about.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

/// One `mod` declaration from `lib.rs`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ModuleDecl {
    pub name: String,
    /// "pub" or "pub(crate)".
    pub visibility: String,
    /// Section header the declaration falls under (core / domain / feature / utility).
    pub category: String,
    /// `#[cfg(feature = "...")]` gate, if any.
    pub cfg_feature: Option<String>,
    /// Declared under `#[cfg(test)]`.
    pub test_only: bool,
}

/// A `pub use path::to::item;` re-export from `lib.rs`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReExport {
    /// The owning top-level module (first path segment), e.g. `safety`.
    pub module: String,
    /// The full re-exported path, e.g. `safety::redact`.
    pub path: String,
}

/// The parsed shape of `lib.rs`: declared modules and their re-exports.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ModuleManifest {
    pub modules: Vec<ModuleDecl>,
    pub reexports: Vec<ReExport>,
}

/// Parse `src/lib.rs` under `root` into its module manifest.
pub fn from_lib_rs(root: &Path) -> Result<ModuleManifest> {
    let lib = root.join("src").join("lib.rs");
    let source =
        std::fs::read_to_string(&lib).with_context(|| format!("reading {}", lib.display()))?;
    Ok(parse(&source))
}

/// Parse crate-root source into a module manifest. Pure — no filesystem access,
/// so it is directly unit-testable.
pub fn parse(source: &str) -> ModuleManifest {
    let mut modules = Vec::new();
    let mut reexports = Vec::new();
    let mut category = "core";
    let mut pending_cfg_feature: Option<String> = None;
    let mut pending_test_only = false;

    for raw in source.lines() {
        let line = raw.trim();

        // Section headers move the current category. They appear inside the
        // `// ===` banner comments as a title line.
        if line.starts_with("//") {
            let lower = line.to_ascii_lowercase();
            if lower.contains("domain modules") {
                category = "domain";
            } else if lower.contains("new feature modules") {
                category = "feature";
            } else if lower.contains("utility modules") {
                category = "utility";
            }
            continue;
        }

        // Attributes preceding a declaration carry cfg/test context.
        if line.starts_with("#[") {
            if line.contains("cfg(test)") || line.contains("cfg(all(test") {
                pending_test_only = true;
            }
            if let Some(feature) = cfg_feature(line) {
                pending_cfg_feature = Some(feature);
            }
            continue;
        }

        if let Some(decl) = mod_decl(line) {
            modules.push(ModuleDecl {
                name: decl.0,
                visibility: decl.1,
                category: category.to_string(),
                cfg_feature: pending_cfg_feature.take(),
                test_only: pending_test_only,
            });
            pending_test_only = false;
            continue;
        }

        if let Some(path) = pub_use_path(line) {
            let module = path.split("::").next().unwrap_or(&path).to_string();
            reexports.push(ReExport { module, path });
        }

        // Any other non-attribute, non-doc line clears pending attribute context.
        if !line.is_empty() {
            pending_cfg_feature = None;
            pending_test_only = false;
        }
    }

    ModuleManifest { modules, reexports }
}

/// Extract a feature name from `#[cfg(feature = "name")]`.
fn cfg_feature(line: &str) -> Option<String> {
    let idx = line.find("feature")?;
    let rest = &line[idx..];
    let start = rest.find('"')?;
    let after = &rest[start + 1..];
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

/// Parse `pub mod name;` / `pub(crate) mod name;` into (name, visibility).
fn mod_decl(line: &str) -> Option<(String, String)> {
    let (visibility, rest) = if let Some(r) = line.strip_prefix("pub(crate) mod ") {
        ("pub(crate)", r)
    } else if let Some(r) = line.strip_prefix("pub mod ") {
        ("pub", r)
    } else {
        return None;
    };
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then(|| (name, visibility.to_string()))
}

/// Parse the path out of `pub use path::to::item;`.
fn pub_use_path(line: &str) -> Option<String> {
    let rest = line.strip_prefix("pub use ")?;
    let path: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
        .collect();
    (!path.is_empty() && path.contains("::")).then_some(path)
}

/// Resolve a module name to its source file relative to the crate root:
/// `src/<name>.rs` if present, otherwise `src/<name>/mod.rs`.
pub fn module_path(root: &Path, name: &str) -> Option<String> {
    let flat = format!("src/{name}.rs");
    if root.join(&flat).is_file() {
        return Some(flat);
    }
    let nested = format!("src/{name}/mod.rs");
    if root.join(&nested).is_file() {
        return Some(nested);
    }
    None
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/module_graph_test.rs"]
mod module_graph_test;
