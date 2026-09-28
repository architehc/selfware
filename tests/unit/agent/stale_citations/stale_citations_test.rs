use super::*;
use crate::agent::citation_check::{
    citation_anchor, moved_citation_range, parse_citations, unchanged_line_map, Citation,
};

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

fn cite(path: &str, start: usize, end: usize, symbol: Option<&str>) -> Citation {
    Citation {
        path: path.to_string(),
        start,
        end,
        symbol: symbol.map(str::to_string),
        quote: None,
    }
}

const ORIGINAL: &str = "use std::fmt;\n\
    \n\
    pub fn alpha() -> u32 {\n\
    \x20   1\n\
    }\n\
    \n\
    pub fn beta() -> u32 {\n\
    \x20   2\n\
    }\n\
    \n\
    pub fn gamma() -> u32 {\n\
    \x20   3\n\
    }\n";

/// ORIGINAL with a one-line `///` above each function (the c24 edit).
const DOCUMENTED: &str = "use std::fmt;\n\
    \n\
    /// Alpha.\n\
    pub fn alpha() -> u32 {\n\
    \x20   1\n\
    }\n\
    \n\
    /// Beta.\n\
    pub fn beta() -> u32 {\n\
    \x20   2\n\
    }\n\
    \n\
    /// Gamma.\n\
    pub fn gamma() -> u32 {\n\
    \x20   3\n\
    }\n";

#[test]
fn line_map_carries_unchanged_lines_past_an_insertion() {
    let old = lines(ORIGINAL);
    let new = lines(DOCUMENTED);
    let map = unchanged_line_map(&old, &new);
    // `pub fn alpha` 3 -> 4, `pub fn beta` 7 -> 9, `pub fn gamma` 11 -> 14.
    assert_eq!(map[2], Some(3));
    assert_eq!(map[6], Some(8));
    assert_eq!(map[10], Some(13));
    assert_eq!(map[0], Some(0), "lines before the edit stay put");
}

#[test]
fn anchor_prefers_the_cited_line_then_the_nearest() {
    let l = lines(DOCUMENTED);
    assert_eq!(
        citation_anchor(&l, &cite("a.rs", 9, 9, Some("beta"))),
        Some(9)
    );
    // Cited 7 (the pre-edit line): `beta` is 2 lines off, inside tolerance.
    assert_eq!(
        citation_anchor(&l, &cite("a.rs", 7, 7, Some("beta"))),
        Some(9)
    );
    // Nothing named `delta` near line 7.
    assert_eq!(
        citation_anchor(&l, &cite("a.rs", 7, 7, Some("delta"))),
        None
    );
    // Out of range.
    assert_eq!(
        citation_anchor(&l, &cite("a.rs", 99, 99, Some("beta"))),
        None
    );
}

/// The c24 shape: notes written against the original file, then doc
/// comments inserted above the cited functions.
#[test]
fn a_reference_written_before_the_edit_moves_with_its_function() {
    let original = lines(ORIGINAL);
    let current = lines(DOCUMENTED);
    let versions: Vec<&[String]> = vec![&original];
    assert_eq!(
        moved_citation_range(&cite("a.rs", 11, 11, Some("gamma")), &versions, &current),
        Some((14, 14))
    );
    assert_eq!(
        moved_citation_range(&cite("a.rs", 3, 3, Some("alpha")), &versions, &current),
        Some((4, 4))
    );
    // A range moves as a whole.
    assert_eq!(
        moved_citation_range(&cite("a.rs", 7, 9, Some("beta")), &versions, &current),
        Some((9, 11))
    );
}

#[test]
fn a_reference_that_is_exact_now_or_was_never_right_is_not_reported() {
    let original = lines(ORIGINAL);
    let current = lines(DOCUMENTED);
    let versions: Vec<&[String]> = vec![&original];
    // Already refreshed.
    assert_eq!(
        moved_citation_range(&cite("a.rs", 14, 14, Some("gamma")), &versions, &current),
        None
    );
    // Wrong from the start (no version has `gamma` near line 2): the
    // completion gate's job, not a moved reference.
    assert_eq!(
        moved_citation_range(&cite("a.rs", 2, 2, Some("gamma")), &versions, &current),
        None
    );
    // No edit: nothing moved.
    let same: Vec<&[String]> = vec![&current];
    assert_eq!(
        moved_citation_range(&cite("a.rs", 9, 9, Some("beta")), &same, &current),
        None
    );
}

#[test]
fn a_cited_line_the_edit_rewrote_is_not_guessed() {
    let original = lines(ORIGINAL);
    let rewritten = lines(&ORIGINAL.replace("pub fn beta() -> u32 {", "pub fn beta() -> u64 {"));
    let current_with_shift = {
        let mut v = vec!["// header".to_string()];
        v.extend(rewritten);
        v
    };
    let versions: Vec<&[String]> = vec![&original];
    assert_eq!(
        moved_citation_range(
            &cite("a.rs", 7, 7, Some("beta")),
            &versions,
            &current_with_shift
        ),
        None,
        "the cited line itself changed: no exact mapping, nothing reported"
    );
}

/// Several edits in one batch: the pre-edit state of the LAST edit has
/// `gamma` within tolerance of the stale line; the exact match in the
/// original must win, or the reported line would be off by the earlier
/// edits (97 for 100 in the live c24 case).
#[test]
fn the_exact_version_wins_over_a_tolerance_match_in_a_later_one() {
    let original = lines(ORIGINAL);
    let after_two = lines(&DOCUMENTED.replace("/// Gamma.\n", "").to_string());
    let current = lines(DOCUMENTED);
    let versions: Vec<&[String]> = vec![&original, &after_two];
    // In `after_two`, `gamma` sits at 13: within tolerance of 11, not exact.
    assert_eq!(
        moved_citation_range(&cite("a.rs", 11, 11, Some("gamma")), &versions, &current),
        Some((14, 14))
    );
}

#[test]
fn watch_maps_prose_references_in_a_written_note() {
    let ws = tempfile::tempdir().unwrap();
    let src = ws.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(ws.path().join("docs")).unwrap();
    let lib = src.join("lib.rs");
    std::fs::write(&lib, ORIGINAL).unwrap();

    let mut watch = StaleCitationWatch::default();
    watch.record_pre_mutation(&lib);
    // Same content again: not a new version.
    watch.record_pre_mutation(&lib);
    std::fs::write(&lib, DOCUMENTED).unwrap();

    let notes = "# Notes\n\n## src/lib.rs\n\n\
                 - `alpha` (line 3) - first.\n\
                 - `beta` (line 7) - second.\n\
                 - `gamma` (line 14) - already current.\n";
    let mut resolver = CitationResolver::new(ws.path());
    let moved = moved_citations_in(&mut resolver, &watch, "docs/NOTES.md", notes);
    let described: Vec<String> = moved.iter().map(MovedCitation::describe).collect();
    assert_eq!(
        described,
        vec![
            "docs/NOTES.md cites src/lib.rs:3 `alpha` — now at :4 after your edit".to_string(),
            "docs/NOTES.md cites src/lib.rs:7 `beta` — now at :9 after your edit".to_string(),
        ]
    );

    // Reported once.
    let fresh = watch.take_unreported(moved.clone());
    assert_eq!(fresh.len(), 2);
    assert!(watch.take_unreported(moved).is_empty());

    let notice = stale_citations_notice(&fresh);
    assert!(notice.starts_with("[POLICY kind=stale_citations retryable=true"));
    assert!(
        notice.contains("src/lib.rs:7 `beta` — now at :9"),
        "{notice}"
    );
    assert!(notice.contains("Update these line numbers in docs/NOTES.md"));
}

#[test]
fn watch_keeps_the_first_version_when_capped() {
    let ws = tempfile::tempdir().unwrap();
    let f = ws.path().join("f.rs");
    let mut watch = StaleCitationWatch::default();
    for i in 0..(MAX_VERSIONS_PER_FILE + 5) {
        std::fs::write(&f, format!("fn v{i}() {{}}\n")).unwrap();
        watch.record_pre_mutation(&f);
    }
    let versions = watch.versions(&std::fs::canonicalize(&f).unwrap());
    assert_eq!(versions.len(), MAX_VERSIONS_PER_FILE);
    assert_eq!(versions[0][0], "fn v0() {}");
    assert_eq!(
        versions.last().unwrap()[0],
        format!("fn v{}() {{}}", MAX_VERSIONS_PER_FILE + 4)
    );
}

/// End to end through the dispatcher: the agent writes notes citing
/// `lib.rs`, then a later batch inserts doc comments. The batch that moved
/// the functions ends with one notice naming each moved reference; a batch
/// that moves nothing new adds none.
#[tokio::test]
async fn dispatcher_reports_references_an_edit_moved_once() {
    use crate::testing::mock_api::MockLlmServer;
    let _g = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder().with_response("done").build().await;
    let mut config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    // The post-edit type check is not under test (and runs a compiler).
    config.agent.verify_after_edit = Some(false);
    let mut agent = crate::agent::Agent::new(config).await.unwrap();

    let ws = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(ws.path()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("docs")).unwrap();
    let lib = root.join("src/lib.rs");
    std::fs::write(&lib, ORIGINAL).unwrap();
    agent
        .tools
        .set_workspace_root(crate::tools::workspace_root::WorkspaceRoot::fixed(&root));
    agent.current_checkpoint = Some(crate::checkpoint::TaskCheckpoint::new(
        "t".to_string(),
        "document lib.rs".to_string(),
    ));

    let notes_path = root.join("docs/NOTES.md");
    let notes = "## src/lib.rs\n\n- `alpha` (line 3) - first.\n- `gamma` (line 11) - third.\n";
    agent
        .execute_tool_batch(vec![(
            "file_write".to_string(),
            serde_json::json!({"path": notes_path.to_str().unwrap(), "content": notes}).to_string(),
            None,
        )])
        .await
        .unwrap();
    let notices = |agent: &crate::agent::Agent| {
        agent
            .messages
            .iter()
            .filter(|m| m.content.text().contains("STALE LINE REFERENCES"))
            .count()
    };
    assert_eq!(notices(&agent), 0, "writing the notes moves nothing");

    agent
        .execute_tool_batch(vec![(
            "file_write".to_string(),
            serde_json::json!({"path": lib.to_str().unwrap(), "content": DOCUMENTED}).to_string(),
            None,
        )])
        .await
        .unwrap();
    assert_eq!(notices(&agent), 1);
    let notice = agent.messages.last().unwrap().content.text().to_string();
    assert!(
        notice.contains("docs/NOTES.md cites src/lib.rs:3 `alpha` — now at :4 after your edit"),
        "{notice}"
    );
    assert!(
        notice.contains("src/lib.rs:11 `gamma` — now at :14"),
        "{notice}"
    );

    // Another mutation that moves nothing new: no second notice.
    agent
        .execute_tool_batch(vec![(
            "file_write".to_string(),
            serde_json::json!({"path": root.join("other.txt").to_str().unwrap(), "content": "x"})
                .to_string(),
            None,
        )])
        .await
        .unwrap();
    assert_eq!(notices(&agent), 1, "each moved reference is reported once");
    server.stop().await;
}

#[test]
fn parse_then_map_matches_the_c24_notes_format() {
    // The live notes' exact shape: a section heading naming the file and
    // "`name` (line N) - description" bullets.
    let text = "## src/agent/context.rs\n\n- `compression_threshold` (line 96) - Return it.\n";
    let c = parse_citations(text);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].path, "src/agent/context.rs");
    assert_eq!(c[0].start, 96);
    assert_eq!(c[0].symbol.as_deref(), Some("compression_threshold"));
}
