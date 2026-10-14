use super::*;
use std::fs;
use tempfile::TempDir;

/// The tool now enforces the workspace path policy on `repo_path`; tests using
/// temp-dir fixtures outside the workspace allow those fixtures explicitly
/// (mirroring the file tool tests).
fn permissive_safety_config() -> SafetyConfig {
    SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..SafetyConfig::default()
    }
}

#[test]
fn test_tokenize_filters_short_words() {
    let terms = tokenize("The quick brown fox");
    // "the" is filtered as a stop word.
    assert!(!terms.contains(&"the".to_string()));
    assert!(terms.contains(&"quick".to_string()));
    assert!(terms.contains(&"brown".to_string()));
    assert!(terms.contains(&"fox".to_string()));
    // "a" and "ab" (length <= 2) are filtered.
    let short = tokenize("a ab abc");
    assert!(!short.contains(&"a".to_string()));
    assert!(!short.contains(&"ab".to_string()));
    assert!(short.contains(&"abc".to_string()));
}

#[test]
fn test_source_scan_retains_only_query_term_counts() {
    let query_terms = HashSet::from(["needle".to_string()]);
    let scan = scan_term_frequencies("needle unrelated another NEEDLE", &query_terms, 10);

    assert!(scan.complete);
    assert_eq!(scan.token_count, 4);
    assert_eq!(scan.frequencies.len(), 1);
    assert_eq!(scan.frequencies.get("needle"), Some(&2));
}

#[test]
fn test_localize_issue_rejects_query_token_limit() {
    let temp_dir = TempDir::new().unwrap();
    let query = vec!["alpha"; MAX_LOCALIZE_QUERY_TOKENS + 1].join(" ");

    let error = localize_issue_sync(&query, temp_dir.path().to_str().unwrap(), None).unwrap_err();

    assert_eq!(
        error.downcast_ref::<LocalizeQueryLimitError>(),
        Some(&LocalizeQueryLimitError::Tokens {
            limit: MAX_LOCALIZE_QUERY_TOKENS,
        })
    );
}

#[test]
fn test_tokenize_query_rejects_byte_and_unique_term_limits() {
    let byte_limits = LocalizationLimits {
        query_bytes: 4,
        ..DEFAULT_LOCALIZATION_LIMITS
    };
    assert_eq!(
        tokenize_query("needle", byte_limits).unwrap_err(),
        LocalizeQueryLimitError::Bytes {
            actual: 6,
            limit: 4,
        }
    );

    let unique_limits = LocalizationLimits {
        query_bytes: 1_024,
        query_tokens: 10,
        query_unique_terms: 2,
        ..DEFAULT_LOCALIZATION_LIMITS
    };
    assert_eq!(
        tokenize_query("alpha beta gamma", unique_limits).unwrap_err(),
        LocalizeQueryLimitError::UniqueTerms { limit: 2 }
    );
}

#[test]
fn test_localize_issue_finds_relevant_file() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();

    // Create a dummy source tree.
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::create_dir_all(repo.join("tests")).unwrap();
    fs::write(
        repo.join("src/lib.rs"),
        "pub fn process_data(input: &str) -> String {\n    input.to_string()\n}\n",
    )
    .unwrap();
    fs::write(
        repo.join("src/main.rs"),
        "fn main() {\n    println!(\"hello\");\n}\n",
    )
    .unwrap();
    fs::write(
        repo.join("tests/test_process.rs"),
        "#[test]\nfn test_process_data() {\n    assert_eq!(process_data(\"x\"), \"x\");\n}\n",
    )
    .unwrap();

    let candidates = localize_issue_sync(
        "process_data returns wrong value",
        repo.to_str().unwrap(),
        None,
    )
    .unwrap();
    assert!(!candidates.is_empty(), "should find at least one candidate");

    // The lib.rs file should rank highest because it contains process_data.
    let top_file = &candidates[0].file;
    assert!(
        top_file.contains("lib.rs") || top_file.contains("test_process.rs"),
        "top candidate should be lib.rs or test_process.rs, got {}",
        top_file
    );

    // Check that the candidate has a reasonable reason.
    assert!(!candidates[0].reason.is_empty());
}

#[test]
fn test_localize_issue_empty_query() {
    let temp_dir = TempDir::new().unwrap();
    let candidates = localize_issue_sync("a", temp_dir.path().to_str().unwrap(), None).unwrap();
    assert!(candidates.is_empty());
}

#[test]
fn test_localize_issue_prunes_private_tool_state_but_keeps_scratchpad() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::create_dir_all(repo.join("scratchpad")).unwrap();
    fs::write(
        repo.join("src/lib.rs"),
        "// repository sentinel\npub fn repository_sentinel() {}\n",
    )
    .unwrap();
    fs::write(
        repo.join("scratchpad/kept.rs"),
        "// scratchpad sentinel\npub fn scratchpad_sentinel() {}\n",
    )
    .unwrap();
    for dir in [".claude", ".codex", ".agents", ".qwen", ".superpowers"] {
        let hidden = repo.join(dir).join("state/private.rs");
        fs::create_dir_all(hidden.parent().unwrap()).unwrap();
        fs::write(
            hidden,
            "pub fn private_tool_state_sentinel() {}\npub fn private_tool_state_sentinel_again() {}\n",
        )
        .unwrap();
    }

    let candidates = localize_issue_sync(
        "repository scratchpad private tool state sentinel",
        repo.to_str().unwrap(),
        None,
    )
    .unwrap();
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.file.contains("src/lib.rs")),
        "ordinary repository source was lost: {candidates:?}"
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.file.contains("scratchpad/kept.rs")),
        "scratchpad is deliberately retained: {candidates:?}"
    );
    assert!(candidates.iter().all(|candidate| {
        ![".claude", ".codex", ".agents", ".qwen", ".superpowers"]
            .iter()
            .any(|dir| candidate.file.contains(dir))
    }));
}

#[cfg(unix)]
#[test]
fn test_localize_issue_does_not_follow_source_directory_symlinks() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("src/lib.rs"),
        "// allowed sentinel\npub fn allowed_sentinel() {}\n",
    )
    .unwrap();

    let outside = TempDir::new().unwrap();
    fs::write(
        outside.path().join("secret.rs"),
        "pub fn symlink_private_sentinel() {}\npub fn symlink_private_sentinel_again() {}\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(outside.path(), repo.join("source-alias")).unwrap();

    let candidates = localize_issue_sync(
        "allowed symlink private sentinel",
        repo.to_str().unwrap(),
        None,
    )
    .unwrap();
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.file.contains("src/lib.rs")),
        "ordinary repository source was lost: {candidates:?}"
    );
    assert!(
        candidates
            .iter()
            .all(|candidate| !candidate.file.contains("source-alias")),
        "symlink target became an implicit source input: {candidates:?}"
    );
}

#[cfg(unix)]
#[test]
fn test_localize_issue_does_not_read_source_file_symlink() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();
    fs::write(
        repo.join("allowed.rs"),
        "pub fn allowed_leaf_sentinel() {}\n",
    )
    .unwrap();

    let outside = TempDir::new().unwrap();
    let secret = outside.path().join("secret.rs");
    fs::write(
        &secret,
        "pub fn symlink_leaf_private_sentinel() {}\npub fn symlink_leaf_private_sentinel_again() {}\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(&secret, repo.join("alias.rs")).unwrap();

    let census = localize_issue_with_census_sync(
        "allowed symlink leaf private sentinel",
        repo.to_str().unwrap(),
        None,
    )
    .unwrap();
    assert!(census.eligible_relative_paths.contains("allowed.rs"));
    assert!(!census.eligible_relative_paths.contains("alias.rs"));
    assert!(
        census
            .candidates
            .iter()
            .all(|candidate| !candidate.file.ends_with("alias.rs")),
        "symlink leaf became a source input: {:?}",
        census.candidates
    );
}

#[cfg(unix)]
#[test]
fn test_descriptor_read_rejects_a_parent_symlink_redirect() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();
    let private_dir = repo.join(".claude");
    fs::create_dir(&private_dir).unwrap();
    fs::write(private_dir.join("secret.rs"), "fn private_marker() {}\n").unwrap();
    std::os::unix::fs::symlink(&private_dir, repo.join("public_alias")).unwrap();

    let canonical_root = fs::canonicalize(repo).unwrap();
    let mut scanned_bytes = 0;
    let result = read_source_file(
        repo.join("public_alias/secret.rs").to_str().unwrap(),
        &canonical_root,
        "public_alias/secret.rs",
        &permissive_safety_config(),
        DEFAULT_LOCALIZATION_LIMITS,
        &mut scanned_bytes,
    );

    assert!(matches!(result, SourceRead::ReadError));
    assert_eq!(scanned_bytes, 0);
}

#[cfg(unix)]
#[test]
fn test_hard_link_is_omitted_and_marks_census_incomplete() {
    let root_temp = TempDir::new().unwrap();
    let outside_temp = TempDir::new().unwrap();
    let outside = outside_temp.path().join("secret.rs");
    fs::write(&outside, "fn hard_link_secret_marker() {}\n").unwrap();
    fs::hard_link(&outside, root_temp.path().join("linked.rs")).unwrap();
    fs::write(
        root_temp.path().join("ordinary.rs"),
        "fn ordinary_marker() {}\n",
    )
    .unwrap();

    let census = localize_issue_with_census_sync(
        "hard link secret ordinary marker",
        root_temp.path().to_str().unwrap(),
        None,
    )
    .unwrap();

    assert!(!census.eligible_relative_paths.contains("linked.rs"));
    assert!(census.eligible_relative_paths.contains("ordinary.rs"));
    assert_eq!(census.read_errors, 1);
    assert!(!census.census_complete());
}

#[test]
fn test_localize_issue_skips_oversized_source_file() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();
    fs::write(
        repo.join("small.rs"),
        "pub fn oversize_query_allowed() {}\n",
    )
    .unwrap();
    fs::write(
        repo.join("oversized.rs"),
        vec![b'x'; MAX_LOCALIZE_FILE_BYTES as usize + 1],
    )
    .unwrap();

    let census =
        localize_issue_with_census_sync("oversize query allowed", repo.to_str().unwrap(), None)
            .unwrap();
    assert!(census.truncated, "the per-file ceiling must be reported");
    assert!(census.eligible_relative_paths.contains("small.rs"));
    assert!(!census.eligible_relative_paths.contains("oversized.rs"));
    assert!(census.scanned_bytes <= MAX_LOCALIZE_TOTAL_BYTES);
}

#[test]
fn test_localize_issue_file_cap_is_deterministic() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();
    for name in ["c.rs", "a.rs", "b.rs"] {
        fs::write(
            repo.join(name),
            format!("pub fn bounded_source_marker_{}() {{}}\n", &name[..1]),
        )
        .unwrap();
    }
    let limits = LocalizationLimits {
        file_bytes: 1_024,
        total_bytes: 4_096,
        source_files: 2,
        walk_entries: 32,
        file_tokens: 1_024,
        total_tokens: 4_096,
        query_bytes: 1_024,
        query_tokens: 128,
        query_unique_terms: 128,
    };

    let first = localize_issue_with_limits(
        "bounded source marker",
        repo.to_str().unwrap(),
        None,
        limits,
        false,
        false,
    )
    .unwrap();
    let second = localize_issue_with_limits(
        "bounded source marker",
        repo.to_str().unwrap(),
        None,
        limits,
        false,
        false,
    )
    .unwrap();

    assert!(first.truncated);
    assert_eq!(first.eligible_relative_paths.len(), 2);
    assert_eq!(
        first.eligible_relative_paths,
        second.eligible_relative_paths
    );
    assert_eq!(first.manifest_sha256, second.manifest_sha256);
    assert_eq!(
        first
            .eligible_relative_paths
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["a.rs".to_string(), "b.rs".to_string()]
    );
}

#[test]
fn test_localize_issue_per_file_token_cap_preserves_census() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();
    fs::write(
        repo.join("a_large.rs"),
        "needle alpha beta gamma delta epsilon\n",
    )
    .unwrap();
    fs::write(repo.join("b_small.rs"), "needle alpha\n").unwrap();
    let limits = LocalizationLimits {
        file_bytes: 1_024,
        total_bytes: 4_096,
        source_files: 10,
        walk_entries: 32,
        file_tokens: 3,
        total_tokens: 32,
        query_bytes: 1_024,
        query_tokens: 128,
        query_unique_terms: 128,
    };

    let ranked =
        localize_issue_with_limits("needle", repo.to_str().unwrap(), None, limits, false, false)
            .unwrap();
    let provenance =
        localize_issue_with_limits("", repo.to_str().unwrap(), None, limits, false, true).unwrap();

    assert!(!ranked.truncated);
    assert!(ranked.ranking_truncated);
    assert_eq!(ranked.eligible_relative_paths.len(), 2);
    assert_eq!(ranked.files, provenance.files);
    assert_eq!(ranked.manifest_sha256, provenance.manifest_sha256);
    assert!(!provenance.ranking_truncated);
    assert!(ranked
        .candidates
        .iter()
        .any(|candidate| candidate.file.ends_with("b_small.rs")));
    assert!(ranked
        .candidates
        .iter()
        .all(|candidate| !candidate.file.ends_with("a_large.rs")));
}

#[test]
fn test_localize_issue_total_token_cap_omits_partial_documents() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();
    fs::write(repo.join("a.rs"), "needle alpha beta\n").unwrap();
    fs::write(repo.join("b.rs"), "needle gamma delta\n").unwrap();
    let limits = LocalizationLimits {
        file_bytes: 1_024,
        total_bytes: 4_096,
        source_files: 10,
        walk_entries: 32,
        file_tokens: 10,
        total_tokens: 4,
        query_bytes: 1_024,
        query_tokens: 128,
        query_unique_terms: 128,
    };

    let census =
        localize_issue_with_limits("needle", repo.to_str().unwrap(), None, limits, false, false)
            .unwrap();

    assert!(!census.truncated);
    assert!(census.ranking_truncated);
    assert_eq!(census.eligible_relative_paths.len(), 2);
    assert!(census
        .candidates
        .iter()
        .any(|candidate| candidate.file.ends_with("a.rs")));
    assert!(census
        .candidates
        .iter()
        .all(|candidate| !candidate.file.ends_with("b.rs")));
}

#[test]
fn test_localize_issue_census_digest_binds_full_content() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();
    let path = repo.join("source.rs");
    fs::write(&path, "pub fn census_digest_marker() { one(); }\n").unwrap();

    let before =
        localize_issue_with_census_sync("census digest marker", repo.to_str().unwrap(), None)
            .unwrap();
    let recomputed = recompute_localization_census_sync(repo.to_str().unwrap(), None).unwrap();
    assert_eq!(before.manifest_sha256, recomputed.manifest_sha256);
    assert_eq!(
        before.eligible_relative_paths,
        recomputed.eligible_relative_paths
    );
    fs::write(&path, "pub fn census_digest_marker() { two(); }\n").unwrap();
    let after =
        localize_issue_with_census_sync("census digest marker", repo.to_str().unwrap(), None)
            .unwrap();

    assert_eq!(before.files.len(), 1);
    assert_eq!(before.files[0].relative_path, "source.rs");
    assert_eq!(before.files[0].sha256.len(), 64);
    assert_ne!(before.files[0].sha256, after.files[0].sha256);
    assert_ne!(before.manifest_sha256, after.manifest_sha256);
}

#[test]
fn test_localize_issue_total_byte_cap_stops_at_sorted_boundary() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();
    let first = "pub fn bounded_total_marker_a() {}\n";
    let second = "pub fn bounded_total_marker_b() {}\n";
    fs::write(repo.join("a.rs"), first).unwrap();
    fs::write(repo.join("b.rs"), second).unwrap();
    let limits = LocalizationLimits {
        file_bytes: 1_024,
        // The EOF probe requires one spare byte. This admits a.rs exactly and
        // makes b.rs the deterministic stopping boundary.
        total_bytes: first.len() as u64 + 1,
        source_files: 10,
        walk_entries: 32,
        file_tokens: 1_024,
        total_tokens: 4_096,
        query_bytes: 1_024,
        query_tokens: 128,
        query_unique_terms: 128,
    };

    let census = localize_issue_with_limits(
        "bounded total marker",
        repo.to_str().unwrap(),
        None,
        limits,
        false,
        false,
    )
    .unwrap();
    assert!(census.truncated);
    assert_eq!(census.scanned_bytes, first.len() as u64);
    assert_eq!(
        census
            .eligible_relative_paths
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["a.rs".to_string()]
    );
}

#[test]
fn test_localize_issue_refuses_explicit_private_root() {
    let temp_dir = TempDir::new().unwrap();
    for name in [
        ".git",
        ".selfware",
        ".claude",
        ".codex",
        ".agents",
        ".qwen",
        ".superpowers",
    ] {
        let private_root = temp_dir.path().join(name);
        fs::create_dir(&private_root).unwrap();
        fs::write(private_root.join("private.rs"), "fn private_marker() {}\n").unwrap();

        let error = localize_issue_sync("private marker", private_root.to_str().unwrap(), None)
            .expect_err("an explicitly selected private state root must be refused");
        assert!(
            error.to_string().contains("private repository root"),
            "{name}: {error}"
        );
    }
}

#[cfg(unix)]
#[test]
fn test_localize_issue_refuses_canonical_private_root_alias() {
    let temp_dir = TempDir::new().unwrap();
    let private_root = temp_dir.path().join(".selfware");
    fs::create_dir(&private_root).unwrap();
    fs::write(private_root.join("private.rs"), "fn private_marker() {}\n").unwrap();
    let alias = temp_dir.path().join("source-alias");
    std::os::unix::fs::symlink(&private_root, &alias).unwrap();

    let error = localize_issue_sync("private marker", alias.to_str().unwrap(), None)
        .expect_err("a canonical private state root must be refused");
    assert!(error
        .to_string()
        .contains("canonical private repository root"));
}

#[test]
fn test_best_function_in_file() {
    let code = "pub fn foo() {}\npub fn bar() {}\npub fn process_data() {}";
    let terms = vec!["process".to_string(), "data".to_string()];
    let best = best_function_in_file(code, &terms);
    assert_eq!(best, "process_data");
}

#[tokio::test]
async fn test_localize_issue_tool_execute() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/lib.rs"), "pub fn fix_me() {}\n").unwrap();

    let tool = LocalizeIssue::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "issue": "fix_me is broken",
        "repo_path": repo.to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let candidates = result["candidates"].as_array().unwrap();
    assert!(!candidates.is_empty());
    assert!(candidates[0]["file"].as_str().unwrap().contains("lib.rs"));
}

/// localize_issue reads every source file under `repo_path` — an
/// out-of-workspace root must be rejected with the path-policy error
/// (regression for the read-only tools bypassing path validation).
#[tokio::test]
async fn test_localize_issue_rejects_out_of_workspace_repo() {
    let tool = LocalizeIssue::with_safety_config(SafetyConfig::default());
    let args = serde_json::json!({
        "issue": "something is broken",
        "repo_path": "/etc"
    });

    let result = tool.execute(args).await;
    let err = result.expect_err("localize_issue on /etc must be rejected");
    let message = err.to_string();
    assert!(
        message.contains("outside working directory")
            || message.contains("not in allowed list")
            || message.contains("protected system path"),
        "expected a path-policy rejection, got: {}",
        message
    );
}

/// P1 regression: localize_issue reads every source file under the repo root —
/// a denied subdirectory inside the allowed root must not be read, so no
/// candidate from it is reported.
#[tokio::test]
async fn test_localize_issue_denied_child_excluded() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::create_dir_all(repo.join("denied_dir")).unwrap();
    fs::write(repo.join("src/lib.rs"), "pub fn process_broken() {}\n").unwrap();
    // The denied file is a much stronger textual match than the allowed one —
    // if it were read at all, it would rank first.
    fs::write(
        repo.join("denied_dir/lib.rs"),
        "pub fn process_broken() {}\npub fn process_broken() {}\npub fn process_broken() {}\n",
    )
    .unwrap();

    let tool = LocalizeIssue::with_safety_config(SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        denied_paths: vec!["denied_dir".to_string()],
        ..SafetyConfig::default()
    });
    let args = serde_json::json!({
        "issue": "process_broken is broken",
        "repo_path": repo.to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let candidates = result["candidates"].as_array().unwrap();
    assert!(!candidates.is_empty(), "allowed files must still be ranked");
    for c in candidates {
        let file = c["file"].as_str().unwrap();
        assert!(
            !file.contains("denied_dir"),
            "candidate from a denied subdirectory leaked: {}",
            file
        );
    }
}

#[test]
fn test_recent_git_files_with_spaces_and_leading_whitespace() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();

    let init_ok = std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !init_ok {
        return;
    }
    let _ = std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo)
        .status();
    let _ = std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo)
        .status();

    fs::write(repo.join(" normal.rs"), "fn normal() {}\n").unwrap();
    fs::write(repo.join("file with spaces.rs"), "fn spaces() {}\n").unwrap();

    let _ = std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(repo)
        .status();
    let _ = std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(repo)
        .status();

    let files = recent_git_files(repo.to_str().unwrap());
    assert!(files.contains(" normal.rs"), "must preserve leading space");
    assert!(
        files.contains("file with spaces.rs"),
        "must preserve spaces"
    );
}

#[test]
fn test_recent_git_files_from_subdirectory_returns_relative_paths() {
    let temp_dir = TempDir::new().unwrap();
    let repo = temp_dir.path();
    let sub = repo.join("crates/subcrate/src");
    fs::create_dir_all(&sub).unwrap();

    let init_ok = std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !init_ok {
        return;
    }
    let _ = std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo)
        .status();
    let _ = std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo)
        .status();

    fs::write(sub.join("lib.rs"), "pub fn subcrate_target() {}\n").unwrap();

    let _ = std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(repo)
        .status();
    let _ = std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(repo)
        .status();

    let subcrate_root = repo.join("crates/subcrate");
    let files = recent_git_files(subcrate_root.to_str().unwrap());
    assert!(
        files.contains("src/lib.rs"),
        "expected subdirectory-relative path src/lib.rs, got: {files:?}"
    );

    let candidates =
        localize_issue_sync("subcrate_target", subcrate_root.to_str().unwrap(), None).unwrap();
    assert_eq!(candidates.len(), 1);
    assert!(
        candidates[0].reason.contains("recently modified"),
        "expected recently modified boost on subdirectory repo_path, got: {}",
        candidates[0].reason
    );
}

#[test]
fn test_localize_issue_accepts_case_variant_repo_path() {
    let temp_dir = TempDir::new().unwrap();
    let real_repo = temp_dir.path().join("MyRepo");
    fs::create_dir_all(real_repo.join("src")).unwrap();
    fs::write(
        real_repo.join("src/lib.rs"),
        "pub fn case_variant_target() {}\n",
    )
    .unwrap();

    let variant_repo = temp_dir.path().join("myrepo");
    if !variant_repo.exists() {
        // Case-sensitive filesystem; nothing to test beyond real_repo.
        return;
    }

    let census = localize_issue_with_census_sync(
        "case_variant_target",
        variant_repo.to_str().unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(census.read_errors, 0);
    assert!(census.census_complete());
    assert!(census.eligible_relative_paths.contains("src/lib.rs"));
    assert_eq!(census.candidates.len(), 1);
}
