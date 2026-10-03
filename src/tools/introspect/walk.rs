//! The one source-file walk behind every introspection tool
//! (`code_introspect`, `code_query`, `code_plan`).
//!
//! Review 2026-09-27 (on v0.9.4): the skip list lived inside each walker's
//! recursive helper only, so `code_introspect {"target":"."}` sent the
//! top-level `node_modules`/`.venv`/`target` straight into the deep walk and
//! — with path-sorted output — those files consumed the budget first. The
//! three walkers also each had their own list (`code_query`'s lacked
//! `.venv`), and all of them followed directory symlinks, so an in-workspace
//! `link -> ..` recursed until ELOOP failed the tool. This module is the
//! single walk:
//!
//! - one skip predicate ([`is_skipped_dir`]) for every directory at every
//!   level, the target's direct children included — the repository
//!   inventory's exclusions plus `scratchpad` and any Python virtualenv
//!   (a directory holding `pyvenv.cfg`, whatever its name);
//! - no symlink traversal: symlinked files and directories never become
//!   implicit source inputs;
//! - `.gitignore` respected through the repository inventory's
//!   `git ls-files` listing when the target is inside a git work tree.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::config::SafetyConfig;
use crate::tools::file::validate_tool_path;

/// Directory names no introspection walk descends into: build output,
/// dependency trees, VCS metadata, caches, virtualenvs — the repository
/// inventory's list (`evolve::graph`) plus `scratchpad` (agent working
/// copies would duplicate the tree).
pub(crate) fn is_skipped_dir(name: &str, path: &Path) -> bool {
    name == "scratchpad"
        || crate::evolve::graph::is_excluded_repository_directory(std::ffi::OsStr::new(name))
        || path.join("pyvenv.cfg").is_file()
}

/// What a walk found.
#[derive(Debug, Default)]
pub(crate) struct SourceWalk {
    /// Code files (by the inventory's language table), sorted.
    pub files: Vec<PathBuf>,
    /// Directories below the depth bound that were not entered.
    pub dirs_not_walked: usize,
    /// Whether the `git ls-files` listing filtered the result.
    pub gitignore_applied: bool,
}

/// Walk `target` for code files.
///
/// `safety`: when given, every file the caller will read and every
/// directory before descent is validated against the workspace path policy
/// (a refusal fails the walk, as before). `max_depth` bounds directory
/// nesting below `target`; directories it cuts off are counted.
pub(crate) fn source_files(
    target: &Path,
    safety: Option<&SafetyConfig>,
    max_depth: usize,
) -> Result<SourceWalk> {
    let mut walk = SourceWalk::default();
    let is_code = super::CodeIntrospect::is_source_file;
    let Ok(target_metadata) = std::fs::symlink_metadata(target) else {
        return Ok(walk);
    };
    if target_metadata.file_type().is_symlink() {
        if target.exists() {
            if let Some(safety) = safety {
                validate_tool_path(&target.to_string_lossy(), safety)?;
            }
        }
        return Ok(walk);
    }
    if target_metadata.is_file() {
        if is_code(target) {
            if let Some(safety) = safety {
                validate_tool_path(&target.to_string_lossy(), safety)?;
            }
            walk.files.push(target.to_path_buf());
        }
        return Ok(walk);
    }
    if !target_metadata.is_dir() {
        return Ok(walk);
    }

    // Explicit stack: (directory, its depth below target).
    let mut stack = vec![(target.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(&dir)?
            .filter_map(|entry| entry.ok())
            .collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                // Preserve the established fail-closed path-policy behavior
                // for a live link that escapes the workspace, but never read
                // or descend through any symlink (including an allowed one).
                if path.exists() {
                    if let Some(safety) = safety {
                        validate_tool_path(&path.to_string_lossy(), safety)?;
                    }
                }
                continue;
            }
            if file_type.is_file() {
                if is_code(&path) {
                    if let Some(safety) = safety {
                        validate_tool_path(&path.to_string_lossy(), safety)?;
                    }
                    walk.files.push(path);
                }
            } else if file_type.is_dir() {
                if is_skipped_dir(&name, &path) {
                    continue;
                }
                if let Some(safety) = safety {
                    validate_tool_path(&path.to_string_lossy(), safety)?;
                }
                if depth + 1 > max_depth {
                    walk.dirs_not_walked += 1;
                    continue;
                }
                stack.push((path, depth + 1));
            }
        }
    }

    // .gitignore: keep only what git would show. A target whose every file
    // is ignored (the caller pointed at an ignored tree on purpose) keeps
    // the walk's own result rather than coming back empty.
    // NOTE(fixer S): single `git ls-files` spawn, via the inventory helper
    // (sanitized env); route through the shared git-spawn helper.
    if !walk.files.is_empty() {
        if let Some(visible) = crate::analysis::repo_inventory::git_visible_files(target) {
            let kept: Vec<PathBuf> = walk
                .files
                .iter()
                .filter(|p| {
                    p.strip_prefix(target)
                        .map(|rel| visible.contains(&slash(rel)))
                        .unwrap_or(true)
                })
                .cloned()
                .collect();
            if !kept.is_empty() {
                walk.files = kept;
                walk.gitignore_applied = true;
            }
        }
    }
    walk.files.sort();
    Ok(walk)
}

fn slash(rel: &Path) -> String {
    rel.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}
