//! Unit tests for the evolve graph query tools (`tools::graph`).
//!
//! Each test builds a tiny fixture project in a tempdir — a handful of
//! source files plus a small graph saved via `OntologyStore` — and points
//! the tools at that root. The real 110K-line `.selfware/evolve-graph.yaml`
//! is never parsed here.

use super::*;
use crate::evolve::{Edge, EdgeType, Graph, Node, OntologyStore};
use serde_json::json;

fn code_node(id: &str, path: &str, tokens: usize, lines: usize, complexity: Option<f64>) -> Node {
    let mut node = Node::code(id, path);
    node.tokens = tokens;
    node.lines = lines;
    node.complexity = complexity;
    node
}

/// Fixture project: alpha ← beta ← delta, gamma standalone, alpha owns a test
/// file, beta owns a test file, alpha carries an inline test block.
fn fixture_project() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    std::fs::create_dir_all(root.join("src")).expect("src dir");
    std::fs::create_dir_all(root.join("tests")).expect("tests dir");
    std::fs::write(
        root.join("src/alpha.rs"),
        "//! Alpha module.\npub fn alpha_fn() {}\npub struct AlphaThing;\n",
    )
    .expect("write alpha");
    std::fs::write(
        root.join("src/beta.rs"),
        "//! Beta module.\npub fn beta_fn() {}\n",
    )
    .expect("write beta");
    std::fs::write(
        root.join("src/gamma.rs"),
        "//! Gamma module.\npub fn gamma_fn() {}\n",
    )
    .expect("write gamma");
    std::fs::write(
        root.join("src/delta.rs"),
        "//! Delta module.\npub fn delta_fn() {}\n",
    )
    .expect("write delta");
    std::fs::write(
        root.join("tests/alpha_test.rs"),
        "pub fn alpha_test_harness() {}\n",
    )
    .expect("write test file");
    std::fs::write(
        root.join("tests/beta_test.rs"),
        "pub fn beta_test_harness() {}\n",
    )
    .expect("write beta test file");

    let mut alpha = code_node("crate::alpha", "src/alpha.rs", 100, 10, Some(5.0));
    alpha.inline_test_ranges = 1;
    alpha.inline_test_lines = 8;
    alpha.inline_test_tokens = 25;
    let mut alpha_test = Node::test("tests::alpha_test", "tests/alpha_test.rs");
    alpha_test.tokens = 40;
    alpha_test.lines = 5;
    let mut beta_test = Node::test("tests::beta_test", "tests/beta_test.rs");
    beta_test.tokens = 30;
    beta_test.lines = 4;
    let graph = Graph {
        nodes: vec![
            alpha,
            code_node("crate::beta", "src/beta.rs", 300, 30, Some(30.0)),
            code_node("crate::gamma", "src/gamma.rs", 200, 20, None),
            code_node("crate::delta", "src/delta.rs", 150, 15, None),
            alpha_test,
            beta_test,
        ],
        edges: vec![
            Edge {
                from: "crate::beta".into(),
                to: "crate::alpha".into(),
                edge_type: EdgeType::DependsOn,
            },
            Edge {
                from: "crate::delta".into(),
                to: "crate::beta".into(),
                edge_type: EdgeType::DependsOn,
            },
            Edge {
                from: "crate::alpha".into(),
                to: "tests::alpha_test".into(),
                edge_type: EdgeType::Contains,
            },
            Edge {
                from: "crate::beta".into(),
                to: "tests::beta_test".into(),
                edge_type: EdgeType::Contains,
            },
        ],
    };
    OntologyStore::new(root.join(".selfware/evolve-graph.yaml"))
        .save(&graph)
        .expect("save graph");
    temp
}

#[tokio::test]
async fn graph_summary_reports_envelope_outline_and_rollups() {
    let temp = fixture_project();
    let tool = GraphSummaryTool::new(temp.path().to_path_buf());
    let out = tool.execute(json!({})).await.expect("summary");

    assert_eq!(out["graph_revision"].as_str().unwrap().len(), 12);
    assert_ne!(out["graph_built_at"].as_str().unwrap(), "unknown");
    assert!(out["measured_tokens"].as_u64().unwrap() > 0);
    assert_eq!(out["budget_tokens"].as_u64().unwrap(), 1500);
    assert!(!out["truncated"].as_bool().unwrap());
    assert_eq!(out["dropped"], json!({}));

    let payload = &out["payload"];
    assert!(
        payload["outline"]
            .as_str()
            .unwrap()
            .contains("# Architectural taxonomy"),
        "outline missing taxonomy header"
    );
    let hotspots = payload["hotspots"].as_array().unwrap();
    assert_eq!(hotspots[0]["id"], "crate::beta");
    assert_eq!(hotspots[0]["tokens"], 300);
    let clusters = payload["clusters"].as_array().unwrap();
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0]["components"], 4);
    assert_eq!(clusters[0]["tokens"], 750);
}

#[tokio::test]
async fn graph_summary_truncates_honestly_on_tiny_budget() {
    let temp = fixture_project();
    let tool = GraphSummaryTool::new(temp.path().to_path_buf());
    let out = tool.execute(json!({"budget": 10})).await.expect("summary");

    assert!(out["truncated"].as_bool().unwrap());
    assert_eq!(out["dropped"]["hotspots"], 4);
    assert_eq!(out["dropped"]["clusters"], 1);
    assert!(out["payload"]["hotspots"].as_array().unwrap().is_empty());
    assert!(out["payload"]["clusters"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn hotspots_ranks_filters_and_validates_args() {
    let temp = fixture_project();
    let tool = HotspotsTool::new(temp.path().to_path_buf());

    let out = tool
        .execute(json!({"metric": "tokens", "layer": "code", "k": 2}))
        .await
        .expect("hotspots");
    let rows = out["payload"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], "crate::beta");
    assert_eq!(rows[1]["id"], "crate::gamma");
    // Self-bounding: measured cost is reported as its own budget.
    assert_eq!(out["measured_tokens"], out["budget_tokens"]);
    assert_eq!(out["truncated"], false);

    let out = tool
        .execute(json!({"layer": "test"}))
        .await
        .expect("test-layer hotspots");
    let rows = out["payload"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], "tests::alpha_test");
    assert_eq!(rows[1]["id"], "tests::beta_test");

    let err = tool
        .execute(json!({"metric": "vibes"}))
        .await
        .expect_err("invalid metric must fail");
    assert!(err.to_string().contains("unknown metric"), "got: {err}");
    let err = tool
        .execute(json!({"layer": "vibes"}))
        .await
        .expect_err("invalid layer must fail");
    assert!(err.to_string().contains("unknown layer"), "got: {err}");
}

#[tokio::test]
async fn context_pack_packs_seeds_and_frontier_within_budget() {
    let temp = fixture_project();
    let tool = ContextPackTool::new(temp.path().to_path_buf());
    let out = tool
        .execute(json!({"task_keywords": "alpha", "token_budget": 8000}))
        .await
        .expect("pack");

    let payload = &out["payload"];
    assert_eq!(payload["fits"], true);
    let included: Vec<&str> = payload["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(included.contains(&"crate::alpha"), "got: {included:?}");
    let documents = payload["documents"].as_array().unwrap();
    assert!(
        documents.iter().all(|d| d["role"] == "seed"),
        "all selected nodes are seeds in this fixture"
    );
    assert!(
        documents.iter().all(
            |d| d["tokens"].as_u64().unwrap() > 0 && !d["content"].as_str().unwrap().is_empty()
        ),
        "every document must carry measured tokens and content"
    );
    assert_eq!(payload["content_hash"].as_str().unwrap().len(), 64);
    assert!(payload["total_tokens"].as_u64().unwrap() > 0);
    assert_eq!(out["truncated"], false);
    assert_eq!(out["dropped"], json!({}));
    assert_eq!(out["graph_revision"].as_str().unwrap().len(), 12);
}

#[tokio::test]
async fn context_pack_includes_linked_tests_when_asked() {
    let temp = fixture_project();
    let tool = ContextPackTool::new(temp.path().to_path_buf());
    let out = tool
        .execute(json!({"task_keywords": "alpha", "include_tests": true}))
        .await
        .expect("pack");

    let documents = out["payload"]["documents"].as_array().unwrap();
    let test_doc = documents
        .iter()
        .find(|d| d["role"] == "test")
        .expect("a test-role document must be packed");
    assert_eq!(test_doc["id"], "tests::alpha_test");
    assert!(
        test_doc["content"]
            .as_str()
            .unwrap()
            .contains("alpha_test_harness"),
        "test document must carry the projected test source"
    );
}

#[tokio::test]
async fn context_pack_signatures_detail_projects_interfaces() {
    let temp = fixture_project();
    let tool = ContextPackTool::new(temp.path().to_path_buf());
    let out = tool
        .execute(json!({"task_keywords": "alpha", "detail": "signatures"}))
        .await
        .expect("pack");

    let documents = out["payload"]["documents"].as_array().unwrap();
    let seed = documents
        .iter()
        .find(|d| d["id"] == "crate::alpha")
        .expect("alpha seed document");
    assert!(
        seed["content"].as_str().unwrap().contains("alpha_fn"),
        "signature projection must name the public fn: {}",
        seed["content"]
    );
}

#[tokio::test]
async fn context_pack_unknown_keywords_suggests_nearest_ids() {
    let temp = fixture_project();
    let tool = ContextPackTool::new(temp.path().to_path_buf());
    let out = tool
        .execute(json!({"task_keywords": "zzzzz"}))
        .await
        .expect("no-match pack is not an error");

    let payload = &out["payload"];
    assert_eq!(payload["matches"], 0);
    assert!(
        !payload["suggestions"].as_array().unwrap().is_empty(),
        "no-match responses must suggest real node ids"
    );
}

#[tokio::test]
async fn context_pack_tiny_budget_reports_fits_false_with_cheapest_cost() {
    let temp = fixture_project();
    let tool = ContextPackTool::new(temp.path().to_path_buf());
    let out = tool
        .execute(json!({"task_keywords": "alpha", "token_budget": 1}))
        .await
        .expect("pack");

    let payload = &out["payload"];
    assert_eq!(payload["fits"], false);
    assert!(
        payload["cheapest_cost_tokens"].as_u64().unwrap() > 0,
        "must name the cheapest option's measured cost"
    );
    assert!(payload["documents"].as_array().unwrap().is_empty());
    assert!(
        !payload["dropped_detail"].as_array().unwrap().is_empty(),
        "every cut node must be reported"
    );
    assert_eq!(out["truncated"], true);
}

#[tokio::test]
async fn missing_graph_errors_honestly() {
    let temp = tempfile::tempdir().expect("tempdir");
    let tool = GraphSummaryTool::new(temp.path().to_path_buf());
    let err = tool.execute(json!({})).await.expect_err("must fail");
    assert!(
        err.to_string().contains("selfware self-evolve"),
        "got: {err}"
    );
}

#[test]
fn tools_declare_read_only_metadata_and_categories() {
    let temp = fixture_project();
    let root = temp.path().to_path_buf();
    let summary = GraphSummaryTool::new(root.clone());
    let hotspots = HotspotsTool::new(root.clone());
    let pack = ContextPackTool::new(root.clone());
    let impact = ImpactTool::new(root.clone());
    let neighbors = NeighborsTool::new(root.clone());
    let test_map = TestMapTool::new(root.clone());
    let cycles = CyclesTool::new(root.clone());
    let dups = DupsTool::new(root);
    for tool in [
        &summary as &dyn Tool,
        &hotspots,
        &pack,
        &impact,
        &neighbors,
        &test_map,
        &cycles,
        &dups,
    ] {
        let metadata = tool.metadata();
        assert!(metadata.read_only, "{} must be read-only", tool.name());
        assert!(!metadata.destructive);
        assert_eq!(metadata.risk_level, crate::safety::RiskLevel::Low);
    }

    for name in [
        "graph_summary",
        "hotspots",
        "context_pack",
        "impact",
        "neighbors",
        "test_map",
        "cycles",
        "dups",
    ] {
        assert_eq!(
            crate::tools::tool_search::categorize_tool(name),
            "code_intelligence",
            "{name} must be categorized as code_intelligence"
        );
    }
}

#[test]
fn registry_registers_the_graph_tools_as_deferred() {
    let registry = crate::tools::ToolRegistry::new();
    for name in [
        "graph_summary",
        "hotspots",
        "context_pack",
        "impact",
        "neighbors",
        "test_map",
        "cycles",
        "dups",
    ] {
        assert!(registry.get(name).is_some(), "{name} not registered");
        assert!(
            !registry.is_activated(name),
            "{name} must be deferred, not critical"
        );
        let found = registry.search(name, 1);
        assert_eq!(found[0].category, "code_intelligence");
    }
}

// ---------------------------------------------------------------------------
// Phase 2: impact / neighbors / test_map, hotspots filters, pack reasons
// ---------------------------------------------------------------------------

#[tokio::test]
async fn impact_ranks_by_depth_and_types_every_row() {
    let temp = fixture_project();
    let tool = ImpactTool::new(temp.path().to_path_buf());
    let out = tool
        .execute(json!({"id": "crate::alpha"}))
        .await
        .expect("impact");

    let payload = &out["payload"];
    let rows = payload["rows"].as_array().unwrap();
    // alpha ← beta (depth 1) ← delta (depth 2).
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], "crate::beta");
    assert_eq!(rows[0]["depth"], 1);
    assert_eq!(rows[0]["via"], "crate::alpha");
    assert_eq!(rows[1]["id"], "crate::delta");
    assert_eq!(rows[1]["depth"], 2);
    assert_eq!(rows[1]["via"], "crate::beta");
    for row in rows {
        assert_eq!(
            row["edge_type"], "depends_on",
            "every impact row must name its edge type"
        );
        assert!(row["tokens"].as_u64().unwrap() > 0);
    }
    assert_eq!(out["graph_revision"].as_str().unwrap().len(), 12);
    assert_eq!(out["truncated"], false);
}

#[tokio::test]
async fn impact_respects_depth_and_k_and_reports_drops() {
    let temp = fixture_project();
    let tool = ImpactTool::new(temp.path().to_path_buf());

    let out = tool
        .execute(json!({"id": "crate::alpha", "depth": 1}))
        .await
        .expect("depth-1 impact");
    let rows = out["payload"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "crate::beta");

    let out = tool
        .execute(json!({"id": "crate::alpha", "k": 1}))
        .await
        .expect("k=1 impact");
    assert_eq!(out["payload"]["count"], 1);
    assert_eq!(out["truncated"], true);
    assert_eq!(out["dropped"]["impact"], 1);
}

#[tokio::test]
async fn impact_unknown_id_errors_with_suggestions() {
    let temp = fixture_project();
    let tool = ImpactTool::new(temp.path().to_path_buf());
    let err = tool
        .execute(json!({"id": "crate::alpa"}))
        .await
        .expect_err("unknown id must fail");
    let message = err.to_string();
    assert!(message.contains("unknown node id"), "got: {message}");
    assert!(
        message.contains("crate::alpha"),
        "must suggest the closest real id, got: {message}"
    );
}

#[tokio::test]
async fn neighbors_lists_typed_edges_and_filters() {
    let temp = fixture_project();
    let tool = NeighborsTool::new(temp.path().to_path_buf());

    // beta: out depends_on→alpha, in depends_on←delta, out contains→beta_test.
    let out = tool
        .execute(json!({"id": "crate::beta"}))
        .await
        .expect("neighbors");
    let rows = out["payload"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    for row in rows {
        assert!(
            row["edge_type"].as_str().is_some_and(|t| !t.is_empty()),
            "every neighbor row must name its edge type: {row}"
        );
    }
    let alpha_row = rows.iter().find(|r| r["id"] == "crate::alpha").unwrap();
    assert_eq!(alpha_row["edge_type"], "depends_on");
    assert_eq!(alpha_row["direction"], "out");
    let delta_row = rows.iter().find(|r| r["id"] == "crate::delta").unwrap();
    assert_eq!(delta_row["edge_type"], "depends_on");
    assert_eq!(delta_row["direction"], "in");
    let test_row = rows.iter().find(|r| r["id"] == "tests::beta_test").unwrap();
    assert_eq!(test_row["edge_type"], "contains");
    assert_eq!(test_row["direction"], "out");

    // kind filter
    let out = tool
        .execute(json!({"id": "crate::beta", "kind": "contains"}))
        .await
        .expect("contains neighbors");
    let rows = out["payload"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "tests::beta_test");

    // direction filter
    let out = tool
        .execute(json!({"id": "crate::beta", "direction": "in"}))
        .await
        .expect("incoming neighbors");
    let rows = out["payload"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "crate::delta");

    // invalid kind / unknown id are typed errors
    let err = tool
        .execute(json!({"id": "crate::beta", "kind": "vibes"}))
        .await
        .expect_err("invalid kind must fail");
    assert!(err.to_string().contains("unknown kind"), "got: {err}");
    let err = tool
        .execute(json!({"id": "crate::bta"}))
        .await
        .expect_err("unknown id must fail");
    assert!(
        err.to_string().contains("crate::beta"),
        "must suggest closest ids, got: {err}"
    );
}

#[tokio::test]
async fn test_map_reports_contains_inline_and_dependent_tests() {
    let temp = fixture_project();
    let tool = TestMapTool::new(temp.path().to_path_buf());
    let out = tool
        .execute(json!({"id": "crate::alpha"}))
        .await
        .expect("test_map");

    let payload = &out["payload"];
    let tests = payload["tests"].as_array().unwrap();
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0]["id"], "tests::alpha_test");
    assert_eq!(tests[0]["path"], "tests/alpha_test.rs");
    assert_eq!(tests[0]["tokens"], 40);

    let inline = &payload["inline"];
    assert_eq!(inline["inline_test_ranges"], 1);
    assert_eq!(inline["inline_test_lines"], 8);
    assert_eq!(inline["inline_test_tokens"], 25);

    // Dependents of alpha = beta; beta's tests must be listed as also_run.
    let also_run = payload["also_run"].as_array().unwrap();
    assert_eq!(also_run.len(), 1);
    assert_eq!(also_run[0]["id"], "tests::beta_test");
    assert_eq!(also_run[0]["via"], "crate::beta");
    assert_eq!(also_run[0]["tokens"], 30);

    let err = tool
        .execute(json!({"id": "crate::alpa"}))
        .await
        .expect_err("unknown id must fail");
    assert!(err.to_string().contains("crate::alpha"), "got: {err}");
}

#[tokio::test]
async fn hotspots_exclude_prefix_filters_rows() {
    let temp = fixture_project();
    let tool = HotspotsTool::new(temp.path().to_path_buf());

    let out = tool
        .execute(json!({"layer": "code", "exclude_prefix": "crate::gamma", "k": 10}))
        .await
        .expect("filtered hotspots");
    let payload = &out["payload"];
    assert_eq!(payload["exclude_prefix"], "crate::gamma");
    let rows = payload["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert!(
        rows.iter().all(|r| r["id"] != "crate::gamma"),
        "excluded prefix must not appear: {rows:?}"
    );
    // Ranking still holds over the remaining candidates.
    assert_eq!(rows[0]["id"], "crate::beta");
}

/// Fixture with 10 code nodes so top-8 seed selection leaves two nodes to
/// the frontier: alpha (the keyword hit) plus fillers f1..f9, where f8 and
/// f9 depend on alpha but have the smallest size priors — f8 has a readable
/// file (packed as a frontier document), f9's file is deliberately missing
/// so the precise drop reason is observable.
fn fixture_project_frontier() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    std::fs::create_dir_all(root.join("src")).expect("src dir");
    std::fs::write(
        root.join("src/alpha.rs"),
        "//! Alpha module.\npub fn alpha_fn() {}\n",
    )
    .expect("write alpha");
    let mut nodes = vec![code_node(
        "crate::alpha",
        "src/alpha.rs",
        400,
        40,
        Some(10.0),
    )];
    let mut edges = Vec::new();
    for i in 1..=9 {
        // f9 is the smallest filler and has no file on disk.
        let tokens = 400 - 10 * i;
        if i < 9 {
            std::fs::write(
                root.join(format!("src/f{i}.rs")),
                format!("//! Filler {i}.\npub fn f{i}_fn() {{}}\n"),
            )
            .expect("write filler");
        }
        nodes.push(code_node(
            &format!("crate::f{i}"),
            &format!("src/f{i}.rs"),
            tokens,
            10,
            None,
        ));
    }
    for dependent in ["crate::f8", "crate::f9"] {
        edges.push(Edge {
            from: dependent.into(),
            to: "crate::alpha".into(),
            edge_type: EdgeType::DependsOn,
        });
    }
    OntologyStore::new(root.join(".selfware/evolve-graph.yaml"))
        .save(&Graph { nodes, edges })
        .expect("save graph");
    temp
}

#[tokio::test]
async fn context_pack_frontier_documents_name_edge_type_and_direction() {
    let temp = fixture_project_frontier();
    let tool = ContextPackTool::new(temp.path().to_path_buf());
    let out = tool
        .execute(json!({"task_keywords": "alpha"}))
        .await
        .expect("pack");

    let payload = &out["payload"];
    // f8 is not among the top-8 seeds; it is packed via the dependent
    // frontier and must name the connecting edge type and direction.
    let documents = payload["documents"].as_array().unwrap();
    let f8_doc = documents
        .iter()
        .find(|d| d["id"] == "crate::f8")
        .expect("f8 must be packed as a frontier document");
    assert_eq!(f8_doc["role"], "dependent");
    assert_eq!(f8_doc["edge_type"], "depends_on");
    assert_eq!(f8_doc["direction"], "in");
    assert!(
        f8_doc["reason"]
            .as_str()
            .unwrap()
            .contains("depends_on edge"),
        "reason must name the connecting edge: {f8_doc}"
    );

    // f9's file is missing, so it must be dropped with the precise reason.
    let dropped = payload["dropped_detail"].as_array().unwrap();
    let f9_drop = dropped
        .iter()
        .find(|d| d["id"] == "crate::f9")
        .expect("f9 must be reported in dropped_detail");
    assert_eq!(f9_drop["role"], "dependent");
    assert!(
        f9_drop["reason"]
            .as_str()
            .unwrap()
            .contains("source file unreadable"),
        "drop reason must say WHY: {f9_drop}"
    );

    // No document may ship with a bare "no readable source projection".
    for entry in dropped {
        assert_ne!(
            entry["reason"], "no readable source projection",
            "bare drop reasons are banned: {entry}"
        );
    }
    assert_eq!(out["dropped"]["dependent"], 1);
}

#[tokio::test]
async fn context_pack_test_documents_name_contains_edge() {
    let temp = fixture_project();
    let tool = ContextPackTool::new(temp.path().to_path_buf());
    let out = tool
        .execute(json!({"task_keywords": "alpha", "include_tests": true}))
        .await
        .expect("pack");

    let documents = out["payload"]["documents"].as_array().unwrap();
    let test_doc = documents
        .iter()
        .find(|d| d["role"] == "test" && d["id"] == "tests::alpha_test")
        .expect("alpha's test document must be packed");
    assert_eq!(test_doc["edge_type"], "contains");
    assert_eq!(test_doc["direction"], "out");
    assert!(
        test_doc["reason"]
            .as_str()
            .unwrap()
            .contains("contains edge"),
        "reason must name the connecting edge: {test_doc}"
    );
    // Seeds carry no edge fields.
    let seed = documents
        .iter()
        .find(|d| d["id"] == "crate::alpha")
        .expect("alpha seed");
    assert!(seed.get("edge_type").is_none() || seed["edge_type"].is_null());
}

// ---------------------------------------------------------------------------
// Task-start L0 injection note
// ---------------------------------------------------------------------------

#[test]
fn graph_summary_note_renders_envelope_and_pointer() {
    let temp = fixture_project();
    let note = graph_summary_note(temp.path()).expect("note must render with a graph");

    assert!(
        note.contains("<selfware_context_note kind=graph_summary revision="),
        "envelope opening missing: {note}"
    );
    assert!(note.contains("built_at="), "built_at missing: {note}");
    assert!(
        note.contains("# Architectural taxonomy"),
        "outline missing: {note}"
    );
    assert!(
        note.contains("crate::beta (300)"),
        "hotspot rows missing: {note}"
    );
    assert!(
        note.contains("Query the full graph via tool_search: graph_summary, hotspots, context_pack, impact, neighbors, test_map, cycles, dups."),
        "tool pointer missing: {note}"
    );
    assert!(note.trim_end().ends_with("</selfware_context_note>"));

    // Revision inside the envelope is the 12-char graph revision.
    let revision = note
        .split("revision=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .expect("revision field");
    assert_eq!(revision.len(), 12);

    // The tiny fixture packs well under the L0 budget — measured, not assumed.
    assert!(crate::token_count::estimate_content_tokens(&note) <= L0_NOTE_BUDGET);
}

#[test]
fn graph_summary_note_is_silent_without_a_graph() {
    let temp = tempfile::tempdir().expect("tempdir");
    assert!(
        graph_summary_note(temp.path()).is_none(),
        "missing graph must mean no note, never an error"
    );
}

#[test]
fn graph_summary_note_handles_real_repo_graph_if_present() {
    // Diagnostic (2026-08-29): fixture tests passed but the note silently
    // never fired on the real 3499-node graph. Skips when no graph exists.
    let root = std::path::Path::new("/home/rig/selfware");
    if !root.join(".selfware/evolve-graph.yaml").exists() {
        eprintln!("no real graph; skipping");
        return;
    }
    let note = crate::tools::graph::graph_summary_note(root);
    assert!(note.is_some(), "note must render against the real graph");
    let note = note.unwrap();
    assert!(note.contains("selfware_context_note"));
    eprintln!("note len chars: {}", note.len());
}

// ---------------------------------------------------------------------------
// Phase 3: cycles / dups / test_map mirror fallback / impact paging
// ---------------------------------------------------------------------------

/// Cycle + duplication fixture (these tools read the graph only — no source
/// files needed): cyc_a → cyc_b → cyc_c → cyc_a, a DuplicateOf pair with
/// drift 400, a SimilarTo pair with drift 50.
fn fixture_project_cycles() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    let graph = Graph {
        nodes: vec![
            code_node("crate::cyc_a", "src/cyc_a.rs", 100, 10, None),
            code_node("crate::cyc_b", "src/cyc_b.rs", 200, 20, None),
            code_node("crate::cyc_c", "src/cyc_c.rs", 300, 30, None),
            code_node("crate::dup_x", "src/dup_x.rs", 500, 50, None),
            code_node("crate::dup_y", "src/dup_y.rs", 100, 10, None),
            code_node("crate::sim_p", "src/sim_p.rs", 300, 30, None),
            code_node("crate::sim_q", "src/sim_q.rs", 250, 25, None),
        ],
        edges: vec![
            Edge {
                from: "crate::cyc_a".into(),
                to: "crate::cyc_b".into(),
                edge_type: EdgeType::DependsOn,
            },
            Edge {
                from: "crate::cyc_b".into(),
                to: "crate::cyc_c".into(),
                edge_type: EdgeType::DependsOn,
            },
            Edge {
                from: "crate::cyc_c".into(),
                to: "crate::cyc_a".into(),
                edge_type: EdgeType::DependsOn,
            },
            Edge {
                from: "crate::dup_x".into(),
                to: "crate::dup_y".into(),
                edge_type: EdgeType::DuplicateOf,
            },
            Edge {
                from: "crate::sim_p".into(),
                to: "crate::sim_q".into(),
                edge_type: EdgeType::SimilarTo,
            },
        ],
    };
    OntologyStore::new(root.join(".selfware/evolve-graph.yaml"))
        .save(&graph)
        .expect("save graph");
    temp
}

/// Mirror-tree fixture: no Contains edges at all — the tests are only
/// reachable via the tests/unit/<module>/ path rule. `crate::a::c` uses a
/// `mod.rs` path to cover that arm.
fn fixture_project_mirror() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    let mut mirror_b = Node::test("test::unit::a::b::b_test", "tests/unit/a/b/b_test.rs");
    mirror_b.tokens = 60;
    mirror_b.lines = 6;
    let mut mirror_c = Node::test("test::unit::a::c::c_test", "tests/unit/a/c/c_test.rs");
    mirror_c.tokens = 70;
    mirror_c.lines = 7;
    let graph = Graph {
        nodes: vec![
            code_node("crate::a::b", "src/a/b.rs", 100, 10, None),
            code_node("crate::a::c", "src/a/c/mod.rs", 110, 11, None),
            mirror_b,
            mirror_c,
        ],
        edges: vec![],
    };
    OntologyStore::new(root.join(".selfware/evolve-graph.yaml"))
        .save(&graph)
        .expect("save graph");
    temp
}

#[tokio::test]
async fn cycles_reports_dependency_cycles_with_tokens() {
    let temp = fixture_project_cycles();
    let tool = CyclesTool::new(temp.path().to_path_buf());
    let out = tool.execute(json!({})).await.expect("cycles");

    let payload = &out["payload"];
    assert_eq!(payload["count"], 1);
    let rows = payload["rows"].as_array().unwrap();
    let cycle = &rows[0];
    let path: Vec<&str> = cycle["path"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(path.first(), path.last(), "cycle must close: {path:?}");
    assert_eq!(cycle["length"], 3);
    assert_eq!(cycle["total_tokens"], 600);
    for member in ["crate::cyc_a", "crate::cyc_b", "crate::cyc_c"] {
        assert!(path.contains(&member), "missing {member}: {path:?}");
    }
    assert_eq!(out["graph_revision"].as_str().unwrap().len(), 12);

    // Acyclic graph → honest empty result, not an error.
    let temp = fixture_project();
    let tool = CyclesTool::new(temp.path().to_path_buf());
    let out = tool.execute(json!({})).await.expect("cycles on acyclic");
    assert_eq!(out["payload"]["count"], 0);
    assert!(out["payload"]["rows"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn dups_lists_pairs_and_ranks_by_drift() {
    let temp = fixture_project_cycles();
    let tool = DupsTool::new(temp.path().to_path_buf());

    // Default: lexicographic by edge_type then from — duplicate_of first.
    let out = tool.execute(json!({})).await.expect("dups");
    let rows = out["payload"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["edge_type"], "duplicate_of");
    assert_eq!(rows[0]["from"], "crate::dup_x");
    assert_eq!(rows[0]["from_tokens"], 500);
    assert_eq!(rows[0]["to_tokens"], 100);
    assert_eq!(rows[0]["drift"], 400);
    assert_eq!(rows[1]["edge_type"], "similar_to");

    // drift=true: the 400-drift clone pair beats the 50-drift similar pair.
    let out = tool
        .execute(json!({"drift": true}))
        .await
        .expect("dups drift");
    let rows = out["payload"]["rows"].as_array().unwrap();
    assert_eq!(rows[0]["drift"], 400);
    assert_eq!(rows[1]["drift"], 50);

    // k caps rows; the cap is reported, never silent.
    let out = tool.execute(json!({"k": 1})).await.expect("dups k=1");
    assert_eq!(out["payload"]["count"], 1);
    assert_eq!(out["payload"]["total_pairs"], 2);
    assert_eq!(out["truncated"], true);
    assert_eq!(out["dropped"]["dups"], 1);
}

#[tokio::test]
async fn test_map_finds_mirror_tree_tests_via_path_rule() {
    let temp = fixture_project_mirror();
    let tool = TestMapTool::new(temp.path().to_path_buf());

    for (id, mirror_id, mirror_path) in [
        (
            "crate::a::b",
            "test::unit::a::b::b_test",
            "tests/unit/a/b/b_test.rs",
        ),
        (
            "crate::a::c",
            "test::unit::a::c::c_test",
            "tests/unit/a/c/c_test.rs",
        ),
    ] {
        let out = tool.execute(json!({"id": id})).await.expect("test_map");
        let payload = &out["payload"];
        assert!(
            payload["tests"].as_array().unwrap().is_empty(),
            "{id} has no Contains-linked tests in this fixture"
        );
        let mirror = payload["mirror_tests"].as_array().unwrap();
        assert_eq!(mirror.len(), 1, "{id} must find its mirror tree");
        assert_eq!(mirror[0]["id"], mirror_id);
        assert_eq!(mirror[0]["path"], mirror_path);
        assert_eq!(mirror[0]["source"], "mirror_path_rule");
        assert!(
            payload["mirror_rule"]
                .as_str()
                .unwrap()
                .contains("tests/unit/"),
            "the payload must cite the rule it applied: {payload}"
        );
    }
}

#[tokio::test]
async fn impact_pages_with_offset_and_next_offset() {
    // alpha's dependents in this fixture are f8 (320 tok) and f9 (310 tok).
    let temp = fixture_project_frontier();
    let tool = ImpactTool::new(temp.path().to_path_buf());

    let out = tool
        .execute(json!({"id": "crate::alpha", "k": 1}))
        .await
        .expect("impact page 1");
    let payload = &out["payload"];
    assert_eq!(payload["total_rows"], 2);
    assert_eq!(payload["offset"], 0);
    assert_eq!(payload["next_offset"], 1);
    let rows = payload["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "crate::f8");
    assert_eq!(out["truncated"], true);

    let out = tool
        .execute(json!({"id": "crate::alpha", "k": 1, "offset": 1}))
        .await
        .expect("impact page 2");
    let payload = &out["payload"];
    assert_eq!(payload["total_rows"], 2);
    assert_eq!(payload["offset"], 1);
    assert!(
        payload["next_offset"].is_null(),
        "next_offset must be null when the page is exhausted"
    );
    let rows = payload["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "crate::f9");
    assert_eq!(out["truncated"], false);

    // Offset past the end: empty page, still honest.
    let out = tool
        .execute(json!({"id": "crate::alpha", "k": 1, "offset": 5}))
        .await
        .expect("impact past end");
    assert!(out["payload"]["rows"].as_array().unwrap().is_empty());
    assert!(out["payload"]["next_offset"].is_null());
}
