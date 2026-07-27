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
