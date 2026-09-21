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
