//! Read-through access to the `expansion_recommendation/` catalog served to
//! the touch canvas. The schema is owned by the UI, so files are kept as
//! parsed `serde_json::Value` with no typed mirror. The catalog is an
//! optional feature: a missing directory degrades to an empty index and
//! typed 404s, never a 500. Files are re-read per request (580 examples ≈
//! 1.5 MB total), which keeps edits hot without file watchers.

use serde_json::{json, Value};
use std::path::Path;

const CATALOG_DIR: &str = "expansion_recommendation";

/// Shape returned when the catalog directory is absent (optional feature).
fn empty_index() -> Value {
    json!({
        "components": [],
        "counts": {},
        "total_examples": 0,
    })
}

/// Catalog document names become filesystem paths, so only the naming
/// scheme the generator uses (lowercase snake_case) is accepted — this
/// rejects `..` and friends before any path join.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn read_document(project_root: &Path, stem: &str) -> Option<Value> {
    if !valid_name(stem) {
        return None;
    }
    let path = project_root.join(CATALOG_DIR).join(format!("{stem}.json"));
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// The catalog index (component list, per-component counts, total), or the
/// empty-catalog shape when the directory does not exist.
pub fn index(project_root: &Path) -> Value {
    read_document(project_root, "index").unwrap_or_else(empty_index)
}

/// One full component document (20 examples plus metadata), if present.
pub fn component(project_root: &Path, component: &str) -> Option<Value> {
    read_document(project_root, component)
}

/// One example by id within a component document, if both exist.
pub fn example(project_root: &Path, component: &str, example_id: &str) -> Option<Value> {
    let document = self::component(project_root, component)?;
    document
        .get("examples")?
        .as_array()?
        .iter()
        .find(|example| example.get("id").and_then(Value::as_str) == Some(example_id))
        .cloned()
}
