use axum::{
    body::Body,
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::Response,
    routing::get,
    Router,
};
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

pub fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("md") => "text/markdown; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    }
}

pub fn resolve(lab_dir: &Path, url_path: &str) -> Option<PathBuf> {
    let rel = url_path.trim_start_matches('/');
    let rel = if rel.is_empty() {
        "web/index.html".to_string()
    } else {
        rel.to_string()
    };
    let mut p = lab_dir.to_path_buf();
    for comp in Path::new(&rel).components() {
        match comp {
            std::path::Component::Normal(c) => p.push(c),
            _ => return None, // rejects "..", root, prefix
        }
    }
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}

async fn serve_file(State(lab): State<PathBuf>, AxumPath(path): AxumPath<String>) -> Response {
    match resolve(&lab, &path).and_then(|p| std::fs::read(&p).ok().map(|b| (p, b))) {
        Some((p, bytes)) => Response::builder()
            .header("content-type", mime_for(p.to_str().unwrap_or("")))
            .body(Body::from(bytes))
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("not found"))
            .unwrap(),
    }
}

pub fn build_router(lab: PathBuf) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async { axum::response::Redirect::temporary("/web/index.html") }),
        )
        .route("/*path", get(serve_file))
        .with_state(lab)
}

#[tokio::main]
async fn main() {
    let lab = lab_dir();
    let manifest_text = match std::fs::read_to_string(lab.join("curriculum.json")) {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "tqec-lab: cannot read {}: {e}",
                lab.join("curriculum.json").display()
            );
            std::process::exit(1);
        }
    };
    let manifest: Manifest = match serde_json::from_str(&manifest_text) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("tqec-lab: curriculum.json is malformed: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = validate_manifest(&manifest, &lab) {
        eprintln!("tqec-lab: invalid curriculum: {e}");
        std::process::exit(1);
    }
    let port: u16 = std::env::var("TQEC_LAB_PORT")
        .ok()
        .map(|s| {
            s.parse().unwrap_or_else(|_| {
                eprintln!("tqec-lab: warning: TQEC_LAB_PORT={s:?} is not a valid port, using 7837");
                7837
            })
        })
        .unwrap_or(7837);
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("tqec-lab: cannot bind {addr}: {e} (set TQEC_LAB_PORT to change port)");
            std::process::exit(1);
        }
    };
    println!("tqec-lab: {} nodes validated", manifest.nodes.len());
    println!("tqec-lab: open http://127.0.0.1:{port}/");
    if let Err(e) = axum::serve(listener, build_router(lab)).await {
        eprintln!("tqec-lab: server error: {e}");
        std::process::exit(1);
    }
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

    #[test]
    fn mime_mapping() {
        assert_eq!(mime_for("web/app.js"), "text/javascript; charset=utf-8");
        assert_eq!(mime_for("curriculum.json"), "application/json");
        assert_eq!(mime_for("lessons/a.md"), "text/markdown; charset=utf-8");
        assert_eq!(mime_for("web/index.html"), "text/html; charset=utf-8");
        assert_eq!(mime_for("x.bin"), "application/octet-stream");
    }

    #[test]
    fn resolve_rejects_traversal() {
        let dir = Path::new("/tmp/whatever");
        assert!(resolve(dir, "/../Cargo.toml").is_none());
        assert!(resolve(dir, "/lessons/../../etc/passwd").is_none());
    }

    #[test]
    fn resolve_maps_root_to_index() {
        let dir = std::env::temp_dir().join("tqec_lab_test_resolve");
        std::fs::create_dir_all(dir.join("web")).unwrap();
        std::fs::write(dir.join("web/index.html"), "<html></html>").unwrap();
        assert_eq!(resolve(&dir, "/"), Some(dir.join("web/index.html")));
        assert_eq!(resolve(&dir, ""), Some(dir.join("web/index.html")));
        assert!(resolve(&dir, "/missing.js").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
