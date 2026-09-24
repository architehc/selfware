//! Rust edition resolution for direct `rustfmt` invocations.
//!
//! `rustfmt` run on a file (not through `cargo fmt`) does NOT read
//! `Cargo.toml`: without `--edition` it parses as Rust 2015 and rejects
//! valid modern code (`async fn` → E0670 "async fn is not permitted in Rust
//! 2015"). The per-edit syntax check classified that as a syntax failure and
//! failed verification on code that `cargo test`/`cargo clippy` accepted.
//!
//! Every place selfware runs `rustfmt` directly must pass the edition the
//! crate actually uses; this module is the single resolver for that.
//!
//! Resolution walks the ancestors of the file, nearest first. In each
//! directory:
//! 1. `rustfmt.toml` / `.rustfmt.toml` with an `edition` key wins (that is the
//!    edition rustfmt itself would pick up for this file);
//! 2. otherwise a `Cargo.toml` decides: `package.edition`, or
//!    `workspace.package.edition` of the workspace root when the package says
//!    `edition.workspace = true`, or `workspace.package.edition` of a virtual
//!    manifest.
//!
//! A formatter config ABOVE the owning `Cargo.toml` loses to the manifest,
//! matching `cargo fmt`, which passes the manifest edition on the command line
//! (CLI beats config).
//!
//! When no edition can be determined the fallback is [`FALLBACK_EDITION`]
//! (2021), not cargo's own default of 2015. Rationale: this is a SYNTAX gate,
//! and its false failures block verification. A crate without an `edition`
//! key is rare in modern Rust (`cargo new` always writes one), and loose `.rs`
//! files outside any crate are overwhelmingly modern code; parsing them as 2015
//! rejects `async fn`/`dyn`-era syntax, while parsing as 2021 only misjudges
//! the rare 2015 code that uses `async`/`await`/`dyn` as identifiers. The
//! resolution records WHY the fallback was used so callers can report it
//! (Rule 3: say what was actually checked).

use std::path::{Path, PathBuf};

/// Edition used when none can be determined (see module docs for why this is
/// 2021 rather than cargo's 2015 default).
pub const FALLBACK_EDITION: &str = "2021";

/// Where a resolved edition came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditionSource {
    /// `edition` key in a `rustfmt.toml` / `.rustfmt.toml`.
    RustfmtConfig(PathBuf),
    /// `package.edition` in the owning `Cargo.toml`.
    CargoPackage(PathBuf),
    /// `workspace.package.edition` in a workspace root `Cargo.toml`
    /// (inherited via `edition.workspace = true`, or a virtual manifest).
    CargoWorkspace(PathBuf),
    /// Nothing usable was found; the string says why.
    Fallback(String),
}

/// A resolved edition plus its provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEdition {
    pub edition: String,
    pub source: EditionSource,
}

impl ResolvedEdition {
    fn fallback(reason: impl Into<String>) -> Self {
        Self {
            edition: FALLBACK_EDITION.to_string(),
            source: EditionSource::Fallback(reason.into()),
        }
    }

    /// True when the edition was guessed rather than read from a config.
    pub fn is_fallback(&self) -> bool {
        matches!(self.source, EditionSource::Fallback(_))
    }

    /// Short human description, e.g. `edition 2021 (Cargo.toml package.edition)`.
    pub fn describe(&self) -> String {
        match &self.source {
            EditionSource::RustfmtConfig(p) => {
                format!("edition {} (from {})", self.edition, p.display())
            }
            EditionSource::CargoPackage(p) => {
                format!(
                    "edition {} (package.edition in {})",
                    self.edition,
                    p.display()
                )
            }
            EditionSource::CargoWorkspace(p) => format!(
                "edition {} (workspace.package.edition in {})",
                self.edition,
                p.display()
            ),
            EditionSource::Fallback(why) => {
                format!("edition {} (fallback: {})", self.edition, why)
            }
        }
    }
}

/// Accept only edition-shaped values (`20xx`) so a malformed config cannot
/// inject arbitrary rustfmt arguments or produce a bogus `--edition` value.
fn valid_edition(s: &str) -> Option<String> {
    let s = s.trim();
    (s.len() == 4 && s.starts_with("20") && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.to_string())
}

fn read_toml(path: &Path) -> Option<toml::Value> {
    let text = std::fs::read_to_string(path).ok()?;
    toml::from_str::<toml::Value>(&text).ok()
}

fn rustfmt_config_edition(dir: &Path) -> Option<ResolvedEdition> {
    for name in ["rustfmt.toml", ".rustfmt.toml"] {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let edition = read_toml(&path)
            .as_ref()
            .and_then(|v| v.get("edition"))
            .and_then(|e| e.as_str())
            .and_then(valid_edition);
        if let Some(edition) = edition {
            return Some(ResolvedEdition {
                edition,
                source: EditionSource::RustfmtConfig(path),
            });
        }
    }
    None
}

fn workspace_package_edition(manifest: &toml::Value) -> Option<String> {
    manifest
        .get("workspace")?
        .get("package")?
        .get("edition")?
        .as_str()
        .and_then(valid_edition)
}

/// Find the workspace root manifest for a package manifest in `pkg_dir`:
/// an explicit `package.workspace = "<path>"`, else the nearest ancestor
/// `Cargo.toml` with a `[workspace]` table.
fn find_workspace_manifest(pkg_dir: &Path, pkg: &toml::Value) -> Option<(PathBuf, toml::Value)> {
    if let Some(rel) = pkg
        .get("package")
        .and_then(|p| p.get("workspace"))
        .and_then(|w| w.as_str())
    {
        let path = pkg_dir.join(rel).join("Cargo.toml");
        let value = read_toml(&path)?;
        return Some((path, value));
    }
    for dir in pkg_dir.ancestors().skip(1) {
        let path = dir.join("Cargo.toml");
        if !path.is_file() {
            continue;
        }
        if let Some(value) = read_toml(&path) {
            if value.get("workspace").is_some() {
                return Some((path, value));
            }
        }
    }
    None
}

fn cargo_manifest_edition(dir: &Path) -> Option<ResolvedEdition> {
    let path = dir.join("Cargo.toml");
    if !path.is_file() {
        return None;
    }
    let Some(manifest) = read_toml(&path) else {
        return Some(ResolvedEdition::fallback(format!(
            "{} could not be parsed",
            path.display()
        )));
    };

    if let Some(package) = manifest.get("package") {
        match package.get("edition") {
            Some(toml::Value::String(s)) => {
                return Some(match valid_edition(s) {
                    Some(edition) => ResolvedEdition {
                        edition,
                        source: EditionSource::CargoPackage(path),
                    },
                    None => ResolvedEdition::fallback(format!(
                        "unrecognized package.edition {s:?} in {}",
                        path.display()
                    )),
                });
            }
            Some(toml::Value::Table(t))
                if t.get("workspace").and_then(|w| w.as_bool()) == Some(true) =>
            {
                // `edition.workspace = true`: inherit from the workspace root.
                if let Some((ws_path, ws)) = find_workspace_manifest(dir, &manifest) {
                    if let Some(edition) = workspace_package_edition(&ws) {
                        return Some(ResolvedEdition {
                            edition,
                            source: EditionSource::CargoWorkspace(ws_path),
                        });
                    }
                    return Some(ResolvedEdition::fallback(format!(
                        "{} inherits the edition but {} has no workspace.package.edition",
                        path.display(),
                        ws_path.display()
                    )));
                }
                return Some(ResolvedEdition::fallback(format!(
                    "{} inherits the edition but no workspace root was found",
                    path.display()
                )));
            }
            _ => {
                return Some(ResolvedEdition::fallback(format!(
                    "{} has no package.edition",
                    path.display()
                )));
            }
        }
    }

    // Virtual manifest (workspace only): a file under it but outside any
    // member package. Use the shared edition if one is declared.
    if manifest.get("workspace").is_some() {
        return Some(match workspace_package_edition(&manifest) {
            Some(edition) => ResolvedEdition {
                edition,
                source: EditionSource::CargoWorkspace(path),
            },
            None => ResolvedEdition::fallback(format!(
                "virtual manifest {} has no workspace.package.edition",
                path.display()
            )),
        });
    }

    // A Cargo.toml with neither [package] nor [workspace] is not a manifest
    // that owns this file; keep walking.
    None
}

/// Resolve the Rust edition that applies to `file` (a path to a `.rs` file,
/// or a directory to resolve from). Never fails: an undeterminable edition
/// yields [`FALLBACK_EDITION`] with the reason recorded.
pub fn resolve_rust_edition(file: &Path) -> ResolvedEdition {
    let start = if file.is_dir() {
        Some(file)
    } else {
        file.parent()
    };
    let Some(start) = start else {
        return ResolvedEdition::fallback("file has no parent directory");
    };
    for dir in start.ancestors() {
        if let Some(r) = rustfmt_config_edition(dir) {
            return r;
        }
        if let Some(r) = cargo_manifest_edition(dir) {
            return r;
        }
    }
    ResolvedEdition::fallback("no Cargo.toml or rustfmt.toml above the file")
}

/// Group files by their resolved edition, preserving first-seen order, so a
/// multi-crate edit runs one rustfmt invocation per edition.
pub fn group_by_edition(files: &[PathBuf]) -> Vec<(ResolvedEdition, Vec<PathBuf>)> {
    let mut groups: Vec<(ResolvedEdition, Vec<PathBuf>)> = Vec::new();
    for f in files {
        let resolved = resolve_rust_edition(f);
        match groups
            .iter_mut()
            .find(|(r, _)| r.edition == resolved.edition)
        {
            Some((_, v)) => v.push(f.clone()),
            None => groups.push((resolved, vec![f.clone()])),
        }
    }
    groups
}

/// Arguments for a parse-only `rustfmt --check` of individual files:
/// the resolved edition, and `skip_children` so each file is checked alone
/// (without it rustfmt follows `mod foo;` declarations and fails the edited
/// file on an unrelated child module's error or a not-yet-created module
/// file). On toolchains where `skip_children` is still unstable rustfmt prints
/// a warning and checks children as before — never a false pass.
pub fn rustfmt_check_args(edition: &str) -> Vec<String> {
    vec![
        "--edition".to_string(),
        edition.to_string(),
        "--check".to_string(),
        "--config".to_string(),
        "skip_children=true".to_string(),
    ]
}

#[cfg(test)]
#[path = "../../tests/unit/testing/rust_edition/rust_edition_test.rs"]
mod tests;
