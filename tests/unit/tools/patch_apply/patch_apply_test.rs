use super::*;

#[test]
fn test_parse_diff_stats() {
    let diff = r#"--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
 line1
-line2
+line2_modified
 line3
"#;
    let (files, insertions, deletions, targets) = parse_diff_stats(diff);
    assert_eq!(files, 1);
    assert_eq!(insertions, 1);
    assert_eq!(deletions, 1);
    assert_eq!(targets, vec!["file.txt"]);
}

#[test]
fn test_parse_diff_stats_multi_file() {
    let diff = r#"--- a/one.txt
+++ b/one.txt
@@ -1 +1 @@
-old1
+new1
--- a/two.txt
+++ b/two.txt
@@ -1,2 +1,3 @@
 line
-old
+new
+added
"#;
    let (files, insertions, deletions, targets) = parse_diff_stats(diff);
    assert_eq!(files, 2);
    assert_eq!(insertions, 3);
    assert_eq!(deletions, 2);
    assert_eq!(targets, vec!["one.txt", "two.txt"]);
}

#[test]
fn test_patch_apply_name() {
    let tool = PatchApply::new();
    assert_eq!(tool.name(), "patch_apply");
}

#[test]
fn test_patch_apply_schema() {
    let tool = PatchApply::new();
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["diff"].is_object());
}

#[test]
fn test_parse_diff_stats_deletion_targets_old_path() {
    let diff = r#"--- a/deleted.txt
+++ /dev/null
@@ -1,2 +0,0 @@
-line1
-line2
"#;
    let (files, insertions, deletions, targets) = parse_diff_stats(diff);
    assert_eq!(files, 1);
    assert_eq!(insertions, 0);
    assert_eq!(deletions, 2);
    // The deleted file's OLD path is the operation target, never /dev/null.
    assert_eq!(targets, vec!["deleted.txt"]);
}

#[test]
fn test_parse_diff_stats_new_file_has_no_old_target() {
    let diff = r#"--- /dev/null
+++ b/new.txt
@@ -0,0 +1 @@
+line
"#;
    let (files, insertions, deletions, targets) = parse_diff_stats(diff);
    assert_eq!(files, 1);
    assert_eq!(insertions, 1);
    assert_eq!(deletions, 0);
    assert_eq!(targets, vec!["new.txt"]);
}

#[test]
fn test_parse_diff_stats_mixed_edit_and_deletion() {
    let diff = r#"--- a/keep.txt
+++ b/keep.txt
@@ -1 +1 @@
-old
+new
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
"#;
    let (files, _, _, targets) = parse_diff_stats(diff);
    assert_eq!(files, 2);
    assert_eq!(targets, vec!["keep.txt", "gone.txt"]);
}

#[tokio::test]
async fn test_deletion_of_denied_path_rejected() {
    // `.env` is in the default denied_paths; a patch deleting it must be
    // rejected even though the diff's new-file is /dev/null.
    let diff = "--- a/.env\n+++ /dev/null\n@@ -1 +0,0 @@\n-x\n";
    let result = PatchApply::new()
        .execute(serde_json::json!({"diff": diff}))
        .await;
    let err = result.expect_err("deletion of denied path must be rejected");
    assert!(
        err.to_string().contains(".env"),
        "error should name the rejected path: {err}"
    );
}

#[tokio::test]
async fn test_deletion_with_parent_escape_rejected() {
    let diff = "--- a/../outside.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-x\n";
    let result = PatchApply::new()
        .execute(serde_json::json!({"diff": diff}))
        .await;
    let err = result.expect_err("parent-escape deletion must be rejected");
    assert!(
        err.to_string().contains("parent-directory"),
        "unexpected error: {err}"
    );
}

// ── Paths git finds that the `---`/`+++` scan does not ──

/// `git apply` names files in more places than the `+++` lines and decodes
/// them differently; the path policy must apply to git's list, both to the
/// file it writes and to the one it reads. `.env` is in the default
/// denied_paths, so each of these must be refused and must name `.env`.
#[tokio::test]
async fn test_denied_path_reached_through_git_only_header_forms_rejected() {
    let (_dir, repo, _state) = patch_repo_with(".env", "DB_PASSWORD=x\n");
    let diffs = [
        // C-quoted header names (`\056` is `.`).
        "--- \"a/\\056env\"\n+++ \"b/\\056env\"\n@@ -1 +1 @@\n-DB_PASSWORD=x\n+DB_PASSWORD=y\n",
        // Copy header only: no `---`/`+++` lines at all.
        "diff --git a/.env b/leak.txt\nsimilarity index 100%\ncopy from .env\ncopy to leak.txt\n",
        // Rename header only.
        "diff --git a/.env b/leak.txt\nsimilarity index 100%\nrename from .env\nrename to leak.txt\n",
        // Implicit rename: differing names without a rename header; git
        // reads `.env`, writes `leak.txt` and removes `.env`.
        "diff --git a/.env b/leak.txt\n--- a/.env\n+++ b/leak.txt\n@@ -2,0 +2,1 @@\n+x\n",
        // git prefers the shorter name when one is a prefix of the other.
        "--- a/.env\n+++ b/.envx\n@@ -1 +1 @@\n-DB_PASSWORD=x\n+DB_PASSWORD=y\n",
        // Mode change only.
        "diff --git a/.env b/.env\nold mode 100644\nnew mode 100755\n",
    ];
    for diff in diffs {
        let result = PatchApply::new()
            .execute(serde_json::json!({"diff": diff, "allow_3way": false}))
            .await;
        let err = result.expect_err("diff touching a denied path must be rejected");
        assert!(
            err.to_string().contains(".env"),
            "error should name the rejected path for {diff:?}: {err}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(repo.join(".env")).unwrap(),
        "DB_PASSWORD=x\n"
    );
    assert!(!repo.join("leak.txt").exists());
}

#[tokio::test]
async fn test_unparseable_diff_rejected_before_apply() {
    let (_dir, _repo, _state) = patch_repo_with("f.txt", "x\n");
    let result = PatchApply::new()
        .execute(serde_json::json!({"diff": "not a diff\n"}))
        .await;
    let err = result.expect_err("a diff git cannot parse must be refused");
    assert!(
        err.to_string().contains("could not parse"),
        "unexpected error: {err}"
    );
}

/// A rename of an allowed file still applies: both of its names pass.
#[tokio::test]
async fn test_rename_of_allowed_file_still_applies() {
    let (_dir, repo, _state) = patch_repo_with("old.txt", "hello\n");
    let diff =
        "diff --git a/old.txt b/new.txt\nsimilarity index 100%\nrename from old.txt\nrename to new.txt\n";
    PatchApply::new()
        .execute(serde_json::json!({"diff": diff, "allow_3way": false}))
        .await
        .expect("rename between allowed paths must apply");
    assert!(!repo.join("old.txt").exists());
    assert_eq!(
        std::fs::read_to_string(repo.join("new.txt")).unwrap(),
        "hello\n"
    );
}

// ── Symlink (mode 120000) rejection (pass-3) ──

#[test]
fn test_diff_touches_symlink_mode_detection() {
    // Creation of a symlink whose target is a sensitive host path.
    assert!(diff_touches_symlink_mode(
        "--- /dev/null\n+++ b/evil\nnew file mode 120000\n@@ -0,0 +1 @@\n+link:/etc/shadow\n",
    ));
    // Mode conversion of an existing file into a symlink.
    assert!(diff_touches_symlink_mode(
        "old mode 100644\nnew mode 120000\n"
    ));
    // Retargeting an existing symlink (deliberately denied too).
    assert!(diff_touches_symlink_mode(
        "old mode 120000\nnew mode 120000\n"
    ));
    // Regular file modes never match.
    assert!(!diff_touches_symlink_mode("new file mode 100644\n"));
    assert!(!diff_touches_symlink_mode(
        "old mode 100644\nnew mode 100755\n"
    ));
    assert!(!diff_touches_symlink_mode(
        "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n"
    ));
}

#[tokio::test]
async fn test_symlink_creation_diff_rejected() {
    let diff =
        "--- /dev/null\n+++ b/evil\nnew file mode 120000\n@@ -0,0 +1 @@\n+link:/etc/shadow\n";
    let result = PatchApply::new()
        .execute(serde_json::json!({"diff": diff}))
        .await;
    let err = result.expect_err("symlink-creating diff must be refused");
    assert!(
        err.to_string().contains("symlink"),
        "error must name symlinks: {err}"
    );
}

#[tokio::test]
async fn test_existing_file_converted_to_symlink_rejected() {
    let diff = "diff --git a/conf b/conf\nold mode 100644\nnew mode 120000\nindex 111..222 100644\n--- a/conf\n+++ b/conf\n@@ -1 +1 @@\n-x\n+link:/etc/shadow\n";
    let result = PatchApply::new()
        .execute(serde_json::json!({"diff": diff}))
        .await;
    let err = result.expect_err("mode-converting diff must be refused");
    assert!(
        err.to_string().contains("symlink"),
        "error must name symlinks: {err}"
    );
}

/// The symlink guard must not break the tool's normal job: a regular
/// file-creation diff still applies end-to-end (real `git apply` in an
/// isolated temp repo). Run under `CwdGuard` because the tool applies
/// relative to the process cwd.
#[tokio::test]
async fn test_regular_file_creation_diff_still_applies() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let _state = crate::test_support::CwdGuard::enter(&repo);
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .status()
        .unwrap();

    let diff = "diff --git a/new.txt b/new.txt\nnew file mode 100644\n--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1 @@\n+hello world\n";
    let result = PatchApply::new()
        .execute(serde_json::json!({"diff": diff}))
        .await;
    let value = result.expect("regular file-creation diff must apply");
    assert_eq!(value["files_changed"], 1);
    assert_eq!(
        std::fs::read_to_string(repo.join("new.txt")).unwrap(),
        "hello world\n"
    );
}

/// Regression (review finding P1): `git apply` runs against project-controlled
/// diffs, so the constructed command must not inherit host credentials. A
/// pass-through `git` stub (intercepted only when a sentinel arg is present,
/// delegating to the real git otherwise so concurrent tests never see it)
/// dumps the child environment to a file; the synthetic marker must be absent
/// while the shared allowlist (PATH) still reaches the child.
#[tokio::test]
#[cfg(unix)]
async fn git_apply_command_sanitizes_env() {
    let _env = crate::test_support::EnvGuard::capture(&["SELFWARE_PATCH_MARKER", "PATH"]);
    _env.set("SELFWARE_PATCH_MARKER", "synthetic-leak-marker");

    let dir = tempfile::tempdir().unwrap();
    let dump = dir.path().join("child.env");
    let real_git = std::process::Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .expect("resolve git")
        .stdout;
    let real_git = String::from_utf8_lossy(&real_git).trim().to_string();
    assert!(!real_git.is_empty(), "real git must be resolvable");

    let stub = dir.path().join("git");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\n\
             for a in \"$@\"; do\n\
               case \"$a\" in\n\
                 --selfware-env-dump=*) dump=\"${{a#--selfware-env-dump=}}\"; env > \"$dump\"; exit 0 ;;\n\
               esac\n\
             done\n\
             exec {real_git} \"$@\"\n"
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    let stub_dir = dir.path().to_path_buf();
    _env.set(
        "PATH",
        format!(
            "{}:{}",
            stub_dir.display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );

    let dump_arg = format!("--selfware-env-dump={}", dump.display());
    let output = git_apply_command(&["apply", "--check", "/tmp/example.patch", &dump_arg])
        .output()
        .await
        .expect("stub git must run");
    assert!(
        output.status.success(),
        "stub should exit 0; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let child_env = std::fs::read_to_string(&dump).expect("stub must dump its env");
    assert!(
        !child_env.contains("SELFWARE_PATCH_MARKER"),
        "synthetic marker leaked to the git child; saw:\n{child_env}"
    );
    assert!(
        child_env.contains("PATH="),
        "the shared allowlist (PATH) must still reach the child; saw:\n{child_env}"
    );
}

// ---- line-number prefixes copied from file_read ---------------------------

#[test]
fn strip_numbered_diff_requires_every_context_and_removed_line_prefixed() {
    let numbered = "--- a/f.txt\n+++ b/f.txt\n@@ -1,3 +1,3 @@\n      1\talpha\n-     2\tbeta\n+     2\tBETA\n      3\tgamma\n";
    assert_eq!(
        strip_numbered_diff(numbered).as_deref(),
        Some("--- a/f.txt\n+++ b/f.txt\n@@ -1,3 +1,3 @@\n alpha\n-beta\n+BETA\n gamma\n")
    );
    // Added lines without a prefix stay as written.
    let fresh_add =
        "--- a/f.txt\n+++ b/f.txt\n@@ -1,2 +1,3 @@\n      1\talpha\n+inserted\n      2\tbeta\n";
    assert_eq!(
        strip_numbered_diff(fresh_add).as_deref(),
        Some("--- a/f.txt\n+++ b/f.txt\n@@ -1,2 +1,3 @@\n alpha\n+inserted\n beta\n")
    );
    // One unprefixed removed line: not a numbered copy, left alone.
    let mixed = "--- a/f.txt\n+++ b/f.txt\n@@ -1,2 +1,2 @@\n      1\talpha\n-beta\n+BETA\n";
    assert_eq!(strip_numbered_diff(mixed), None);
    // A plain diff has nothing to strip.
    let plain = "--- a/f.txt\n+++ b/f.txt\n@@ -1 +1 @@\n-old\n+new\n";
    assert_eq!(strip_numbered_diff(plain), None);
}

fn patch_repo_with(
    file: &str,
    content: &str,
) -> (
    tempfile::TempDir,
    std::path::PathBuf,
    crate::test_support::CwdGuard,
) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let state = crate::test_support::CwdGuard::enter(&repo);
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .status()
        .unwrap();
    std::fs::write(repo.join(file), content).unwrap();
    (dir, repo, state)
}

#[tokio::test]
async fn numbered_diff_applies_with_prefixes_stripped_and_says_so() {
    let (_dir, repo, _state) = patch_repo_with("f.txt", "alpha\nbeta\ngamma\n");
    let diff = "--- a/f.txt\n+++ b/f.txt\n@@ -1,3 +1,3 @@\n      1\talpha\n-     2\tbeta\n+     2\tBETA\n      3\tgamma\n";
    let value = PatchApply::new()
        .execute(serde_json::json!({"diff": diff, "allow_3way": false}))
        .await
        .expect("de-numbered diff must apply");
    assert_eq!(value["line_number_prefixes_stripped"], true);
    assert!(value["note"]
        .as_str()
        .unwrap()
        .contains("line-number prefixes stripped"));
    assert_eq!(
        std::fs::read_to_string(repo.join("f.txt")).unwrap(),
        "alpha\nBETA\ngamma\n"
    );
}

#[tokio::test]
async fn diff_of_genuinely_numbered_content_applies_as_written() {
    // The file itself has `N<TAB>` lines: the diff matches as written and
    // must never be de-numbered.
    let (_dir, repo, _state) = patch_repo_with("ids.tsv", "1\tone\n2\ttwo\n");
    let diff = "--- a/ids.tsv\n+++ b/ids.tsv\n@@ -1,2 +1,2 @@\n 1\tone\n-2\ttwo\n+2\tTWO\n";
    let value = PatchApply::new()
        .execute(serde_json::json!({"diff": diff, "allow_3way": false}))
        .await
        .expect("diff matching as written must apply");
    assert!(value.get("line_number_prefixes_stripped").is_none());
    assert_eq!(
        std::fs::read_to_string(repo.join("ids.tsv")).unwrap(),
        "1\tone\n2\tTWO\n"
    );
}

#[test]
fn diff_sides_split_per_target_for_the_display_artifact_guard() {
    let diff =
        "--- a/x.py\n+++ b/x.py\n@@ -1,2 +1,2 @@\n ctx\n-old & line\n+new &lt;tool_call> line\n\
                --- a/y.py\n+++ b/y.py\n@@ -1 +1 @@\n-k = v\n+k = [REDACTED:token]\n";
    let sides = diff_sides_by_target(diff);
    assert_eq!(sides.len(), 2);
    assert_eq!(sides[0].0, "x.py");
    assert_eq!(sides[0].1, "ctx\nold & line\n");
    assert_eq!(sides[0].2, "new &lt;tool_call> line\n");
    assert_eq!(sides[1].0, "y.py");
    assert!(crate::tools::file::refuse_display_artifacts(
        "patch_apply",
        &sides[1].0,
        Some("k = v\n"),
        &sides[1].1,
        &sides[1].2
    )
    .is_err());
    assert!(crate::tools::file::refuse_display_artifacts(
        "patch_apply",
        &sides[0].0,
        Some("ctx\nold & line\n"),
        &sides[0].1,
        &sides[0].2
    )
    .is_err());
}

#[test]
fn test_extract_patch_paths_timestamps_renames_and_c_quoted_rejection() {
    // 1. Trailing \t2026-10-14 timestamps are stripped from --- and +++ headers.
    let timestamped_diff = "\
--- a/foo.rs\t2026-10-14
+++ b/foo.rs\t2026-10-14 12:00:00.000000000 +0000
@@ -1 +1 @@
-old
+new
";
    assert!(check_diff_header_safety(timestamped_diff).is_ok());
    let (files, insertions, deletions, targets) = parse_diff_stats(timestamped_diff);
    assert_eq!(files, 1);
    assert_eq!(insertions, 1);
    assert_eq!(deletions, 1);
    assert_eq!(targets, vec!["foo.rs"]);

    // 2. rename from / rename to / copy from / copy to paths are extracted.
    let rename_copy_diff = "\
diff --git a/src/old_name.rs b/src/new_name.rs
similarity index 100%
rename from src/old_name.rs
rename to src/new_name.rs
diff --git a/src/orig.rs b/src/copied.rs
similarity index 100%
copy from src/orig.rs
copy to src/copied.rs
";
    assert!(check_diff_header_safety(rename_copy_diff).is_ok());
    let (_, _, _, rename_targets) = parse_diff_stats(rename_copy_diff);
    assert_eq!(
        rename_targets,
        vec![
            "src/old_name.rs",
            "src/new_name.rs",
            "src/orig.rs",
            "src/copied.rs"
        ]
    );

    // 3. C-quoted paths (e.g. --- "a/foo\141.rs") are rejected.
    for quoted_diff in [
        "--- \"a/foo\\141.rs\"\n+++ b/fooa.rs\n@@ -1 +1 @@\n-a\n+b\n",
        "--- a/foo.rs\n+++ \"b/foo\\141.rs\"\n@@ -1 +1 @@\n-a\n+b\n",
        "diff --git \"a/foo\\141.rs\" \"b/foo\\141.rs\"\n",
        "rename to \"a/foo\\141.rs\"\n",
        "copy to \"b/foo\\141.rs\"\n",
    ] {
        let err = check_diff_header_safety(quoted_diff)
            .expect_err("C-quoted diff header path must be rejected");
        assert!(
            err.contains("rejects quoted"),
            "unexpected error for {quoted_diff:?}: {err}"
        );
    }
}

#[tokio::test]
async fn patch_apply_clears_file_snapshot_allowing_subsequent_file_edit() {
    let (_dir, repo, _state) = patch_repo_with("f.txt", "alpha\nbeta\ngamma\n");

    // 1. Read the file via FileRead (which anchors "f.txt" to the absolute repo path).
    crate::tools::file::FileRead::new()
        .execute(serde_json::json!({"path": "f.txt"}))
        .await
        .expect("initial file_read must succeed");

    // 2. Apply a patch via PatchApply::new() (which clears "f.txt" in FILE_SNAPSHOTS).
    let diff = "--- a/f.txt\n+++ b/f.txt\n@@ -1,3 +1,3 @@\n alpha\n-beta\n+BETA\n gamma\n";
    PatchApply::new()
        .execute(serde_json::json!({"diff": diff, "allow_3way": false}))
        .await
        .expect("patch_apply must succeed");

    // 3. Edit the file via FileEdit without re-reading — must succeed without FileStale.
    crate::tools::file::FileEdit::new()
        .execute(serde_json::json!({
            "path": "f.txt",
            "old_str": "BETA",
            "new_str": "BETA_EDITED"
        }))
        .await
        .expect("FileEdit after PatchApply::new() must not fail with FileStale");

    assert_eq!(
        std::fs::read_to_string(repo.join("f.txt")).unwrap(),
        "alpha\nBETA_EDITED\ngamma\n"
    );
}

#[tokio::test]
async fn patch_apply_with_safety_config_honors_custom_denied_paths() {
    let (_dir, repo, _state) = patch_repo_with("custom_locked.txt", "before\n");

    let mut cfg = crate::config::SafetyConfig::default();
    cfg.denied_paths.push("**/custom_locked.txt".to_string());
    let tool = PatchApply::with_safety_config(cfg);

    let diff = "--- a/custom_locked.txt\n+++ b/custom_locked.txt\n@@ -1 +1 @@\n-before\n+after\n";
    let res = tool
        .execute(serde_json::json!({"diff": diff, "allow_3way": false}))
        .await;
    assert!(
        res.is_err(),
        "expected custom denied_paths to block patch_apply"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("custom_locked.txt")).unwrap(),
        "before\n"
    );
}
