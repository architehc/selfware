//! Theseus Workspace Scaffolding (Environment-Data-Model Co-Evolution)
//!
//! Synthesizes and injects structured workspace grounding artifacts into shadow
//! worktrees before autonomous agent execution. Prevents hallucinated APIs,
//! stale assumptions, and redundant exploration by providing:
//! 1. **Collection Map**: Module hierarchy, dependencies, and public AST symbol index.
//! 2. **Event Log**: Recent commits, git diff summaries, and environmental events.
//! 3. **Executive Guide (`.theseus.md`)**: Top-level orientation artifact for agent context.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

use crate::evolve::module_graph::from_lib_rs;
use crate::evolve::symbols::extract_pub_symbols;

/// AST symbol entry in the Collection Map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolEntry {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line_range: (usize, usize),
    pub tokens: usize,
}

/// Structured Collection Map indexing the codebase architecture and API surface.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionMap {
    pub modules: Vec<String>,
    pub reexports: Vec<String>,
    pub symbols: Vec<SymbolEntry>,
}

impl CollectionMap {
    pub fn to_markdown(&self) -> String {
        let mut md = String::from("# Theseus Collection Map: Codebase Architecture\n\n");
        md.push_str("## Top-Level Modules\n");
        for m in &self.modules {
            md.push_str(&format!("- `{}`\n", m));
        }

        if !self.reexports.is_empty() {
            md.push_str("\n## Key Re-Exports\n");
            for r in &self.reexports {
                md.push_str(&format!("- `{}`\n", r));
            }
        }

        md.push_str("\n## Discovered Public Symbols (AST Index)\n");
        for s in &self.symbols {
            md.push_str(&format!(
                "- `{}` ({}) in `{}:{}-{}` (~{} tokens)\n",
                s.name, s.kind, s.file, s.line_range.0, s.line_range.1, s.tokens
            ));
        }

        md
    }
}

/// A recent event or commit record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEntry {
    pub timestamp: String,
    pub kind: String,
    pub summary: String,
}

/// Structured Event Log tracking recent change history and environment state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventLog {
    pub events: Vec<EventEntry>,
}

impl EventLog {
    pub fn to_markdown(&self) -> String {
        let mut md = String::from("# Theseus Event Log: Recent Changes & Environment State\n\n");
        if self.events.is_empty() {
            md.push_str("No recent historical events recorded.\n");
        } else {
            for e in &self.events {
                md.push_str(&format!(
                    "- **[{}]** ({}) {}\n",
                    e.timestamp, e.kind, e.summary
                ));
            }
        }
        md
    }
}

/// Outcome report of the workspace scaffolding process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaffoldReport {
    pub modules_mapped: usize,
    pub symbols_extracted: usize,
    pub events_recorded: usize,
    pub scaffold_path: PathBuf,
}

/// Theseus Workspace Scaffolder
pub struct TheseusScaffold;

impl TheseusScaffold {
    /// Generates the Collection Map from `root`.
    pub fn generate_collection_map(root: &Path) -> Result<CollectionMap> {
        let mut map = CollectionMap::default();

        // 1. Module graph from lib.rs (if available)
        if root.join("src/lib.rs").exists() {
            if let Ok(manifest) = from_lib_rs(root) {
                map.modules = manifest.modules.into_iter().map(|m| m.name).collect();
                map.reexports = manifest.reexports.into_iter().map(|r| r.path).collect();
            }
        }

        // 2. Discover symbols across src/*.rs and key subdirectories
        let src_dir = root.join("src");
        if src_dir.exists() {
            let mut rs_files = Vec::new();
            collect_rs_files(&src_dir, &mut rs_files, 3);

            for file_path in rs_files {
                if let Ok(content) = fs::read_to_string(&file_path) {
                    let symbols = extract_pub_symbols(&content);
                    let rel_path = file_path
                        .strip_prefix(root)
                        .unwrap_or(&file_path)
                        .to_string_lossy()
                        .to_string();

                    for sym in symbols {
                        map.symbols.push(SymbolEntry {
                            name: sym.name,
                            kind: sym.kind.to_string(),
                            file: rel_path.clone(),
                            line_range: sym.line_range,
                            tokens: sym.tokens,
                        });
                    }
                }
            }
        }

        map.symbols.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(map)
    }

    /// Generates the Event Log from git history.
    pub fn generate_event_log(root: &Path, max_commits: usize) -> Result<EventLog> {
        let mut log = EventLog::default();

        // Attempt git log execution
        let output = std::process::Command::new("git")
            .args([
                "log",
                &format!("-n{}", max_commits),
                "--pretty=format:%h|%an|%ad|%s",
                "--date=iso",
            ])
            .current_dir(root)
            .output();

        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split('|').collect();
                    if parts.len() >= 4 {
                        log.events.push(EventEntry {
                            timestamp: parts[2].trim().to_string(),
                            kind: "git_commit".to_string(),
                            summary: format!("[{}] {}: {}", parts[0], parts[1], parts[3]),
                        });
                    }
                }
            }
        }

        if log.events.is_empty() {
            log.events.push(EventEntry {
                timestamp: Utc::now().to_rfc3339(),
                kind: "workspace_init".to_string(),
                summary: "Initialized Theseus workspace scaffolding session".to_string(),
            });
        }

        Ok(log)
    }

    /// Injects Theseus artifacts into the target shadow worktree.
    pub fn scaffold_shadow_worktree(
        shadow_root: &Path,
        base_root: &Path,
    ) -> Result<ScaffoldReport> {
        let theseus_dir = shadow_root.join(".selfware/theseus");
        if let Ok(meta) = theseus_dir.symlink_metadata() {
            if meta.file_type().is_symlink() {
                anyhow::bail!(
                    "Theseus destination directory is a symlink: {:?}",
                    theseus_dir
                );
            }
        }
        fs::create_dir_all(&theseus_dir)?;

        // 1. Generate and write Collection Map
        let collection_map = Self::generate_collection_map(base_root)
            .context("generating Theseus Collection Map")?;
        let col_map_md = collection_map.to_markdown();
        let col_map_path = theseus_dir.join("collection_map.md");
        if let Ok(meta) = col_map_path.symlink_metadata() {
            if meta.file_type().is_symlink() {
                anyhow::bail!(
                    "Collection map destination is a symlink: {:?}",
                    col_map_path
                );
            }
        }
        fs::write(&col_map_path, &col_map_md)?;

        // 2. Generate and write Event Log
        let event_log =
            Self::generate_event_log(base_root, 10).context("generating Theseus Event Log")?;
        let event_log_md = event_log.to_markdown();
        let event_log_path = theseus_dir.join("event_log.md");
        if let Ok(meta) = event_log_path.symlink_metadata() {
            if meta.file_type().is_symlink() {
                anyhow::bail!("Event log destination is a symlink: {:?}", event_log_path);
            }
        }
        fs::write(&event_log_path, &event_log_md)?;

        // 3. Write top-level Executive Grounding Guide (.theseus.md)
        let guide_content = format!(
            "# Theseus Environment Guide\n\n\
             Autonomous workspace grounding generated at `{}`.\n\n\
             ## Architecture Summary\n\
             - **Modules**: {} declared\n\
             - **Symbols**: {} public symbols indexed\n\
             - **Recent Commits**: {} events logged\n\n\
             ## Workspace Rules & Working Agreements\n\
             - All edits must pass `cargo check` and adhere to repository invariant gates.\n\
             - See `.selfware/theseus/collection_map.md` for symbol signatures and file locations.\n\
             - See `.selfware/theseus/event_log.md` for recent change context.\n",
            Utc::now().to_rfc3339(),
            collection_map.modules.len(),
            collection_map.symbols.len(),
            event_log.events.len(),
        );
        let root_guide_path = shadow_root.join(".theseus.md");
        if let Ok(meta) = root_guide_path.symlink_metadata() {
            if meta.file_type().is_symlink() {
                anyhow::bail!(
                    "Executive guide destination is a symlink: {:?}",
                    root_guide_path
                );
            }
        }
        fs::write(&root_guide_path, guide_content)?;

        info!(
            "Theseus scaffold injected into {:?}: {} modules, {} symbols",
            shadow_root,
            collection_map.modules.len(),
            collection_map.symbols.len()
        );

        Ok(ScaffoldReport {
            modules_mapped: collection_map.modules.len(),
            symbols_extracted: collection_map.symbols.len(),
            events_recorded: event_log.events.len(),
            scaffold_path: root_guide_path,
        })
    }
}

fn collect_rs_files(dir: &Path, rs_files: &mut Vec<PathBuf>, depth_left: usize) {
    if depth_left == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with('.') && name != "target" {
                collect_rs_files(&path, rs_files, depth_left - 1);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            rs_files.push(path);
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/theseus_scaffold_test.rs"]
mod tests;
