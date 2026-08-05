use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub nodes: Vec<Node>,
}

#[derive(Debug, Deserialize)]
pub struct Node {
    pub id: String,
    pub title: String,
    pub tier: u32,
    #[serde(default)]
    pub prereqs: Vec<String>,
    pub lesson: String,
    pub widget: Option<String>,
    #[serde(default)]
    pub papers: Vec<String>,
}

pub const KNOWN_WIDGETS: [&str; 2] = ["lattice", "decoder"];

pub fn lab_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lab")
}

pub fn validate_manifest(m: &Manifest, lab_dir: &Path) -> Result<(), String> {
    let mut ids = HashSet::new();
    for n in &m.nodes {
        if !ids.insert(n.id.as_str()) {
            return Err(format!("duplicate node id '{}'", n.id));
        }
    }
    for n in &m.nodes {
        for p in &n.prereqs {
            if !ids.contains(p.as_str()) {
                return Err(format!("node '{}' has unknown prereq '{}'", n.id, p));
            }
        }
        let lesson_path = lab_dir.join("lessons").join(&n.lesson);
        if !lesson_path.is_file() {
            return Err(format!(
                "node '{}' references missing lesson '{}'",
                n.id, n.lesson
            ));
        }
        if let Some(w) = &n.widget {
            if !KNOWN_WIDGETS.contains(&w.as_str()) {
                return Err(format!("node '{}' references unknown widget '{}'", n.id, w));
            }
        }
    }
    // cycle check via DFS with colors
    fn visit<'a>(
        id: &'a str,
        nodes: &std::collections::HashMap<&'a str, &'a Node>,
        state: &mut std::collections::HashMap<&'a str, u8>,
    ) -> Result<(), String> {
        match state.get(id) {
            Some(1) => return Err(format!("prereq cycle involving '{}'", id)),
            Some(2) => return Ok(()),
            _ => {}
        }
        state.insert(id, 1);
        for p in &nodes[id].prereqs {
            visit(p, nodes, state)?;
        }
        state.insert(id, 2);
        Ok(())
    }
    let nodes: std::collections::HashMap<&str, &Node> =
        m.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut state = std::collections::HashMap::new();
    for n in &m.nodes {
        visit(&n.id, &nodes, &mut state)?;
    }
    Ok(())
}

fn main() {
    println!("tqec-lab scaffold");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, prereqs: &[&str], lesson: &str, widget: Option<&str>) -> Node {
        Node {
            id: id.into(),
            title: id.into(),
            tier: 0,
            prereqs: prereqs.iter().map(|s| s.to_string()).collect(),
            lesson: lesson.into(),
            widget: widget.map(|s| s.to_string()),
            papers: vec![],
        }
    }

    #[test]
    fn rejects_unknown_prereq() {
        let m = Manifest {
            nodes: vec![node("a", &["ghost"], "a.md", None)],
        };
        let dir = Path::new("/nonexistent");
        let err = validate_manifest(&m, dir).unwrap_err();
        assert!(err.contains("unknown prereq"), "got: {err}");
    }

    #[test]
    fn rejects_cycle() {
        let m = Manifest {
            nodes: vec![
                node("a", &["b"], "a.md", None),
                node("b", &["a"], "b.md", None),
            ],
        };
        // lesson files won't exist, so use a dir where they do: create temp files
        let dir = std::env::temp_dir().join("tqec_lab_test_cycle");
        std::fs::create_dir_all(dir.join("lessons")).unwrap();
        std::fs::write(dir.join("lessons/a.md"), "x").unwrap();
        std::fs::write(dir.join("lessons/b.md"), "x").unwrap();
        let err = validate_manifest(&m, &dir).unwrap_err();
        assert!(err.contains("cycle"), "got: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_unknown_widget() {
        let dir = std::env::temp_dir().join("tqec_lab_test_widget");
        std::fs::create_dir_all(dir.join("lessons")).unwrap();
        std::fs::write(dir.join("lessons/a.md"), "x").unwrap();
        let m = Manifest {
            nodes: vec![node("a", &[], "a.md", Some("hologram"))],
        };
        let err = validate_manifest(&m, &dir).unwrap_err();
        assert!(err.contains("unknown widget"), "got: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_missing_lesson() {
        let dir = std::env::temp_dir().join("tqec_lab_test_missing");
        std::fs::create_dir_all(dir.join("lessons")).unwrap();
        let m = Manifest {
            nodes: vec![node("a", &[], "nope.md", None)],
        };
        let err = validate_manifest(&m, &dir).unwrap_err();
        assert!(err.contains("missing lesson"), "got: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
