use std::collections::HashMap;

use selfware::evolve::envelope::build_envelope_with_root;
use selfware::evolve::{build_envelope, ContextMode, Graph, Node};

const FOO_RS: &str = r#"//! Foo module docs.
//! A second line of module documentation to pad the comment overhead.
//! A third line explaining the fixture's arithmetic helpers in detail.

use std::collections::HashMap;

/// Adds one.
pub fn add_one(x: usize) -> usize {
    // inner comment
    let mut acc = x + 1;
    // accumulate over a small range so the body has real weight
    for i in 0..x {
        acc += i * 2;
        acc %= 1024;
    }
    acc
}

/// Adds two with a match-heavy body.
pub fn add_two(x: usize) -> usize {
    // classify the input, then build a label string
    let label = match x {
        0 => "empty",
        1..=4 => "short",
        _ => "long",
    };
    let mut out = String::from(label);
    out.push_str("done");
    x + 2 + out.len()
}

/// Multiplies into a running accumulator.
pub fn mul_acc(x: usize, y: usize) -> usize {
    // running product with a modular cap
    let mut acc = 1usize;
    for i in 0..y {
        acc *= x + i;
        acc %= 4096;
    }
    acc
}

/// Builds a greeting string for the given name.
pub fn greet(name: &str) -> String {
    // pre-allocate, then extend
    let mut s = String::with_capacity(name.len() + 16);
    s.push_str("hello, ");
    s.push_str(name);
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_adds() {
        assert_eq!(super::add_one(1), 2);
        assert_eq!(super::add_two(1), 10);
    }

    #[test]
    fn it_greets() {
        assert!(super::greet("foo").contains("foo"));
    }
}
"#;

fn fixture() -> (Graph, impl Fn(&str) -> Option<String>) {
    let mut node = Node::code("crate::foo", "src/foo.rs");
    node.tokens = selfware::token_count::estimate_content_tokens(FOO_RS);
    let graph = Graph {
        nodes: vec![node],
        edges: vec![],
    };
    let mut sources = HashMap::new();
    sources.insert("src/foo.rs".to_string(), FOO_RS.to_string());
    let reader = move |rel: &str| sources.get(rel).cloned();
    (graph, reader)
}

/// The Map projection reads sources from disk via `build_map`, so give it a
/// real file tree: a tempdir root containing `src/foo.rs`.
fn disk_fixture() -> (tempfile::TempDir, Graph, impl Fn(&str) -> Option<String>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).expect("mkdir src");
    std::fs::write(src_dir.join("foo.rs"), FOO_RS).expect("write fixture");
    let (graph, reader) = fixture();
    (dir, graph, reader)
}

#[test]
fn lite_envelope_ships_signatures_not_bodies() {
    let (graph, reader) = fixture();
    let env = build_envelope(
        &graph,
        &ContextMode::Lite,
        &["crate::foo".to_string()],
        "rev1",
        reader,
    );
    assert_eq!(env.mode, ContextMode::Lite);
    assert!(env.documents[0].content.contains("pub fn add_one"));
    assert!(
        !env.documents[0].content.contains("x + 1"),
        "lite must not ship bodies"
    );
    assert!(env.total_tokens > 0 && env.total_tokens == env.documents[0].tokens);
}

#[test]
fn compact_envelope_strips_comments_and_tests() {
    let (graph, reader) = fixture();
    let env = build_envelope(
        &graph,
        &ContextMode::Compact,
        &["crate::foo".to_string()],
        "rev1",
        reader,
    );
    let content = &env.documents[0].content;
    assert!(!content.contains("//! Foo module docs"));
    assert!(!content.contains("inner comment"));
    assert!(
        !content.contains("it_adds"),
        "compact must drop cfg(test) blocks"
    );
    assert!(content.contains("pub fn add_one"));
}

#[test]
fn full_envelope_ships_verbatim_source_and_hash_is_deterministic() {
    let (graph, reader) = fixture();
    let a = build_envelope(
        &graph,
        &ContextMode::Full,
        &["crate::foo".to_string()],
        "rev1",
        &reader,
    );
    let b = build_envelope(
        &graph,
        &ContextMode::Full,
        &["crate::foo".to_string()],
        "rev1",
        &reader,
    );
    assert_eq!(a.documents[0].content, FOO_RS);
    assert_eq!(a.content_hash, b.content_hash);
    let c = build_envelope(
        &graph,
        &ContextMode::Full,
        &["crate::foo".to_string()],
        "rev2",
        &reader,
    );
    assert_ne!(
        a.content_hash, c.content_hash,
        "revision must feed the hash"
    );
}

#[test]
fn map_envelope_ships_card_text_not_source() {
    let (dir, graph, reader) = disk_fixture();
    let env = build_envelope_with_root(
        &graph,
        &ContextMode::Map,
        &["crate::foo".to_string()],
        "rev1",
        dir.path(),
        reader,
    );
    let content = &env.documents[0].content;
    assert!(
        content.contains("add_one"),
        "card lists public symbol names"
    );
    assert!(!content.contains("x + 1"), "card must not ship bodies");
}

#[test]
fn envelope_sizes_order_map_lite_compact_full() {
    let (dir, graph, reader) = disk_fixture();
    let size = |mode: ContextMode| {
        build_envelope_with_root(
            &graph,
            &mode,
            &["crate::foo".to_string()],
            "rev1",
            dir.path(),
            &reader,
        )
        .total_tokens
    };
    let (map, lite, compact, full) = (
        size(ContextMode::Map),
        size(ContextMode::Lite),
        size(ContextMode::Compact),
        size(ContextMode::Full),
    );
    assert!(map < lite, "{map} < {lite}");
    assert!(lite < compact, "{lite} < {compact}");
    assert!(compact < full, "{compact} < {full}");
}

#[test]
fn unknown_and_unreadable_nodes_are_skipped() {
    let (graph, reader) = fixture();
    let env = build_envelope(
        &graph,
        &ContextMode::Full,
        &["crate::foo".to_string(), "crate::ghost".to_string()],
        "rev1",
        reader,
    );
    assert_eq!(env.included.len(), 2, "included echoes the request");
    assert_eq!(env.documents.len(), 1, "unknown ids produce no document");
}

/// Custom is the user's hand-picked selection: production nodes for non-.rs
/// files are NodeLayer::Auxiliary (NodeLayer::Code only exists for
/// rust_source), so the Code-only layer filter used to drop a picked
/// README.md before the verbatim arm ever ran — the selection shipped
/// nothing. Custom now ships Auxiliary picks VERBATIM; Lite stays Code-only
/// (the signature tier). The TierMeasurer's signature fraction for such
/// nodes stays a budget estimate only.
#[test]
fn custom_ships_handpicked_auxiliary_files_verbatim_lite_stays_code_only() {
    const README_MD: &str = "# Project\n\nSome prose the model must see.\n";
    let mut node = Node::code("crate::foo", "src/foo.rs");
    node.tokens = selfware::token_count::estimate_content_tokens(FOO_RS);
    // Production shape: non-source files are Auxiliary, classified markup.
    let mut md_node = Node::auxiliary("crate::readme", "README.md", "markup");
    md_node.tokens = selfware::token_count::estimate_content_tokens(README_MD);
    let graph = Graph {
        nodes: vec![node, md_node],
        edges: vec![],
    };
    let mut sources = HashMap::new();
    sources.insert("src/foo.rs".to_string(), FOO_RS.to_string());
    sources.insert("README.md".to_string(), README_MD.to_string());
    let reader = move |rel: &str| sources.get(rel).cloned();

    // Custom ships what the user picked: the .md verbatim, the .rs skeleton.
    let env = build_envelope(
        &graph,
        &ContextMode::Custom,
        &["crate::foo".to_string(), "crate::readme".to_string()],
        "rev1",
        &reader,
    );
    let md_doc = env
        .documents
        .iter()
        .find(|d| d.path == "README.md")
        .expect("Custom must not drop a hand-picked Auxiliary node");
    assert_eq!(
        md_doc.content, README_MD,
        "Custom ships non-Rust content verbatim"
    );
    let rs_doc = env
        .documents
        .iter()
        .find(|d| d.path == "src/foo.rs")
        .expect(".rs file still present");
    assert!(
        !rs_doc.content.contains("x + 1"),
        "Custom still projects .rs files to skeletons"
    );

    // Lite stays the Code-only signature tier: the Auxiliary .md is out of
    // scope there, the .rs skeleton ships.
    let env = build_envelope(
        &graph,
        &ContextMode::Lite,
        &["crate::foo".to_string(), "crate::readme".to_string()],
        "rev1",
        &reader,
    );
    assert!(
        env.documents.iter().all(|d| d.path != "README.md"),
        "Lite is Code-only — Auxiliary nodes stay dropped"
    );
    let rs_doc = env
        .documents
        .iter()
        .find(|d| d.path == "src/foo.rs")
        .expect(".rs file still present in Lite");
    assert!(
        !rs_doc.content.contains("x + 1"),
        "Lite projects .rs files to skeletons"
    );
}
