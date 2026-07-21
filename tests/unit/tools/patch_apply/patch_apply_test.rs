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
