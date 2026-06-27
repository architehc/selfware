"""Tests for SWE-bench Pro patch utilities."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

from patch_utils import (
    extract_partial_diff,
    filter_patch_excluding_paths,
    filter_patch_to_source_files,
    paths_from_patch,
)


COMPLETE_TWO_HUNK_DIFF = """\
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -10,7 +10,7 @@ fn old_name() -> i32 {
-    1
+    2
 }

 fn unchanged() {}
@@ -25,3 +25,4 @@ fn another() {
     3
+    4
 }
"""

TRAILING_INCOMPLETE_HUNK_DIFF = """\
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -10,7 +10,7 @@ fn old_name() -> i32 {
-    1
+    2
 }

@@ -25,3 +25,4 @@ fn another() {
"""

MIXED_SOURCE_AND_TEST_DIFF = """\
diff --git a/src/widget.py b/src/widget.py
--- a/src/widget.py
+++ b/src/widget.py
@@ -1,3 +1,3 @@
 def widget():
-    return 1
+    return 2

diff --git a/test_widget.py b/test_widget.py
--- /dev/null
+++ b/test_widget.py
@@ -0,0 +1,3 @@
+def test_widget():
+    assert widget() == 2
"""


def test_extract_partial_diff_keeps_complete_hunks():
    result = extract_partial_diff(COMPLETE_TWO_HUNK_DIFF)
    assert result is not None
    assert result.count("@@") == 4  # two hunks, each header has two @@
    assert "fn old_name" in result
    assert "fn another" in result


def test_extract_partial_diff_drops_incomplete_trailing_hunk():
    result = extract_partial_diff(TRAILING_INCOMPLETE_HUNK_DIFF)
    assert result is not None
    assert result.count("@@") == 2  # only first hunk kept
    assert "fn old_name" in result
    assert "fn another" not in result


def test_extract_partial_diff_returns_none_for_no_hunks():
    assert extract_partial_diff("just some prose\n") is None


def test_extract_partial_diff_keeps_last_hunk_without_final_newline():
    # The diff ends immediately after the last hunk body; it is complete.
    diff = COMPLETE_TWO_HUNK_DIFF.rstrip("\n")
    result = extract_partial_diff(diff)
    assert result is not None
    assert "fn another" in result


def test_paths_from_patch_extracts_target_paths():
    patch = """\
diff --git a/src/a.py b/src/a.py
--- a/src/a.py
+++ b/src/a.py
@@ -1 +1 @@
-old
+new

diff --git a/b/c.go b/b/c.go
new file mode 100644
--- /dev/null
+++ b/b/c.go
@@ -0,0 +1 @@
+package c
"""
    assert paths_from_patch(patch) == {"src/a.py", "b/c.go"}


def test_paths_from_patch_returns_empty_set_for_empty_patch():
    assert paths_from_patch("") == set()


def test_filter_patch_excluding_paths_drops_excluded_hunks():
    result = filter_patch_excluding_paths(
        MIXED_SOURCE_AND_TEST_DIFF, {"test_widget.py"}
    )
    assert "src/widget.py" in result
    assert "test_widget.py" not in result


def test_filter_patch_excluding_paths_returns_unchanged_when_no_exclusions():
    result = filter_patch_excluding_paths(MIXED_SOURCE_AND_TEST_DIFF, set())
    assert "src/widget.py" in result
    assert "test_widget.py" in result


def test_filter_patch_to_source_files_excludes_test_patch_paths():
    # The test file from the official test patch must be dropped even though it
    # is a source-like path, while the legitimate source edit is kept.
    result = filter_patch_to_source_files(
        MIXED_SOURCE_AND_TEST_DIFF,
        test_patch_paths={"test_widget.py"},
    )
    assert "src/widget.py" in result
    assert "test_widget.py" not in result


def test_filter_patch_to_source_files_drops_test_files_by_default():
    result = filter_patch_to_source_files(MIXED_SOURCE_AND_TEST_DIFF)
    # Without test_patch_paths, the generic test-file filter still drops it.
    assert "test_widget.py" not in result
