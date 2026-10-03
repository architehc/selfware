use super::*;

// ── Introspection tool path policy (2026-09-21 review sweep) ─────────────
//
// code_query.scope, code_plan.codebase_root, code_diff_plan's
// target_file/codebase_root — and code_introspect's target — walk and read
// the filesystem; each must obey the same workspace path policy as
// file_read BEFORE any filesystem access.

fn is_path_policy_error(msg: &str) -> bool {
    msg.contains("not allowed")
        || msg.contains("not in allowed")
        || msg.contains("outside working")
        || msg.contains("protected")
        || msg.contains("denied pattern")
}

#[tokio::test]
async fn test_code_query_scope_policy() {
    let tool = CodeQuery::new();
    let err = tool
        .execute(serde_json::json!({"query": "config", "scope": "/etc"}))
        .await
        .unwrap_err();
    assert!(
        is_path_policy_error(&err.to_string()),
        "code_query scope outside the workspace must be refused, got: {err}"
    );
}

#[tokio::test]
async fn test_code_plan_codebase_root_policy() {
    let tool = CodePlan::new();
    let err = tool
        .execute(serde_json::json!({"goal": "g", "codebase_root": "/etc"}))
        .await
        .unwrap_err();
    assert!(
        is_path_policy_error(&err.to_string()),
        "code_plan codebase_root outside the workspace must be refused, got: {err}"
    );
}

#[tokio::test]
async fn test_code_diff_plan_paths_policy() {
    let tool = CodeDiffPlan::new();
    for args in [
        serde_json::json!({"target_file": "/etc/passwd", "change_type": "modify"}),
        serde_json::json!({"target_file": "src/main.rs", "change_type": "modify", "codebase_root": "/etc"}),
    ] {
        let err = tool.execute(args).await.unwrap_err();
        assert!(
            is_path_policy_error(&err.to_string()),
            "code_diff_plan out-of-workspace path must be refused, got: {err}"
        );
    }
}

#[tokio::test]
async fn test_code_introspect_target_policy() {
    let tool = CodeIntrospect::new();
    let err = tool
        .execute(serde_json::json!({"target": "/etc"}))
        .await
        .unwrap_err();
    assert!(
        is_path_policy_error(&err.to_string()),
        "code_introspect target outside the workspace must be refused, got: {err}"
    );
}

#[tokio::test]
async fn test_code_query_scope_in_workspace_ok() {
    let tool = CodeQuery::new();
    // An in-workspace scope passes the path policy; any error that surfaces
    // must be a search/build error, not a path-policy refusal.
    let result = tool
        .execute(serde_json::json!({"query": "fn main", "scope": "src"}))
        .await;
    match result {
        Ok(_) => {}
        Err(e) => assert!(
            !is_path_policy_error(&e.to_string()),
            "in-workspace scope must not trip the path policy, got: {e}"
        ),
    }
}

#[tokio::test]
async fn relative_introspection_paths_follow_the_active_workspace() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("scoped_only.rs"),
        "pub fn scoped_anchor_symbol() {}\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("caller.rs"),
        "pub fn caller() { scoped_only::scoped_anchor_symbol(); }\n",
    )
    .unwrap();
    let root = crate::tools::workspace_root::WorkspaceRoot::fixed(dir.path());
    let expected_target = dir.path().join("scoped_only.rs");
    let expected_caller = dir.path().join("caller.rs");

    crate::tools::workspace_root::scope(root, async {
        let outline: IntrospectResult = serde_json::from_value(
            CodeIntrospect::new()
                .execute(json!({"target": ".", "max_tokens": 20_000}))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(outline.coverage.files_total, 2);
        assert!(outline.content.contains("scoped_anchor_symbol"));

        let query = CodeQuery::new()
            .execute(json!({"query": "scoped_anchor_symbol", "scope": "."}))
            .await
            .unwrap();
        let query_file = std::path::Path::new(query["results"][0]["file"].as_str().unwrap());
        assert!(query_file.starts_with(dir.path()), "{query}");
        assert_eq!(query_file.file_name().unwrap(), "scoped_only.rs");

        let plan = CodePlan::new()
            .execute(json!({
                "goal": "modify scoped_anchor_symbol",
                "codebase_root": "."
            }))
            .await
            .unwrap();
        let rendered_plan = serde_json::to_string(&plan).unwrap();
        assert!(rendered_plan.contains(&dir.path().to_string_lossy().to_string()));
        assert!(rendered_plan.contains("scoped_only.rs"));

        let impact = CodeDiffPlan::new()
            .execute(json!({
                "target_file": "scoped_only.rs",
                "change_type": "modify",
                "affected_symbol": "scoped_anchor_symbol",
                "codebase_root": "."
            }))
            .await
            .unwrap();
        assert_eq!(
            impact["target_file"],
            expected_target.to_string_lossy().as_ref()
        );
        let caller = std::path::Path::new(impact["direct_callers"][0]["file"].as_str().unwrap());
        assert!(caller.starts_with(dir.path()), "{impact}");
        assert_eq!(
            caller.file_name().unwrap(),
            expected_caller.file_name().unwrap()
        );
    })
    .await;
}

// ── Recursive-walk validation (2026-09-21 review, P2) ────────────────────
//
// The introspection walkers validated only the ROOT target: denied source
// files nested under an allowed directory, and symlinks escaping the
// workspace, were discovered and READ unvalidated. Every candidate the walk
// will read must pass the same workspace path policy as the root.

fn scoped_config(allowed: String, denied: Vec<String>) -> SafetyConfig {
    SafetyConfig {
        allowed_paths: vec![allowed],
        denied_paths: denied,
        ..SafetyConfig::default()
    }
}

/// A denied source file nested under an allowed directory must be refused —
/// the walk discovers it, validation refuses it, and nothing is read.
#[tokio::test]
async fn test_code_introspect_refuses_denied_file_nested_under_allowed_dir() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("allowed");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("ok.rs"), "pub fn ok() {}\n").unwrap();
    let dir_str = dir.path().to_string_lossy().to_string();

    // A denied file with a source extension, two levels below the allowed
    // root: under the old walk only `target` was validated, so this would
    // have been read.
    std::fs::write(
        root.join("sub").join("secret_calc.rs"),
        "pub fn secret() {}\n",
    )
    .unwrap();

    let config = scoped_config(
        format!("{dir_str}/**"),
        vec!["**/secret_calc.rs".to_string()],
    );
    let tool = CodeIntrospect::with_safety_config(config);
    let target = root.to_string_lossy().to_string();
    let err = tool
        .execute(serde_json::json!({"target": target, "depth": "full"}))
        .await
        .unwrap_err();
    assert!(
        is_path_policy_error(&err.to_string()),
        "a denied file nested under an allowed directory must be refused, got: {err}"
    );
    assert!(
        err.to_string().contains("denied"),
        "the refusal should name the denied pattern, got: {err}"
    );
}

/// A symlink inside the allowed tree pointing OUTSIDE the workspace must be
/// refused — containment is enforced per candidate, so the target is never
/// read.
#[cfg(unix)]
#[tokio::test]
async fn test_code_introspect_refuses_symlink_escaping_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let allowed = dir.path().join("allowed");
    std::fs::create_dir_all(&allowed).unwrap();
    std::fs::write(allowed.join("ok.rs"), "pub fn ok() {}\n").unwrap();

    // A real file OUTSIDE the allowed root (but inside the temp dir), plus
    // a source-extension symlink inside the allowed tree pointing at it.
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("leak_target.rs"), "pub fn leaked() {}\n").unwrap();
    std::os::unix::fs::symlink(outside.join("leak_target.rs"), allowed.join("evil.rs")).unwrap();

    let dir_str = dir.path().to_string_lossy().to_string();
    let config = scoped_config(format!("{dir_str}/allowed/**"), vec![]);
    let tool = CodeIntrospect::with_safety_config(config);
    let target = allowed.to_string_lossy().to_string();
    let err = tool
        .execute(serde_json::json!({"target": target, "depth": "full"}))
        .await
        .unwrap_err();
    assert!(
        is_path_policy_error(&err.to_string()),
        "a symlink escaping the allowed workspace must be refused, got: {err}"
    );
}

/// Control: an allowed directory holding only benign source files must not
/// trip the path policy — per-candidate validation must not over-block.
#[tokio::test]
async fn test_code_introspect_allowed_tree_with_benign_files_is_not_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("allowed");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("ok.rs"), "pub fn ok() {}\n").unwrap();
    std::fs::write(root.join("sub").join("helper.rs"), "pub fn helper() {}\n").unwrap();

    let dir_str = dir.path().to_string_lossy().to_string();
    let config = scoped_config(format!("{dir_str}/**"), vec![]);
    let tool = CodeIntrospect::with_safety_config(config);
    let target = root.to_string_lossy().to_string();
    let result = tool
        .execute(serde_json::json!({"target": target, "depth": "full"}))
        .await;
    match result {
        Ok(_) => {}
        Err(e) => assert!(
            !is_path_policy_error(&e.to_string()),
            "an allowed tree must not trip the path policy, got: {e}"
        ),
    }
}

/// code_query walks the same way: a denied source file nested under an
/// allowed scope must be refused there too.
#[tokio::test]
async fn test_code_query_refuses_denied_file_nested_under_allowed_scope() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("allowed");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("ok.rs"), "pub fn ok() {}\n").unwrap();
    std::fs::write(root.join("secret_query.rs"), "pub fn secret() {}\n").unwrap();

    let dir_str = dir.path().to_string_lossy().to_string();
    let config = scoped_config(
        format!("{dir_str}/**"),
        vec!["**/secret_query.rs".to_string()],
    );
    let tool = CodeQuery::with_safety_config(config);
    let scope = root.to_string_lossy().to_string();
    let err = tool
        .execute(serde_json::json!({"query": "secret", "scope": scope}))
        .await
        .unwrap_err();
    assert!(
        is_path_policy_error(&err.to_string()),
        "code_query must refuse a denied file inside the scope, got: {err}"
    );
}

// ── Budget, auto-depth, coverage and ranking (2026-09 introspect review) ──

/// A Rust source with `n` public functions with long signatures (plus one
/// private one, so signatures and full depth differ).
fn rust_source(prefix: &str, n: usize) -> String {
    let mut s = String::from("use std::collections::HashMap;\n\n");
    for i in 0..n {
        s.push_str(&format!(
            "pub fn {prefix}_function_{i}(first_argument: HashMap<String, Vec<u64>>, \
             second_argument: Option<&str>) -> Result<Vec<String>, std::io::Error> {{ todo!() }}\n"
        ));
    }
    s.push_str(&format!("fn {prefix}_private_helper() {{}}\n"));
    s
}

fn fixture(files: &[(&str, String)]) -> (tempfile::TempDir, String, CodeIntrospect) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("src");
    std::fs::create_dir_all(&root).unwrap();
    for (name, body) in files {
        std::fs::write(root.join(name), body).unwrap();
    }
    let config = scoped_config(format!("{}/**", dir.path().to_string_lossy()), vec![]);
    let tool = CodeIntrospect::with_safety_config(config);
    (dir, root.to_string_lossy().to_string(), tool)
}

async fn introspect(tool: &CodeIntrospect, args: Value) -> IntrospectResult {
    serde_json::from_value(tool.execute(args).await.unwrap()).unwrap()
}

/// Every reported number equals what was rendered, and the content never
/// exceeds max_tokens (finding 1).
fn assert_numbers_match_render(result: &IntrospectResult, max_tokens: usize, format: &str) {
    assert!(
        result.tokens_used <= max_tokens,
        "tokens_used {} exceeds max_tokens {max_tokens}",
        result.tokens_used
    );
    assert_eq!(
        result.tokens_used,
        estimate_content_tokens(&result.content),
        "tokens_used must be the measured tokens of the returned content"
    );
    assert_eq!(result.tokens_remaining, max_tokens - result.tokens_used);
    let renderer = OutputRenderer::new(format);
    // Rebuild the entries (rendered lines are not serialized) and check
    // each file's reported tokens against its entry as it appears.
    let files: Vec<FileInfo> = result.files_included.clone();
    let flags = renderer.last_in_group_flags(&files);
    let mut entry_tokens = 0;
    for (i, f) in files.iter().enumerate() {
        let body = std::fs::read_to_string(&f.path).unwrap();
        let parsed = parser::parse(&body, Language::detect(Path::new(&f.path), None));
        let depth = Depth::parse(&f.depth).unwrap();
        let lines: Vec<String> = parser::extract_at_depth(&parsed, &depth)
            .iter()
            .map(|s| render::symbol_line(s, &depth))
            .take(f.symbols.len())
            .collect();
        let mut rebuilt = f.clone();
        rebuilt.rendered_lines = lines;
        let block = renderer.file_block(&rebuilt, i, flags[i]);
        assert!(
            result.content.contains(&block),
            "entry for {} not in content",
            f.path
        );
        assert_eq!(
            f.tokens,
            estimate_content_tokens(&block),
            "per-file tokens for {}",
            f.path
        );
        entry_tokens += f.tokens;
    }
    assert!(entry_tokens <= result.tokens_used);
    assert_eq!(
        result.coverage.symbols_included,
        files.iter().map(|f| f.symbols.len()).sum::<usize>()
    );
}

#[tokio::test]
async fn test_introspect_budget_is_a_hard_limit_on_rendered_content() {
    let files: Vec<(String, String)> = (0..6)
        .map(|i| (format!("m{i}.rs"), rust_source(&format!("m{i}"), 30)))
        .collect();
    let refs: Vec<(&str, String)> = files.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    let (_dir, target, tool) = fixture(&refs);

    for format in ["tree", "flat"] {
        let result = introspect(
            &tool,
            json!({"target": target, "depth": "signatures", "max_tokens": 700, "format": format}),
        )
        .await;
        assert_numbers_match_render(&result, 700, format);
        // Symbols of ALL candidate files are counted — coverage is partial,
        // not structurally 100% (finding 3).
        assert_eq!(result.coverage.files_total, 6);
        assert_eq!(result.coverage.symbols_total, 6 * 30);
        assert!(result.coverage.symbols_included < result.coverage.symbols_total);
        assert!(result.coverage.symbols_coverage_pct < 100.0);
        assert!(result.coverage.is_partial());
        assert!(
            result
                .suggestions
                .iter()
                .any(|s| s.starts_with("Partial coverage")),
            "partial coverage must be named: {:?}",
            result.suggestions
        );
        assert!(result.content.contains("not a read of the files' contents"));
    }
}

#[tokio::test]
async fn test_introspect_truncates_a_file_that_does_not_fit_whole() {
    let (_dir, target, tool) = fixture(&[("big.rs", rust_source("big", 120))]);
    let file = format!("{target}/big.rs");
    let result = introspect(
        &tool,
        json!({"target": file, "depth": "signatures", "max_tokens": 600}),
    )
    .await;
    assert_numbers_match_render(&result, 600, "tree");
    assert_eq!(result.coverage.files_included, 1);
    assert_eq!(result.coverage.files_truncated, 1);
    let f = &result.files_included[0];
    assert!(f.symbols_omitted > 0);
    assert_eq!(f.symbols.len() + f.symbols_omitted, 120);
    assert!(result.content.contains(&format!(
        "… {} more symbols omitted (token budget)",
        f.symbols_omitted
    )));
}

#[tokio::test]
async fn test_introspect_rejects_a_budget_below_the_result_frame() {
    let (_dir, target, tool) = fixture(&[("a.rs", rust_source("a", 3))]);
    let err = tool
        .execute(json!({"target": target, "max_tokens": 5}))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("raise max_tokens"), "{err}");
}

/// Auto-depth is chosen from measured sizes of the collected files: a
/// small set gets signatures, a set whose signatures do not fit but whose
/// overview does gets overview (finding 2 — it was a constant Signatures).
#[tokio::test]
async fn test_introspect_auto_depth_depends_on_measured_size() {
    let (_small_dir, small, small_tool) =
        fixture(&[("a.rs", rust_source("a", 3)), ("b.rs", rust_source("b", 3))]);
    let result = introspect(&small_tool, json!({"target": small, "max_tokens": 8000})).await;
    assert!(!result.coverage.is_partial());
    assert!(result
        .files_included
        .iter()
        .all(|f| f.depth == "signatures"));

    let files: Vec<(String, String)> = (0..5)
        .map(|i| (format!("l{i}.rs"), rust_source(&format!("l{i}"), 25)))
        .collect();
    let refs: Vec<(&str, String)> = files.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    let (_large_dir, large, large_tool) = fixture(&refs);
    let huge = 1_000_000;
    let sig = introspect(
        &large_tool,
        json!({"target": large, "depth": "signatures", "max_tokens": huge}),
    )
    .await
    .tokens_used;
    let ov = introspect(
        &large_tool,
        json!({"target": large, "depth": "overview", "max_tokens": huge}),
    )
    .await
    .tokens_used;
    assert!(
        ov < sig,
        "fixture: overview ({ov}) must be cheaper than signatures ({sig})"
    );
    let budget = ov + (sig - ov) / 2;
    let result = introspect(&large_tool, json!({"target": large, "max_tokens": budget})).await;
    assert!(result.files_included.iter().all(|f| f.depth == "overview"));
    assert!(
        !result.coverage.is_partial(),
        "overview fits whole at {budget}"
    );
    assert_numbers_match_render(&result, budget, "tree");
}

/// Partial coverage is named at overview too, and above 50% (finding 4).
#[tokio::test]
async fn test_introspect_partial_coverage_warned_at_overview() {
    let files: Vec<(String, String)> = (0..4)
        .map(|i| (format!("o{i}.rs"), rust_source(&format!("o{i}"), 40)))
        .collect();
    let refs: Vec<(&str, String)> = files.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    let (_dir, target, tool) = fixture(&refs);
    let full = introspect(
        &tool,
        json!({"target": target, "depth": "overview", "max_tokens": 1_000_000}),
    )
    .await
    .tokens_used;
    // Just short of everything: coverage is high but not 100%.
    let result = introspect(
        &tool,
        json!({"target": target, "depth": "overview", "max_tokens": full - 10}),
    )
    .await;
    assert!(result.coverage.is_partial());
    assert!(result.coverage.symbols_coverage_pct > 50.0);
    assert!(
        result
            .suggestions
            .iter()
            .any(|s| s.starts_with("Partial coverage")
                && s.contains("symbols omitted")
                && s.contains("'overview'")),
        "{:?}",
        result.suggestions
    );
}

/// The query decides the order (finding 5): `rank_files` recomputes BM25
/// from the query, so different queries put different files first.
#[tokio::test]
async fn test_introspect_query_changes_order() {
    let a = "pub fn zebra_striping() {}\npub fn zebra_count() -> usize { 0 }\npub fn other() {}\n";
    let b = "pub fn parse_config() {}\npub fn parse_args() {}\npub fn other_b() {}\n";
    let (_dir, target, tool) = fixture(&[("a.rs", a.to_string()), ("b.rs", b.to_string())]);
    let first = |r: &IntrospectResult| {
        Path::new(&r.files_included[0].path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string()
    };
    let none = introspect(&tool, json!({"target": target, "depth": "signatures"})).await;
    assert_eq!(first(&none), "a.rs");
    let parse = introspect(
        &tool,
        json!({"target": target, "depth": "signatures", "query": "parse"}),
    )
    .await;
    assert_eq!(first(&parse), "b.rs");
    let zebra = introspect(
        &tool,
        json!({"target": target, "depth": "signatures", "query": "zebra"}),
    )
    .await;
    assert_eq!(first(&zebra), "a.rs");
}

/// code_query (Rule 5 sweep): results come back most relevant first, and
/// `total_matches` counts every matching file, not just the returned ones.
#[tokio::test]
async fn test_code_query_orders_by_relevance_and_counts_all_matches() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("src");
    std::fs::create_dir_all(&root).unwrap();
    // a.rs sorts first but matches weakly; b.rs matches strongly.
    std::fs::write(root.join("a.rs"), "pub fn load() -> Config { todo!() }\n").unwrap();
    std::fs::write(
        root.join("b.rs"),
        "pub fn config_load() {}\npub fn config_save() {}\npub struct Config;\n",
    )
    .unwrap();
    let config = scoped_config(format!("{}/**", dir.path().to_string_lossy()), vec![]);
    let tool = CodeQuery::with_safety_config(config);
    let out = tool
        .execute(json!({"query": "config", "scope": root.to_string_lossy(), "max_results": 1}))
        .await
        .unwrap();
    assert_eq!(out["total_matches"], 2);
    assert_eq!(out["files_returned"], 1);
    let file = out["results"][0]["file"].as_str().unwrap();
    assert!(file.ends_with("b.rs"), "most relevant first, got {file}");
    let results = serde_json::to_string(&out["results"]).unwrap();
    assert_eq!(out["tokens_used"], estimate_content_tokens(&results));
}

// ── The walk that defines "the repo" (review 2026-09-27) ─────────────────
//
// A depth cap of 3 and a 10-extension list shrank the denominator silently:
// coverage read 100% of a subset. Deep layouts must be walked, the language
// table is the repository inventory's, and anything the bound does cut off
// is counted.

fn walk_fixture() -> (tempfile::TempDir, CodeIntrospect) {
    let dir = tempfile::tempdir().unwrap();
    let config = scoped_config(format!("{}/**", dir.path().to_string_lossy()), vec![]);
    (dir, CodeIntrospect::with_safety_config(config))
}

#[tokio::test]
async fn introspect_walks_deep_layouts_and_every_code_language() {
    let (dir, tool) = walk_fixture();
    let root = dir.path();
    let files = [
        "src/main/java/com/example/app/service/Billing.java",
        "packages/ui/src/components/button/index.tsx",
        "packages/ui/src/components/button/hooks.jsx",
        "app/src/main/kotlin/com/example/Main.kt",
        "lib/models/user.rb",
        "src/lib.rs",
    ];
    for f in files {
        let p = root.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "fn f() {}\n").unwrap();
    }
    // Docs and config are not code.
    std::fs::write(root.join("README.md"), "# readme\n").unwrap();
    std::fs::write(root.join("Cargo.toml"), "[package]\n").unwrap();
    let result = introspect(
        &tool,
        json!({"target": root.to_string_lossy(), "max_tokens": 20000}),
    )
    .await;
    assert_eq!(
        result.coverage.files_total,
        files.len(),
        "every code file at any depth, in any inventory language, is found"
    );
    assert_eq!(result.coverage.dirs_not_walked, 0);
}

#[tokio::test]
async fn introspect_counts_directories_below_the_walk_bound() {
    let (dir, tool) = walk_fixture();
    let mut deep = dir.path().to_path_buf();
    for i in 0..(MAX_WALK_DEPTH + 2) {
        deep = deep.join(format!("d{i}"));
    }
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(deep.join("hidden.rs"), "fn hidden() {}\n").unwrap();
    std::fs::write(dir.path().join("top.rs"), "fn top() {}\n").unwrap();
    let result = introspect(
        &tool,
        json!({"target": dir.path().to_string_lossy(), "max_tokens": 20000}),
    )
    .await;
    assert_eq!(
        result.coverage.files_total, 1,
        "only top.rs is within the bound"
    );
    assert!(result.coverage.dirs_not_walked >= 1);
    assert!(
        result.coverage.is_partial(),
        "a directory the walk did not enter makes coverage partial, never 100%"
    );
    assert!(
        result
            .suggestions
            .iter()
            .any(|s| s.contains("were not walked")),
        "the cut-off is named: {:?}",
        result.suggestions
    );
}

// ── One walk for every introspection tool (review 2026-09-27, on v0.9.4) ──
//
// The skip list only ran inside the recursive helper, so the target's own
// node_modules/.venv/target were walked; symlinked directories were
// followed with no visited set; graph output claimed symbols it never
// rendered and did not pay for its edges.

fn write_file(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

fn dependency_tree_fixture() -> (tempfile::TempDir, CodeIntrospect) {
    let (dir, tool) = walk_fixture();
    let root = dir.path();
    write_file(root, "src/lib.rs", "pub fn real() {}\n");
    // Top-level dependency / build / environment trees.
    write_file(
        root,
        "node_modules/left-pad/index.js",
        "function pad() {}\n",
    );
    write_file(root, "target/debug/build/gen.rs", "fn gen() {}\n");
    write_file(root, ".venv/pyvenv.cfg", "home = /usr\n");
    write_file(
        root,
        ".venv/lib/python3.12/site-packages/requests/api.py",
        "def get(): pass\n",
    );
    // A virtualenv under any name is recognised by its pyvenv.cfg.
    write_file(root, "env/pyvenv.cfg", "home = /usr\n");
    write_file(root, "env/lib/site-packages/six.py", "def f(): pass\n");
    write_file(root, "scratchpad/copy/src/lib.rs", "pub fn dup() {}\n");
    for (dir, function) in [
        (".claude", "claude_private"),
        (".codex", "codex_private"),
        (".agents", "agents_private"),
        (".qwen", "qwen_private"),
        (".superpowers", "superpowers_private"),
    ] {
        write_file(
            root,
            &format!("{dir}/state/private.rs"),
            &format!("pub fn {function}() {{}}\n"),
        );
    }
    (dir, tool)
}

#[tokio::test]
async fn introspect_skips_dependency_trees_at_the_top_level() {
    let (dir, tool) = dependency_tree_fixture();
    let result = introspect(
        &tool,
        json!({"target": dir.path().to_string_lossy(), "max_tokens": 20000}),
    )
    .await;
    let paths: Vec<&str> = result
        .files_included
        .iter()
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(
        result.coverage.files_total, 1,
        "only src/lib.rs is project code, got {paths:?}"
    );
    assert!(paths[0].ends_with("src/lib.rs"));
    assert!(paths.iter().all(|path| {
        ![".claude", ".codex", ".agents", ".qwen", ".superpowers"]
            .iter()
            .any(|dir| path.contains(dir))
    }));
    assert!(!result.coverage.gitignore_applied, "not a git work tree");
}

#[tokio::test]
async fn code_query_uses_the_same_skips() {
    let (dir, _) = dependency_tree_fixture();
    let config = scoped_config(format!("{}/**", dir.path().to_string_lossy()), vec![]);
    let tool = CodeQuery::with_safety_config(config);
    let out = tool
        .execute(
            json!({
                "query": "get pad gen real dup six claude_private codex_private agents_private qwen_private superpowers_private",
                "scope": dir.path().to_string_lossy()
            }),
        )
        .await
        .unwrap();
    let results = serde_json::to_string(&out["results"]).unwrap();
    assert!(results.contains("real"), "{results}");
    for foreign in [
        "pad",
        "\"get\"",
        "gen",
        "dup",
        "six",
        "claude_private",
        "codex_private",
        "agents_private",
        "qwen_private",
        "superpowers_private",
    ] {
        assert!(
            !results.contains(foreign),
            "{foreign} comes from a skipped tree: {results}"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn introspection_never_traverses_directory_symlinks() {
    let (dir, tool) = walk_fixture();
    let root = dir.path();
    write_file(root, "src/lib.rs", "pub fn alpha_walk() {}\n");
    write_file(root, "src/nested/b.rs", "pub fn beta_walk() {}\n");
    write_file(
        root,
        "outside/secret.rs",
        "pub fn symlink_only_secret() {}\n",
    );
    // src/nested/up -> .. (src): a cycle inside the workspace.
    std::os::unix::fs::symlink("..", root.join("src/nested/up")).unwrap();
    // A normal-looking alias to a source directory outside the selected root
    // must not make that source an implicit input.
    std::os::unix::fs::symlink("../outside", root.join("src/alias")).unwrap();
    let result = introspect(
        &tool,
        json!({"target": root.join("src").to_string_lossy(), "max_tokens": 20000}),
    )
    .await;
    assert_eq!(
        result.coverage.files_total,
        2,
        "only real files beneath the selected root: {:?}",
        result
            .files_included
            .iter()
            .map(|f| &f.path)
            .collect::<Vec<_>>()
    );
    assert_eq!(result.coverage.dirs_not_walked, 0);
    assert!(!result.content.contains("symlink_only_secret"));

    let config = scoped_config(format!("{}/**", root.to_string_lossy()), vec![]);
    let out = CodeQuery::with_safety_config(config.clone())
        .execute(json!({
            "query": "alpha_walk beta_walk symlink_only_secret",
            "scope": root.join("src").to_string_lossy()
        }))
        .await
        .unwrap();
    assert_eq!(out["total_matches"], 2, "code_query skips every symlink");
    assert!(!serde_json::to_string(&out["results"])
        .unwrap()
        .contains("symlink_only_secret"));

    let plan = CodePlan::with_safety_config(config.clone())
        .execute(json!({
            "goal": "fix symlink_only_secret",
            "codebase_root": root.join("src").to_string_lossy()
        }))
        .await
        .unwrap();
    assert!(plan["phases"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|phase| phase["target"].as_str())
        .all(|target| !target.contains("alias") && !target.contains("secret.rs")));

    let impact = CodeDiffPlan::with_safety_config(config)
        .execute(json!({
            "target_file": root.join("src/lib.rs").to_string_lossy(),
            "change_type": "modify",
            "affected_symbol": "symlink_only_secret",
            "codebase_root": root.join("src").to_string_lossy()
        }))
        .await
        .unwrap();
    assert_eq!(impact["estimated_files_to_update"], 0);
    assert_eq!(impact["direct_callers"], json!([]));
    assert_eq!(impact["tests_affected"], json!([]));
}

#[tokio::test]
async fn introspect_respects_gitignore_in_a_git_work_tree() {
    let (dir, tool) = walk_fixture();
    let root = dir.path();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
    };
    match git(&["init", "-q"]) {
        Ok(o) if o.status.success() => {}
        _ => return, // git unavailable: nothing to assert
    }
    write_file(root, ".gitignore", "generated/\n");
    write_file(root, "src/lib.rs", "pub fn a() {}\n");
    write_file(root, "generated/big.rs", "pub fn g() {}\n");
    let result = introspect(
        &tool,
        json!({"target": root.to_string_lossy(), "max_tokens": 20000}),
    )
    .await;
    assert_eq!(result.coverage.files_total, 1);
    assert!(result.coverage.gitignore_applied);
}

#[tokio::test]
async fn graph_view_claims_no_symbols_and_pays_for_its_edges() {
    let files: Vec<(String, String)> =
        std::iter::once(("mod.rs".to_string(), rust_source("hub", 3)))
            .chain((0..30).map(|i| (format!("m{i:02}.rs"), rust_source(&format!("m{i}"), 20))))
            .collect();
    let refs: Vec<(&str, String)> = files.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    let (_dir, target, tool) = fixture(&refs);

    let roomy = introspect(
        &tool,
        json!({"target": target, "format": "graph", "max_tokens": 20000}),
    )
    .await;
    assert_eq!(roomy.coverage.files_included, 31);
    assert_eq!(roomy.coverage.symbols_included, 0, "the graph renders none");
    assert_eq!(roomy.coverage.symbols_total, 0);
    assert!(!roomy.coverage.is_partial());
    assert!(roomy.files_included.iter().all(|f| f.symbols.is_empty()));
    assert!(roomy.content.contains(" --> "), "edges rendered");
    assert!(roomy.content.contains("no symbols rendered"));

    // Tight: the edges are charged, so the limit holds and whole files drop.
    for limit in [180, 260, 400] {
        let tight = introspect(
            &tool,
            json!({"target": target, "format": "graph", "max_tokens": limit}),
        )
        .await;
        assert!(tight.tokens_used <= limit);
        assert_eq!(tight.tokens_used, estimate_content_tokens(&tight.content));
        assert!(tight.coverage.files_included < 31);
        assert_eq!(tight.coverage.symbols_included, 0);
    }
}

/// Review 2026-09-27 (F1): code_plan's planner had its own walker with no
/// per-path validation. It now uses the shared walk with the tool's policy:
/// a denied file under an allowed root is refused, not read.
#[tokio::test]
async fn code_plan_walk_validates_every_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_file(root, "src/lib.rs", "pub fn ok() {}\n");
    write_file(root, "src/secret/keys.rs", "pub const K: &str = \"x\";\n");
    write_file(root, ".venv/pyvenv.cfg", "home = /usr\n");
    write_file(root, ".venv/lib/site-packages/dep.py", "def f(): pass\n");
    let config = scoped_config(
        format!("{}/**", root.to_string_lossy()),
        vec![format!("{}/src/secret/**", root.to_string_lossy())],
    );
    let err = CodePlan::with_safety_config(config)
        .execute(json!({"goal": "improve ok", "codebase_root": root.to_string_lossy()}))
        .await
        .unwrap_err();
    assert!(is_path_policy_error(&err.to_string()), "{err}");
}
