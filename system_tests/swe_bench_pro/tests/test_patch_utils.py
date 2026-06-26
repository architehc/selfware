"""Tests for SWE-bench Pro patch utilities."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))

from patch_utils import extract_partial_diff


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
