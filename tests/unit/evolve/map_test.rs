//! Unit tests for the component-map context tier (`evolve::map`).

use super::*;
use crate::evolve::{Edge, Graph, Node};

fn sym_name(line: &str) -> Option<String> {
    public_symbol(line).map(|(_, name)| name)
}

#[test]
fn parses_each_public_item_kind() {
    assert_eq!(
        sym_name("pub fn run_task(&self) {").as_deref(),
        Some("run_task")
    );
    assert_eq!(sym_name("pub struct Agent {").as_deref(), Some("Agent"));
    assert_eq!(sym_name("pub struct Marker;").as_deref(), Some("Marker"));
    assert_eq!(sym_name("pub enum State {").as_deref(), Some("State"));
    assert_eq!(sym_name("pub trait Tool {").as_deref(), Some("Tool"));
    assert_eq!(sym_name("pub type Result = ();").as_deref(), Some("Result"));
    assert_eq!(
        sym_name("pub const MAX: usize = 4;").as_deref(),
        Some("MAX")
    );
    assert_eq!(
        sym_name("pub static FLAG: bool = true;").as_deref(),
        Some("FLAG")
    );
    assert_eq!(sym_name("pub union Raw {").as_deref(), Some("Raw"));
}

#[test]
fn parses_modifiers_and_generics() {
    assert_eq!(
        sym_name("pub async fn fetch<T>(x: T) {").as_deref(),
        Some("fetch")
    );
    assert_eq!(sym_name("pub unsafe fn poke() {").as_deref(), Some("poke"));
    assert_eq!(
        sym_name("pub const fn size() -> usize {").as_deref(),
        Some("size")
    );
    assert_eq!(
        sym_name("pub extern \"C\" fn c_abi() {").as_deref(),
        Some("c_abi")
    );
}

#[test]
fn classifies_symbol_kinds() {
    assert!(matches!(
        public_symbol("pub fn f() {"),
        Some((SymKind::Fn, _))
    ));
    assert!(matches!(
        public_symbol("pub struct S;"),
        Some((SymKind::Type, _))
    ));
    assert!(matches!(
        public_symbol("pub enum E {"),
        Some((SymKind::Type, _))
    ));
    assert!(matches!(
        public_symbol("pub trait T {"),
        Some((SymKind::Trait, _))
    ));
    assert!(matches!(
        public_symbol("pub const C: u8 = 0;"),
        Some((SymKind::Const, _))
    ));
}

#[test]
fn excludes_non_public_and_non_items() {
    assert_eq!(sym_name("pub(crate) fn internal() {"), None);
    assert_eq!(sym_name("fn private() {"), None);
    assert_eq!(sym_name("    let x = 3;"), None);
    assert_eq!(sym_name("// pub fn commented() {"), None);
    assert_eq!(sym_name("pub use crate::foo;"), None);
    assert_eq!(sym_name(""), None);
}

#[test]
fn module_doc_extracts_first_inner_doc_line() {
    let src = "#![allow(dead_code)]\n//! First line of purpose.\n//! second line\npub fn a() {}\n";
    assert_eq!(module_doc(src).as_deref(), Some("First line of purpose."));
}

#[test]
fn module_doc_absent_when_code_comes_first() {
    assert_eq!(module_doc("use std::io;\n//! not a module doc\n"), None);
    assert_eq!(module_doc("pub fn a() {}\n"), None);
}

#[test]
fn build_card_caps_symbols_and_counts_overflow() {
    let mut src = String::from("//! Big module.\n");
    let total = MAX_SYMBOLS_PER_COMPONENT + 10;
    for i in 0..total {
        src.push_str(&format!("pub fn f{i}() {{}}\n"));
    }
    let card = build_card("big::mod", "src/big.rs", &src, 100, 20);
    let kept = card.fns.len() + card.types.len() + card.traits.len() + card.consts.len();
    assert_eq!(kept, MAX_SYMBOLS_PER_COMPONENT);
    assert_eq!(card.more, 10);
    assert_eq!(card.doc.as_deref(), Some("Big module."));
}

#[test]
fn render_includes_header_and_component_names() {
    let card = build_card(
        "crate::foo",
        "src/foo.rs",
        "//! Foo does things.\npub fn go() {}\npub struct Foo;\n",
        42,
        3,
    );
    let out = render(&[card], 42);
    assert!(out.contains("# Component map"));
    assert!(out.contains("### crate::foo  (src/foo.rs · 42 tok)"));
    assert!(out.contains("Foo does things."));
    assert!(out.contains("fns: go"));
    assert!(out.contains("types: Foo"));
}

#[test]
fn taxonomy_outline_groups_components_by_cluster_in_loop_order() {
    let graph = Graph {
        nodes: vec![
            Node::code("crate::evolve::map", "src/evolve/map.rs"),
            Node::code("crate::agent::execution", "src/agent/execution.rs"),
            Node::code("crate::tools::shell", "src/tools/shell.rs"),
        ],
        edges: Vec::<Edge>::new(),
    };
    let out = crate::evolve::clusters::taxonomy_outline(&graph);
    assert!(out.contains("# Architectural taxonomy"));
    assert!(out.contains("## Loop Core — agent"));
    assert!(out.contains("## Action — tools"));
    assert!(out.contains("## Evolution — evolve"));
    // Loop Core precedes Action precedes Evolution (dev-loop order).
    let loop_core = out.find("Loop Core").unwrap();
    let action = out.find("Action").unwrap();
    let evolution = out.find("Evolution").unwrap();
    assert!(loop_core < action && action < evolution);
}

#[test]
fn orientation_always_has_taxonomy_and_gates_the_map() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("errors.rs"), "//! Errors.\npub struct E;\n").unwrap();
    let graph = Graph {
        nodes: vec![Node::code("crate::errors", "errors.rs")],
        edges: Vec::<Edge>::new(),
    };

    let without = orientation(&graph, dir.path(), false);
    assert!(without.contains("# Architectural taxonomy"));
    assert!(!without.contains("# Component map"));

    let with = orientation(&graph, dir.path(), true);
    assert!(with.contains("# Architectural taxonomy"));
    assert!(with.contains("# Component map"));
    assert!(with.len() > without.len());
}

#[test]
fn build_map_and_expand_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let src =
        "//! Errors for the crate.\npub struct MyError;\npub fn boom() -> MyError { MyError }\n";
    std::fs::write(dir.path().join("errors.rs"), src).unwrap();

    let graph = Graph {
        nodes: vec![Node::code("crate::errors", "errors.rs")],
        edges: Vec::<Edge>::new(),
    };

    let map = build_map(&graph, dir.path());
    assert_eq!(map.components, 1);
    assert!(map.map_tokens > 0);
    assert!(map.rendered.contains("crate::errors"));
    assert!(map.rendered.contains("Errors for the crate."));

    // Signatures view keeps declarations, elides bodies.
    let sigs = expand(&graph, dir.path(), "crate::errors", None, false).unwrap();
    assert!(sigs.contains("pub fn boom"));
    assert!(sigs.contains("pub struct MyError"));

    // Full view returns the (comment-stripped) source.
    let full = expand(&graph, dir.path(), "crate::errors", None, true).unwrap();
    assert!(full.contains("pub fn boom"));

    // Symbol view returns just that item's numbered span.
    let sym = expand(&graph, dir.path(), "crate::errors", Some("boom"), false).unwrap();
    assert!(sym.contains("pub fn boom"));
    assert!(!sym.contains("pub struct MyError"));

    // Unknown component yields nothing.
    assert!(expand(&graph, dir.path(), "crate::missing", None, false).is_none());
}
