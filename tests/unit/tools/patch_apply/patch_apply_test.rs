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
    let tool = PatchApply;
    assert_eq!(tool.name(), "patch_apply");
}

#[test]
fn test_patch_apply_schema() {
    let tool = PatchApply;
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
    let result = PatchApply.execute(serde_json::json!({"diff": diff})).await;
    let err = result.expect_err("deletion of denied path must be rejected");
    assert!(
        err.to_string().contains(".env"),
        "error should name the rejected path: {err}"
    );
}

#[tokio::test]
async fn test_deletion_with_parent_escape_rejected() {
    let diff = "--- a/../outside.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-x\n";
    let result = PatchApply.execute(serde_json::json!({"diff": diff})).await;
    let err = result.expect_err("parent-escape deletion must be rejected");
    assert!(
        err.to_string().contains("parent-directory"),
        "unexpected error: {err}"
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
    let result = PatchApply.execute(serde_json::json!({"diff": diff})).await;
    let err = result.expect_err("symlink-creating diff must be refused");
    assert!(
        err.to_string().contains("symlink"),
        "error must name symlinks: {err}"
    );
}

#[tokio::test]
async fn test_existing_file_converted_to_symlink_rejected() {
    let diff = "diff --git a/conf b/conf\nold mode 100644\nnew mode 120000\nindex 111..222 100644\n--- a/conf\n+++ b/conf\n@@ -1 +1 @@\n-x\n+link:/etc/shadow\n";
    let result = PatchApply.execute(serde_json::json!({"diff": diff})).await;
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
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .status()
        .unwrap();

    let diff = "diff --git a/new.txt b/new.txt\nnew file mode 100644\n--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1 @@\n+hello world\n";
    let _state = crate::test_support::CwdGuard::enter(&repo);
    let result = PatchApply.execute(serde_json::json!({"diff": diff})).await;
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

fn patch_repo_with(file: &str, content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .status()
        .unwrap();
    std::fs::write(repo.join(file), content).unwrap();
    (dir, repo)
}

#[tokio::test]
async fn numbered_diff_applies_with_prefixes_stripped_and_says_so() {
    let (_dir, repo) = patch_repo_with("f.txt", "alpha\nbeta\ngamma\n");
    let diff = "--- a/f.txt\n+++ b/f.txt\n@@ -1,3 +1,3 @@\n      1\talpha\n-     2\tbeta\n+     2\tBETA\n      3\tgamma\n";
    let _state = crate::test_support::CwdGuard::enter(&repo);
    let value = PatchApply
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
    let (_dir, repo) = patch_repo_with("ids.tsv", "1\tone\n2\ttwo\n");
    let diff = "--- a/ids.tsv\n+++ b/ids.tsv\n@@ -1,2 +1,2 @@\n 1\tone\n-2\ttwo\n+2\tTWO\n";
    let _state = crate::test_support::CwdGuard::enter(&repo);
    let value = PatchApply
        .execute(serde_json::json!({"diff": diff, "allow_3way": false}))
        .await
        .expect("diff matching as written must apply");
    assert!(value.get("line_number_prefixes_stripped").is_none());
    assert_eq!(
        std::fs::read_to_string(repo.join("ids.tsv")).unwrap(),
        "1\tone\n2\tTWO\n"
    );
}
